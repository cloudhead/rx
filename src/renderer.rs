use crate::brush::{Brush, BrushMode, BrushState};
use crate::color;
use crate::data;
use crate::font::{Font, TextBatch};
use crate::framebuffer2d;
use crate::gpu;
use crate::image;
use crate::platform::{self, LogicalSize};
use crate::resources::ResourceManager;
use crate::screen2d;
use crate::session::{
    self, Effect, ExecutionMode, Mode, Rgb8, Session, Tool, VisualMode,
};
use crate::view::{View, ViewId, ViewManager, ViewOp};

use rgx::core;
use rgx::core::{Blending, Filter, Op, PassOp, Rect, Rgba};
use rgx::kit;
use rgx::kit::shape2d;
use rgx::kit::shape2d::{Fill, Line, Shape, Stroke};
use rgx::kit::sprite2d;
use rgx::kit::{Bgra8, Origin, Rgba8};
use rgx::math::{Matrix4, Vector2};

use std::collections::BTreeMap;
use std::time;

/// 2D Renderer. Renders the [`Session`] to screen.
pub struct Renderer {
    /// Window size.
    window: LogicalSize,
    /// The font used to render text.
    font: Font,
    cursors: Cursors,
    checker: Checker,
    /// View transforms. These are sorted by [`ViewId`].
    view_transforms: Vec<Matrix4<f32>>,
    /// View transform buffer, created from the transform matrices. This is bound
    /// as a dynamic uniform buffer, to render all views in a single pass.
    view_transforms_buf: gpu::TransformBuffer,
    /// Sampler used for literally everything.
    sampler: core::Sampler,

    /// Pipeline for shapes, eg. UI elements.
    shape2d: kit::shape2d::Pipeline,
    /// Pipeline for sprites, eg. text and views.
    sprite2d: kit::sprite2d::Pipeline,
    /// Pipeline for off-screen rendering.
    framebuffer2d: framebuffer2d::Pipeline,
    /// Pipeline for brush strokes.
    brush2d: kit::shape2d::Pipeline,
    /// Pipeline for eraser strokes and other use-cases that require
    /// "constant" blending.
    const2d: kit::shape2d::Pipeline,
    /// Pipeline for pasting to the view.
    paste2d: kit::sprite2d::Pipeline,

    /// Pipeline used to render to the screen/window.
    screen2d: screen2d::Pipeline,
    /// Screen framebuffer. Everything seen by the user is rendered here first.
    /// This allows us to do things like UI scaling.
    screen_fb: core::Framebuffer,
    screen_vb: core::VertexBuffer,
    screen_binding: core::BindingGroup,

    /// Resources shared between the renderer and session.
    resources: ResourceManager,

    /// View data, such as buffers, bindings etc.
    view_data: BTreeMap<ViewId, ViewData>,

    /// Paste buffer.
    paste: Paste,
}

/// Paste buffer.
struct Paste {
    binding: core::BindingGroup,
    texture: core::Texture,
    outputs: Vec<core::VertexBuffer>,
}

struct Checker {
    binding: core::BindingGroup,
    texture: core::Texture,
}

impl Checker {
    fn rect() -> Rect<f32> {
        Rect::origin(2., 2.)
    }
}

struct Cursors {
    binding: core::BindingGroup,
    texture: core::Texture,
}

impl Cursors {
    fn offset(t: &Tool) -> Vector2<f32> {
        match t {
            Tool::Sampler => Vector2::new(1., 1.),
            Tool::Brush(_) => Vector2::new(-8., -8.),
            Tool::Pan => Vector2::new(0., 0.),
        }
    }

    fn rect(t: &Tool) -> Option<Rect<f32>> {
        match t {
            Tool::Sampler => Some(Rect::new(0., 0., 16., 16.)),
            Tool::Brush(_) => Some(Rect::new(16., 0., 32., 16.)),
            Tool::Pan => None,
        }
    }
}

/// View data used for rendering.
struct ViewData {
    /// View framebuffer. Brush strokes and edits are written to this buffer.
    fb: core::Framebuffer,
    /// View staging framebuffer. Brush strokes are rendered here first, before
    /// being rendered to the "real" framebuffer.
    staging_fb: core::Framebuffer,
    /// Vertex buffer. This holds the vertices that form the view quad.
    vb: core::VertexBuffer,
    /// Texture/sampler binding for the "real" framebuffer.
    binding: core::BindingGroup,
    /// Texture/sampler binding for the staging framebuffer.
    staging_binding: core::BindingGroup,
    /// Animation quad.
    anim_vb: Option<core::VertexBuffer>,
    /// Animation texture/sampler binding.
    anim_binding: core::BindingGroup,
}

impl ViewData {
    fn new(
        w: u32,
        h: u32,
        framebuffer2d: &framebuffer2d::Pipeline,
        sprite2d: &sprite2d::Pipeline,
        r: &core::Renderer,
    ) -> Self {
        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);
        let vb = framebuffer2d::Pipeline::vertex_buffer(w, h, r);

        let fb = r.framebuffer(w, h);
        let binding = framebuffer2d.binding(r, &fb, &sampler);

        let staging_fb = r.framebuffer(w, h);
        let staging_binding = framebuffer2d.binding(r, &staging_fb, &sampler);

        let anim_binding = sprite2d.binding(r, &fb.texture, &sampler);

        ViewData {
            fb,
            vb,
            binding,
            staging_fb,
            staging_binding,
            anim_vb: None,
            anim_binding,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

const GLYPH_WIDTH: f32 = 8.;
const GLYPH_HEIGHT: f32 = 14.;

const LINE_HEIGHT: f32 = GLYPH_HEIGHT + 4.;
const MARGIN: f32 = 10.;

impl Renderer {
    pub fn new(
        r: &mut core::Renderer,
        window: LogicalSize,
        resources: ResourceManager,
    ) -> Self {
        let (win_w, win_h) = (window.width as u32, window.height as u32);

        let sprite2d: kit::sprite2d::Pipeline = r.pipeline(Blending::default());
        let shape2d: kit::shape2d::Pipeline = r.pipeline(Blending::default());
        let framebuffer2d: framebuffer2d::Pipeline =
            r.pipeline(Blending::default());
        let screen2d: screen2d::Pipeline = r.pipeline(Blending::default());

        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);

        let view_transforms_buf = gpu::TransformBuffer::with_capacity(
            Session::MAX_VIEWS,
            &framebuffer2d.pipeline.layout.sets[1],
            &r.device,
        );
        let view_transforms = Vec::with_capacity(Session::MAX_VIEWS);

        let (font, font_img) = {
            let (img, width, height) = image::decode(data::GLYPHS).unwrap();
            let texture = r.texture(width, height);
            let binding = sprite2d.binding(r, &texture, &sampler);

            (
                Font::new(
                    texture,
                    binding,
                    self::GLYPH_WIDTH,
                    self::GLYPH_HEIGHT,
                ),
                img,
            )
        };
        let (cursors, cursors_img) = {
            let (img, width, height) = image::decode(data::CURSORS).unwrap();
            let texture = r.texture(width, height);
            let binding = sprite2d.binding(r, &texture, &sampler);

            (Cursors { texture, binding }, img)
        };
        let (checker, checker_img) = {
            #[rustfmt::skip]
            let texels: [u8; 16] = [
                0x55, 0x55, 0x55, 0xff,
                0x66, 0x66, 0x66, 0xff,
                0x66, 0x66, 0x66, 0xff,
                0x55, 0x55, 0x55, 0xff,
            ];
            let texture = r.texture(2, 2);
            let binding = sprite2d.binding(r, &texture, &sampler);

            (Checker { texture, binding }, texels)
        };

        let brush2d = r.pipeline(Blending::default());
        let const2d = r.pipeline(Blending::constant());
        let paste2d: sprite2d::Pipeline = r.pipeline(Blending::default());

        let paste = {
            let texture = r.texture(1, 1);
            let binding = paste2d.binding(r, &texture, &sampler);
            Paste {
                texture,
                binding,
                outputs: Vec::new(),
            }
        };

        let screen_vb = screen2d::Pipeline::vertex_buffer(r);
        let screen_fb = r.framebuffer(win_w, win_h);
        let screen_binding = screen2d.binding(r, &screen_fb, &sampler);

        r.submit(&[
            Op::Fill(&font.texture, Rgba8::align(&font_img)),
            Op::Fill(&cursors.texture, Rgba8::align(&cursors_img)),
            Op::Fill(&checker.texture, Rgba8::align(&checker_img)),
        ]);

        Self {
            window,
            font,
            cursors,
            checker,
            view_transforms,
            view_transforms_buf,
            sampler,
            shape2d,
            sprite2d,
            framebuffer2d,
            brush2d,
            const2d,
            paste2d,
            screen2d,
            resources,
            screen_fb,
            screen_vb,
            screen_binding,
            view_data: BTreeMap::new(),
            paste,
        }
    }

    pub fn init(
        &mut self,
        effects: Vec<Effect>,
        views: &ViewManager,
        r: &mut core::Renderer,
    ) {
        self.handle_effects(effects, &views, r);
    }

    fn render_help(
        &self,
        session: &Session,
        r: &mut core::Renderer,
        textures: &mut core::SwapChain,
    ) {
        let out = &textures.next();
        let mut text = TextBatch::new(&self.font);
        let column_offset = self::GLYPH_WIDTH * 16.;
        let left_margin = self::MARGIN * 2.;

        text.add(
            &format!(
                "rx v{}: help ({} to exit)",
                crate::VERSION,
                platform::Key::Escape,
            ),
            left_margin,
            self.window.height as f32 - self::MARGIN - self::LINE_HEIGHT,
            color::LIGHT_GREY,
        );

        for (i, (display, kb)) in session
            .key_bindings
            .iter()
            .filter_map(|kb| kb.display.as_ref().map(|d| (d, kb)))
            .enumerate()
        {
            let y =
                self.window.height as f32 - (i + 4) as f32 * self::LINE_HEIGHT;

            text.add(display, left_margin, y, color::RED);
            text.add(
                &format!("{}", kb.command),
                left_margin + column_offset,
                y,
                color::LIGHT_GREY,
            );
        }
        for (i, l) in session::HELP.lines().enumerate() {
            let y =
                self.window.height as f32 - (i + 4) as f32 * self::LINE_HEIGHT;

            text.add(
                l,
                left_margin + column_offset * 3. + 64.,
                y,
                color::LIGHT_GREEN,
            );
        }
        let buf = text.finish(&r);

        let mut f = r.frame();

        r.update_pipeline(
            &self.sprite2d,
            kit::ortho(out.width, out.height),
            &mut f,
        );

        {
            let mut p =
                f.pass(PassOp::Clear(Rgba::TRANSPARENT), &self.screen_fb);

            p.set_pipeline(&self.sprite2d);
            p.draw(&buf, &self.font.binding);
        }
        {
            let mut p = f.pass(PassOp::Clear(Rgba::TRANSPARENT), out);

            p.set_pipeline(&self.screen2d);
            p.set_binding(&self.screen_binding, &[]);
            p.draw_buffer(&self.screen_vb)
        }
        // Submit frame for presenting.
        r.present(f);
    }

    pub fn frame(
        &mut self,
        session: &Session,
        effects: Vec<session::Effect>,
        avg_frametime: &time::Duration,
        r: &mut core::Renderer,
        textures: &mut core::SwapChain,
    ) {
        if session.state != session::State::Running {
            return;
        }
        if session.mode == Mode::Help {
            self.render_help(session, r, textures);
            return;
        }

        // Handle effects produces by the session.
        self.handle_effects(effects, &session.views, r);

        // Handle view operations.
        for v in session.views.values() {
            if !v.ops.is_empty() {
                self.handle_view_ops(&v, r);
            }
        }

        let present = &textures.next();

        let mut ui_batch = shape2d::Batch::new();
        let mut palette_batch = shape2d::Batch::new();
        let mut text_batch = TextBatch::new(&self.font);
        let mut cursor_batch = sprite2d::Batch::new(
            self.cursors.texture.w,
            self.cursors.texture.h,
        );
        let mut paste_batch =
            sprite2d::Batch::new(self.paste.texture.w, self.paste.texture.h);
        let mut checker_batch = sprite2d::Batch::new(
            self.checker.texture.w,
            self.checker.texture.h,
        );
        let mut paint_batch = shape2d::Batch::new();

        if let Tool::Brush(ref b) = session.tool {
            for shape in b
                .output(
                    Stroke::NONE,
                    Fill::Solid(b.color.into()),
                    1.0,
                    Origin::BottomLeft,
                )
                .iter()
                .cloned()
            {
                paint_batch.add(shape);
            }
        }

        Self::draw_brush(&session, &mut ui_batch);
        Self::draw_paste(&session, &self.paste, &mut paste_batch);
        Self::draw_ui(&session, avg_frametime, &mut ui_batch, &mut text_batch);
        Self::draw_palette(&session, &mut palette_batch);
        Self::draw_cursor(&session, &mut cursor_batch);
        Self::draw_checker(&session, &mut checker_batch);

        let ui_buf = ui_batch.finish(&r);
        let palette_buf = palette_batch.finish(&r);
        let cursor_buf = cursor_batch.finish(&r);
        let checker_buf = checker_batch.finish(&r);
        let text_buf = text_batch.finish(&r);
        let paint_buf = if paint_batch.is_empty() {
            None
        } else {
            Some(paint_batch.finish(&r))
        };
        let paste_buf = if paste_batch.size > 0 {
            Some(paste_batch.finish(&r))
        } else {
            None
        };

        // Start the render frame.
        let mut f = r.frame();

        self.update_view_animations(session, r);
        self.update_view_transforms(
            session.views.values(),
            session.offset,
            &r,
            &mut f,
        );

        let v = session.active_view();
        let view_data = self
            .view_data
            .get(&v.id)
            .expect("the view data for the active view must exist");

        r.update_pipeline(
            &self.shape2d,
            kit::ortho(present.width, present.height),
            &mut f,
        );
        r.update_pipeline(
            &self.sprite2d,
            kit::ortho(present.width, present.height),
            &mut f,
        );
        r.update_pipeline(
            &self.framebuffer2d,
            kit::ortho(present.width, present.height),
            &mut f,
        );

        {
            let mut p =
                f.pass(PassOp::Clear(Rgba::TRANSPARENT), &self.screen_fb);

            // Draw view checkers.
            if session.settings["checker"].is_set() {
                p.set_pipeline(&self.sprite2d);
                p.draw(&checker_buf, &self.checker.binding);
            }
        }

        // Render brush strokes to view framebuffers.
        if let Some(ref paint_buf) = paint_buf {
            if let Tool::Brush(ref b) = session.tool {
                if b.state != BrushState::NotDrawing {
                    r.update_pipeline(
                        &self.brush2d,
                        kit::ortho(v.width(), v.height()),
                        &mut f,
                    );
                    r.update_pipeline(
                        &self.const2d,
                        kit::ortho(v.width(), v.height()),
                        &mut f,
                    );

                    self.render_brush_strokes(paint_buf, view_data, b, &mut f);
                }
            }
        }

        // Draw paste buffer to view staging buffer.
        if let Some(buf) = paste_buf {
            r.update_pipeline(
                &self.paste2d,
                kit::ortho(v.width(), v.height()),
                &mut f,
            );

            let mut p =
                f.pass(PassOp::Clear(Rgba::TRANSPARENT), &view_data.staging_fb);

            p.set_pipeline(&self.paste2d);
            p.draw(&buf, &self.paste.binding);
        }

        // Draw paste buffer to view framebuffer.
        if !self.paste.outputs.is_empty() {
            let mut p = f.pass(PassOp::Load(), &view_data.fb);

            p.set_pipeline(&self.paste2d);

            for out in self.paste.outputs.drain(..) {
                p.draw(&out, &self.paste.binding);
            }
        }

        {
            r.update_pipeline(
                &self.sprite2d,
                kit::ortho(present.width, present.height) * session.transform(),
                &mut f,
            );

            // NOTE: We should be able to use the same render pass for both
            // of the following operations, but strangely enough, this yields
            // validation errors around the dynamic buffer offsets. I'm pretty
            // sure that this is a bug in wgpu, which perhaps doesn't reset
            // the dynamic offset count when the pipeline is switched.

            {
                // Draw view framebuffers to screen framebuffer.
                let mut p = f.pass(PassOp::Load(), &self.screen_fb);
                self.render_views(&mut p);
            }

            // Draw view animations to screen framebuffer.
            if session.settings["animation"].is_set() {
                let mut p = f.pass(PassOp::Load(), &self.screen_fb);
                self.render_view_animations(&session.views, &mut p);
            }
        }

        {
            r.update_pipeline(
                &self.sprite2d,
                kit::ortho(present.width, present.height),
                &mut f,
            );

            let mut p = f.pass(PassOp::Load(), &self.screen_fb);

            // Draw UI elements to screen framebuffer.
            p.set_pipeline(&self.shape2d);
            p.draw_buffer(&ui_buf);

            // Draw text to screen framebuffer.
            p.set_pipeline(&self.sprite2d);
            p.draw(&text_buf, &self.font.binding);

            // Draw palette to screen framebuffer.
            p.set_pipeline(&self.shape2d);
            p.draw_buffer(&palette_buf);

            // Draw cursor to screen framebuffer.
            p.set_pipeline(&self.sprite2d);
            p.draw(&cursor_buf, &self.cursors.binding);
        }

        {
            // Present screen framebuffer to screen.
            let bg = Rgba::from(session.settings["background"].color());
            let mut p = f.pass(PassOp::Clear(bg), present);

            p.set_pipeline(&self.screen2d);
            p.set_binding(&self.screen_binding, &[]);
            p.draw_buffer(&self.screen_vb)
        }

        // Submit frame to device.
        r.present(f);

        // Always clear the active view staging buffer. We do this because
        // it may not get drawn to this frame, and hence may remain dirty
        // from a previous frame.
        //
        // We have to do this *after* the frame has been submitted, otherwise
        // when switching views, the staging buffer isn't cleared.
        r.submit(&[Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT)]);

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            let id = v.id;
            let extent = v.extent();
            let resources = self.resources.clone();

            r.read(&view_data.fb, move |data| {
                if let Some(s) = resources.lock_mut().get_view_mut(&id) {
                    // TODO: This function should just take a `ViewExtent`.
                    s.push_snapshot(data, extent.fw, extent.fh, extent.nframes);
                }
            });
        }
    }

    fn handle_effects(
        &mut self,
        mut effects: Vec<Effect>,
        views: &ViewManager,
        r: &mut core::Renderer,
    ) {
        for eff in effects.drain(..) {
            debug!("handling: {:?}", eff);

            // When switching views, or when the view is dirty (eg. it has been resized),
            // we have to resize the brush pipelines, for the brush strokes to
            // render properly in the view framebuffer. When a snapshot is restored,
            // the view size might also have changed, and therefore we resize
            // on "damaged" too.
            match eff {
                Effect::SessionResized(size) => {
                    self.handle_resized(size, r);
                }
                Effect::ViewActivated(_) => {}
                Effect::ViewAdded(id) => {
                    self.add_views(&[id], r);
                }
                Effect::ViewRemoved(id) => {
                    self.view_data.remove(&id);
                }
                Effect::ViewTouched(id) | Effect::ViewDamaged(id) => {
                    let v = views.get(&id).expect("view must exist");
                    self.handle_view_dirty(v, r);
                }
            }
        }
    }

    fn handle_view_dirty(&mut self, v: &View, r: &mut core::Renderer) {
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        let (vw, vh) = (v.width(), v.height());

        if fb.width() != vw || fb.height() != vh {
            // View size changed. Re-create view resources.
            // This condition is triggered when the size of the view doesn't match the size
            // of the view framebuffer. This can happen in two cases:
            //
            //   1. The view was resized (it's dirty).
            //   2. A snapshot was restored with a different size than the view (it's damaged).
            //
            // Either way, we handle it equally, by re-creating the view-data and restoring
            // the current snapshot.
            let view_data =
                ViewData::new(vw, vh, &self.framebuffer2d, &self.sprite2d, r);

            // We don't want the lock to be held when `submit` is called below,
            // because in some cases it'll trigger the read-back which claims
            // a write lock on resources.
            let (sw, sh, pixels) = {
                let resources = self.resources.lock();
                let (snapshot, pixels) = resources.get_snapshot(&v.id);
                (snapshot.width(), snapshot.height(), pixels.to_owned())
            };

            // Ensure not to transfer more data than can fit
            // in the view buffer.
            let tw = u32::min(sw, vw);
            let th = u32::min(sh, vh);

            r.submit(&[
                Op::Clear(&view_data.fb, Bgra8::TRANSPARENT),
                Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT),
                Op::Transfer(
                    &view_data.fb,
                    &*pixels,
                    sw, // Source width
                    sh, // Source height
                    Rect::origin(tw as i32, th as i32),
                ),
            ]);
            self.view_data.insert(v.id, view_data);
        } else if v.is_damaged() {
            // View is damaged, but its size hasn't changed. This happens when a snapshot
            // with the same size as the view was restored.
            let pixels = {
                let rs = self.resources.lock();
                let (_, pixels) = rs.get_snapshot(&v.id);
                pixels.to_owned()
            };
            r.submit(&[Op::Fill(fb, &*pixels)]);
        }
    }

    fn handle_view_ops(&mut self, v: &View, r: &mut core::Renderer) {
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        for op in &v.ops {
            match op {
                ViewOp::Clear(color) => {
                    r.submit(&[Op::Clear(fb, (*color).into())]);
                }
                ViewOp::Blit(src, dst) => {
                    r.submit(&[Op::Blit(fb, *src, *dst)]);
                }
                ViewOp::Yank(src) => {
                    let resources = self.resources.lock();
                    let (snapshot, pixels) = resources.get_snapshot(&v.id);

                    let w = src.width() as usize;
                    let h = src.height() as usize;

                    let total_w = snapshot.width() as usize;
                    let total_h = snapshot.height() as usize;

                    let mut buffer: Vec<Bgra8> = Vec::with_capacity(w * h);

                    for y in (src.y1 as usize..src.y2 as usize).rev() {
                        let y = total_h - y - 1;
                        let offset = y * total_w + src.x1 as usize;
                        let row = &pixels[offset..offset + w];

                        buffer.extend_from_slice(row);
                    }
                    let mut pixels: Vec<Rgba8> =
                        Vec::with_capacity(buffer.len());
                    for c in buffer.into_iter() {
                        pixels.push(c.into());
                    }
                    assert!(pixels.len() == w * h);

                    self.paste.texture = r.texture(w as u32, h as u32);
                    self.paste.binding = self.paste2d.binding(
                        r,
                        &self.paste.texture,
                        &self.sampler,
                    );

                    r.submit(&[Op::Fill(&self.paste.texture, &pixels)]);
                }
                ViewOp::Paste(dst) => {
                    let buffer = sprite2d::Batch::singleton(
                        self.paste.texture.w,
                        self.paste.texture.h,
                        self.paste.texture.rect(),
                        dst.map(|n| n as f32),
                        Rgba::TRANSPARENT,
                        1.,
                        kit::Repeat::default(),
                    )
                    .finish(&r);

                    self.paste.outputs.push(buffer);
                }
            }
        }
    }

    fn add_views(&mut self, views: &[ViewId], r: &mut core::Renderer) {
        for id in views {
            let resources = self.resources.lock();
            let (s, pixels) = resources.get_snapshot(id);
            let (w, h) = (s.width(), s.height());

            let view_data =
                ViewData::new(w, h, &self.framebuffer2d, &self.sprite2d, r);

            debug_assert!(!pixels.is_empty());
            r.submit(&[
                Op::Clear(&view_data.fb, Bgra8::TRANSPARENT),
                Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT),
                Op::Fill(&view_data.fb, &pixels),
            ]);

            self.view_data.insert(*id, view_data);
        }
    }

    fn draw_ui(
        session: &Session,
        avg_frametime: &time::Duration,
        canvas: &mut shape2d::Batch,
        text: &mut TextBatch,
    ) {
        let view = session.active_view();

        if let Some(selection) = session.selection {
            let fill = match session.mode {
                Mode::Visual(VisualMode::Selecting) => {
                    Rgba8::new(color::RED.r, color::RED.g, color::RED.b, 0x88)
                }
                // TODO: Handle different modes differently.
                _ => Rgba8::TRANSPARENT,
            };
            let stroke = color::RED;

            let r = selection.bounds();
            let offset = session.offset + view.offset;

            {
                // Selection dimensions.
                let s = selection;
                let z = view.zoom;
                let t = format!("{}x{}", r.width(), r.height());
                let x = if s.x2 > s.x1 {
                    (s.x2 + 1) as f32 * z - t.len() as f32 * self::GLYPH_WIDTH
                } else {
                    (s.x2 as f32) * z
                };
                let y = if s.y2 >= s.y1 {
                    (s.y2 + 1) as f32 * z + 1.
                } else {
                    (s.y2) as f32 * z - self::LINE_HEIGHT + 1.
                };
                text.add(&t, x + offset.x, y + offset.y, stroke);
            }

            // Selection stroke.
            canvas.add(Shape::Rectangle(
                r.map(|n| n as f32) * view.zoom + offset,
                Stroke::new(1., stroke.into()),
                Fill::Empty(),
            ));
            // Selection fill.
            canvas.add(Shape::Rectangle(
                r.clamped(Rect::origin(
                    view.width() as i32,
                    view.height() as i32,
                ))
                .map(|n| n as f32)
                    * view.zoom
                    + offset,
                Stroke::NONE,
                Fill::Solid(fill.into()),
            ));
        }

        for (id, v) in session.views.iter() {
            let offset = v.offset + session.offset;

            // Frame lines
            for n in 1..v.animation.len() {
                let n = n as f32;
                let x = n * v.zoom * v.fw as f32 + offset.x;
                canvas.add(Shape::Line(
                    Line::new(x, offset.y, x, v.zoom * v.fh as f32 + offset.y),
                    Stroke::new(1.0, Rgba::new(0.4, 0.4, 0.4, 1.0)),
                ));
            }
            // View border
            let r = v.rect();
            let border_color = if session.is_active(&id) {
                Rgba::WHITE
            } else {
                Rgba::new(0.5, 0.5, 0.5, 1.0)
            };
            canvas.add(Shape::Rectangle(
                Rect::new(r.x1 - 1., r.y1 - 1., r.x2 + 1., r.y2 + 1.)
                    + session.offset,
                Stroke::new(1.0, border_color),
                Fill::Empty(),
            ));

            // View info
            text.add(
                &format!("{}x{}x{}", v.fw, v.fh, v.animation.len()),
                offset.x,
                offset.y - self::LINE_HEIGHT,
                color::GREY,
            );
        }

        if session.settings["debug"].is_set() {
            let mem = crate::ALLOCATOR.allocated();

            // Frame-time
            let txt = &format!(
                "{:3.2}ms {:3.2}ms {}MB {}KB",
                avg_frametime.as_micros() as f64 / 1000.,
                session.avg_time.as_micros() as f64 / 1000.,
                mem / (1024 * 1024),
                mem / 1024 % (1024),
            );
            text.add(
                txt,
                MARGIN,
                session.height - MARGIN - self::LINE_HEIGHT,
                Rgba8::WHITE,
            );
        }

        // Active view status
        text.add(
            &view.status(),
            MARGIN,
            MARGIN + self::LINE_HEIGHT,
            Rgba8::WHITE,
        );

        if let ExecutionMode::Recording(_, path) = &session.execution {
            text.add(
                &format!(
                    "* recording: {} (<Ctrl-Esc> to stop)",
                    path.display()
                ),
                MARGIN * 2.,
                session.height - self::LINE_HEIGHT - MARGIN,
                color::RED,
            );
        } else if let ExecutionMode::Replaying(_, path) = &session.execution {
            text.add(
                &format!("> replaying: {} (<Esc> to stop)", path.display()),
                MARGIN * 2.,
                session.height - self::LINE_HEIGHT - MARGIN,
                color::LIGHT_GREEN,
            );
        }

        {
            // Session status
            text.add(
                &format!("{:>5}%", (view.zoom * 100.) as u32),
                session.width - MARGIN - 6. * 8.,
                MARGIN + self::LINE_HEIGHT,
                Rgba8::WHITE,
            );

            if session.width >= 400. {
                let cursor = session.view_coords(view.id, session.cursor);
                let hover_color = session
                    .hover_color
                    .map_or(String::new(), |c| Rgb8::from(c).to_string());
                text.add(
                    &format!("{:>4},{:<4} {}", cursor.x, cursor.y, hover_color),
                    session.width * 0.5,
                    MARGIN + self::LINE_HEIGHT,
                    Rgba8::WHITE,
                );

                if session.width >= 600. {
                    // Fg color
                    canvas.add(Shape::Rectangle(
                        Rect::origin(11., 11.).with_origin(
                            session.width * 0.4,
                            self::LINE_HEIGHT + self::MARGIN + 2.,
                        ),
                        Stroke::new(1.0, Rgba::WHITE),
                        Fill::Solid(session.fg.into()),
                    ));
                    // Bg color
                    canvas.add(Shape::Rectangle(
                        Rect::origin(11., 11.).with_origin(
                            session.width * 0.4 + 25.,
                            self::LINE_HEIGHT + self::MARGIN + 2.,
                        ),
                        Stroke::new(1.0, Rgba::WHITE),
                        Fill::Solid(session.bg.into()),
                    ));
                }
            }
        }

        // Command-line & message
        if session.mode == Mode::Command {
            let s = format!("{}", &session.cmdline.input());
            text.add(&s, MARGIN, MARGIN, Rgba8::WHITE);
        } else {
            let s = format!("{}", &session.message);
            text.add(&s, MARGIN, MARGIN, session.message.color());
        }
    }

    fn draw_palette(session: &Session, batch: &mut shape2d::Batch) {
        let p = &session.palette;
        for (i, color) in p.colors.iter().cloned().enumerate() {
            let x = if i >= 16 { p.cellsize } else { 0. };
            let y = (i % 16) as f32 * p.cellsize;

            let mut stroke = shape2d::Stroke::NONE;
            if let Some(c) = p.hover {
                if c == color {
                    stroke = shape2d::Stroke::new(1., Rgba::WHITE);
                }
            }

            batch.add(Shape::Rectangle(
                Rect::new(
                    p.x + x,
                    p.y + y,
                    p.x + x + p.cellsize,
                    p.y + y + p.cellsize,
                ),
                stroke,
                shape2d::Fill::Solid(color.into()),
            ));
        }
    }

    fn draw_checker(session: &Session, batch: &mut sprite2d::Batch) {
        if session.settings["checker"].is_set() {
            for (_, v) in session.views.iter() {
                let ratio = v.width() as f32 / v.height() as f32;
                let rx = v.zoom * ratio * 2.;
                let ry = v.zoom * 2.;

                batch.add(
                    Checker::rect(),
                    v.rect() + session.offset,
                    Rgba::TRANSPARENT,
                    1.,
                    kit::Repeat::new(rx, ry),
                );
            }
        }
    }

    fn draw_cursor(session: &Session, batch: &mut sprite2d::Batch) {
        // TODO: Cursor should be greyed out in command mode.
        match session.mode {
            Mode::Present | Mode::Help => {}
            Mode::Visual(_) => {
                if let Some(rect) = Cursors::rect(&Tool::default()) {
                    let offset = Cursors::offset(&Tool::default());
                    let cursor = session.cursor;
                    batch.add(
                        rect,
                        rect.with_origin(cursor.x, cursor.y) + offset,
                        Rgba::TRANSPARENT,
                        1.,
                        kit::Repeat::default(),
                    );
                }
            }
            Mode::Normal | Mode::Command => {
                // When hovering over the palette, switch to the sampler icon
                // to tell the user that clicking will select the color.
                let tool = if session.palette.hover.is_some() {
                    &Tool::Sampler
                } else {
                    &session.tool
                };

                if let Some(rect) = Cursors::rect(tool) {
                    let offset = Cursors::offset(tool);
                    let cursor = session.cursor;
                    batch.add(
                        rect,
                        rect.with_origin(cursor.x, cursor.y) + offset,
                        Rgba::TRANSPARENT,
                        1.,
                        kit::Repeat::default(),
                    );
                }
            }
        }
    }

    fn draw_brush(session: &Session, shapes: &mut shape2d::Batch) {
        if session.palette.hover.is_some() {
            return;
        }
        let v = session.active_view();
        let c = session.cursor;

        match session.mode {
            Mode::Visual(VisualMode::Selecting) => {
                if v.contains(c - session.offset) {
                    let z = v.zoom;
                    let c = session.snap(c, v.offset.x, v.offset.y, z);
                    shapes.add(Shape::Rectangle(
                        Rect::new(c.x, c.y, c.x + z, c.y + z),
                        Stroke::new(1.0, color::RED.into()),
                        Fill::Empty(),
                    ));
                }
            }
            Mode::Normal => {
                if let Tool::Brush(ref brush) = session.tool {
                    // Draw enabled brush
                    if v.contains(c - session.offset) {
                        let (stroke, fill) = if brush.is_set(BrushMode::Erase) {
                            (Stroke::new(1.0, Rgba::WHITE), Fill::Empty())
                        } else {
                            (Stroke::NONE, Fill::Solid(session.fg.into()))
                        };

                        let view_coords =
                            session.active_view_coords(session.cursor);
                        for p in brush.expand(view_coords.into(), v.extent()) {
                            shapes.add(brush.shape(
                                *session.session_coords(v.id, p.into()),
                                stroke,
                                fill,
                                v.zoom,
                                Origin::BottomLeft,
                            ));
                        }
                    // Draw disabled brush
                    } else {
                        let color = if brush.is_set(BrushMode::Erase) {
                            color::GREY
                        } else {
                            session.fg
                        };
                        shapes.add(brush.shape(
                            *c,
                            Stroke::new(1.0, color.into()),
                            Fill::Empty(),
                            v.zoom,
                            Origin::Center,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    fn draw_paste(
        session: &Session,
        paste: &Paste,
        batch: &mut sprite2d::Batch,
    ) {
        if let Mode::Visual(VisualMode::Pasting) = session.mode {
            if let Some(s) = session.selection {
                batch.add(
                    paste.texture.rect(),
                    Rect::new(
                        s.x1 as f32,
                        s.y1 as f32,
                        s.x2 as f32 + 1.,
                        s.y2 as f32 + 1.,
                    ),
                    Rgba::TRANSPARENT,
                    0.9,
                    kit::Repeat::default(),
                );
            }
        }
    }

    fn render_views(&self, p: &mut core::Pass) {
        p.set_pipeline(&self.framebuffer2d);

        for ((_, v), off) in self
            .view_data
            .iter()
            .zip(self.view_transforms_buf.iter_offset())
        {
            // FIXME: (rgx) Why is it that ommitting this line yields an obscure error
            // message?
            p.set_binding(&self.view_transforms_buf.binding, &[off]);
            p.set_binding(&v.binding, &[]);
            p.draw_buffer(&v.vb);

            p.set_binding(&v.staging_binding, &[]);
            p.draw_buffer(&v.vb);
        }
    }

    fn render_view_animations(&self, views: &ViewManager, p: &mut core::Pass) {
        p.set_pipeline(&self.sprite2d);

        for (id, v) in self.view_data.iter() {
            if let Some(ref vb) = v.anim_vb {
                if let Some(view) = views.get(id) {
                    if view.animation.len() > 1 {
                        p.draw(vb, &v.anim_binding);
                    }
                }
            }
        }
    }

    fn render_brush_strokes(
        &self,
        paint_buf: &core::VertexBuffer,
        view_data: &ViewData,
        brush: &Brush,
        f: &mut core::Frame,
    ) {
        debug_assert!(brush.state != BrushState::NotDrawing);

        let ViewData { fb, staging_fb, .. } = view_data;

        let mut p = match brush.state {
            // If we're erasing, we can't use the staging framebuffer, since we
            // need to be replacing pixels on the real buffer.
            _ if brush.is_set(BrushMode::Erase) => f.pass(PassOp::Load(), fb),

            // As long as we haven't finished drawing, render into the staging buffer.
            BrushState::DrawStarted(_) | BrushState::Drawing(_) => {
                f.pass(PassOp::Clear(Rgba::TRANSPARENT), staging_fb)
            }
            // Once we're done drawing, we can render into the real buffer.
            BrushState::DrawEnded(_) => f.pass(PassOp::Load(), fb),
            BrushState::NotDrawing => unreachable!(),
        };

        if brush.is_set(BrushMode::Erase) {
            p.set_pipeline(&self.const2d);
        } else {
            p.set_pipeline(&self.brush2d);
        }
        p.draw_buffer(&paint_buf);
    }

    pub fn handle_resized(
        &mut self,
        size: platform::LogicalSize,
        r: &core::Renderer,
    ) {
        let (w, h) = (size.width as u32, size.height as u32);

        self.window = size;
        self.screen_fb = r.framebuffer(w, h);
        self.screen_binding =
            self.screen2d.binding(r, &self.screen_fb, &self.sampler);
    }

    fn update_view_transforms<'a, I>(
        &mut self,
        views: I,
        offset: Vector2<f32>,
        r: &core::Renderer,
        f: &mut core::Frame,
    ) where
        I: Iterator<Item = &'a View>,
    {
        self.view_transforms.clear();
        for v in views {
            self.view_transforms.push(
                Matrix4::from_translation((offset + v.offset).extend(0.))
                    * Matrix4::from_scale(v.zoom),
            );
        }
        self.view_transforms_buf
            .update(self.view_transforms.as_slice(), r, f);
    }

    fn update_view_animations(&mut self, s: &Session, r: &core::Renderer) {
        if !s.settings["animation"].is_set() {
            return;
        }
        for (id, v) in s.views.iter() {
            if !v.animation.is_playing() {
                continue;
            }
            // FIXME: When `v.animation.val()` doesn't change, we don't need
            // to re-create the buffer.
            let buf = sprite2d::Batch::singleton(
                v.width(),
                v.height(),
                v.animation.val(),
                Rect::new(-(v.fw as f32), 0., 0., v.fh as f32) * v.zoom
                    + v.offset,
                Rgba::TRANSPARENT,
                1.,
                kit::Repeat::default(),
            )
            .finish(&r);

            self.view_data.get_mut(&id).map(|d| {
                d.anim_vb = Some(buf);
            });
        }
    }
}
