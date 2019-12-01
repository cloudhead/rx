use crate::brush::{BrushMode, BrushState};
use crate::color;
use crate::cursor2d;
use crate::data;
use crate::execution::Execution;
use crate::font::{Font, TextBatch};
use crate::framebuffer2d;
use crate::gpu;
use crate::image;
use crate::platform::{self, LogicalSize};
use crate::resources::ResourceManager;
use crate::screen2d;
use crate::session::{self, Effect, Mode, Rgb8, Session, Tool, VisualState};
use crate::view::{View, ViewCoords, ViewId, ViewManager, ViewOp};

use rgx::core;
use rgx::core::{Blending, Filter, Op, PassOp, Rgba};
use rgx::kit;
use rgx::kit::shape2d;
use rgx::kit::shape2d::{Fill, Line, Rotation, Shape, Stroke};
use rgx::kit::sprite2d;
use rgx::kit::{Bgra8, Origin, Rgba8, ZDepth};
use rgx::math::{Matrix4, Vector2};
use rgx::rect::Rect;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time;

/// 2D Renderer. Renders the [`Session`] to screen.
pub struct Renderer {
    /// Window size.
    pub window: LogicalSize,

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

    /// Pipeline for rendering the cursor.
    cursor2d: cursor2d::Pipeline,

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

    final_batch: shape2d::Batch,
    staging_batch: shape2d::Batch,
    blending: Blending,

    cache: Cache,
}

struct Cache {
    ortho: Option<Matrix4<f32>>,
    view_ortho: Option<Matrix4<f32>>,
}

/// Paste buffer.
struct Paste {
    binding: core::BindingGroup,
    texture: core::Texture,
    outputs: Vec<core::VertexBuffer>,
    ready: bool,
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
    texture: core::Texture,
    binding: core::BindingGroup,
}

impl Cursors {
    fn invert(t: &Tool) -> bool {
        match t {
            Tool::Brush(_) => true,
            _ => false,
        }
    }

    fn offset(t: &Tool) -> Vector2<f32> {
        match t {
            Tool::Sampler => Vector2::new(1., 1.),
            Tool::Brush(_) => Vector2::new(-8., -8.),
            Tool::Pan(_) => Vector2::new(-8., -8.),
            Tool::Move => Vector2::new(-8., -8.),
        }
    }

    fn rect(t: &Tool) -> Option<Rect<f32>> {
        match t {
            Tool::Sampler => Some(Rect::new(0., 0., 16., 16.)),
            Tool::Brush(_) => Some(Rect::new(16., 0., 32., 16.)),
            Tool::Move => Some(Rect::new(32., 0., 48., 16.)),
            Tool::Pan(_) => Some(Rect::new(48., 0., 64., 16.)),
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
        let vb = framebuffer2d::Pipeline::vertex_buffer(w, h, ZDepth::ZERO, r);

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
    const CHECKER_LAYER: ZDepth = ZDepth(-0.9);
    const VIEW_LAYER: ZDepth = ZDepth(-0.85);
    const UI_LAYER: ZDepth = ZDepth(-0.8);
    const TEXT_LAYER: ZDepth = ZDepth(-0.6);
    const PALETTE_LAYER: ZDepth = ZDepth(-0.4);
    const HELP_LAYER: ZDepth = ZDepth(-0.3);
    const CURSOR_LAYER: ZDepth = ZDepth(0.0);

    pub fn new(r: &mut core::Renderer, window: LogicalSize, resources: ResourceManager) -> Self {
        let (win_w, win_h) = (window.width as u32, window.height as u32);

        let sprite2d: kit::sprite2d::Pipeline = r.pipeline(Blending::default());
        let shape2d: kit::shape2d::Pipeline = r.pipeline(Blending::default());
        let framebuffer2d: framebuffer2d::Pipeline = r.pipeline(Blending::default());
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
                Font::new(texture, binding, self::GLYPH_WIDTH, self::GLYPH_HEIGHT),
                img,
            )
        };

        let mut cursor2d: cursor2d::Pipeline = r.pipeline(Blending::default());
        let (cursors, cursors_img) = {
            let (img, width, height) = image::decode(data::CURSORS).unwrap();
            let texture = r.texture(width, height);
            let binding = sprite2d.binding(r, &texture, &sampler);

            cursor2d.set_cursor(&texture, &sampler, &r);

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
                ready: false,
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
            cursor2d,
            resources,
            screen_fb,
            screen_vb,
            screen_binding,
            view_data: BTreeMap::new(),
            paste,
            staging_batch: shape2d::Batch::new(),
            final_batch: shape2d::Batch::new(),
            blending: Blending::default(),
            cache: Cache {
                ortho: None,
                view_ortho: None,
            },
        }
    }

    pub fn init(&mut self, effects: Vec<Effect>, views: &ViewManager, r: &mut core::Renderer) {
        self.handle_effects(effects, &views, r);
    }

    fn render_help(&self, session: &Session, r: &mut core::Renderer, p: &mut core::Pass) {
        let win_buf = shape2d::Batch::singleton(Shape::Rectangle(
            Rect::origin(self.window.width as f32, self.window.height as f32),
            Renderer::HELP_LAYER,
            Rotation::ZERO,
            Stroke::new(1., color::RED.into()),
            Fill::Solid(Rgba::BLACK),
        ))
        .finish(&r);

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
            Renderer::HELP_LAYER,
            color::LIGHT_GREY,
        );

        let (normal_kbs, visual_kbs): (
            Vec<(&String, &session::KeyBinding)>,
            Vec<(&String, &session::KeyBinding)>,
        ) = session
            .key_bindings
            .iter()
            .filter_map(|kb| kb.display.as_ref().map(|d| (d, kb)))
            .partition(|(_, kb)| kb.modes.contains(&Mode::Normal));

        let mut line = (0..(self.window.height as usize - self::LINE_HEIGHT as usize * 4))
            .rev()
            .step_by(self::LINE_HEIGHT as usize);

        for (display, kb) in normal_kbs.iter() {
            if let Some(y) = line.next() {
                text.add(
                    display,
                    left_margin,
                    y as f32,
                    Renderer::HELP_LAYER,
                    color::RED,
                );
                text.add(
                    &format!("{}", kb.command),
                    left_margin + column_offset,
                    y as f32,
                    Renderer::HELP_LAYER,
                    color::LIGHT_GREY,
                );
            }
        }

        if let Some(y) = line.nth(1) {
            text.add(
                "VISUAL MODE",
                left_margin,
                y as f32,
                Renderer::HELP_LAYER,
                color::RED,
            );
        }
        line.next();

        for (display, kb) in visual_kbs.iter() {
            if let Some(y) = line.next() {
                text.add(
                    display,
                    left_margin,
                    y as f32,
                    Renderer::HELP_LAYER,
                    color::RED,
                );
                text.add(
                    &format!("{}", kb.command),
                    left_margin + column_offset,
                    y as f32,
                    Renderer::HELP_LAYER,
                    color::LIGHT_GREY,
                );
            }
        }
        for (i, l) in session::HELP.lines().enumerate() {
            let y = self.window.height as f32 - (i + 4) as f32 * self::LINE_HEIGHT;

            text.add(
                l,
                left_margin + column_offset * 3. + 64.,
                y,
                Renderer::HELP_LAYER,
                color::LIGHT_GREEN,
            );
        }
        let text_buf = text.finish(&r);

        p.set_pipeline(&self.shape2d);
        p.draw_buffer(&win_buf);

        p.set_pipeline(&self.sprite2d);
        p.draw(&text_buf, &self.font.binding);
    }

    pub fn frame(
        &mut self,
        session: &Session,
        execution: Rc<RefCell<Execution>>,
        effects: Vec<session::Effect>,
        avg_frametime: &time::Duration,
        r: &mut core::Renderer,
        textures: &mut core::SwapChain,
    ) {
        if session.state != session::State::Running {
            return;
        }
        self.staging_batch.clear();
        self.final_batch.clear();

        // Handle effects produced by the session.
        self.handle_effects(effects, &session.views, r);

        let mut ui_batch = shape2d::Batch::new();
        let mut text_batch = TextBatch::new(&self.font);
        let mut overlay_batch = TextBatch::new(&self.font);
        let mut cursor_sprite =
            cursor2d::Sprite::new(self.cursors.texture.w, self.cursors.texture.h);
        let mut tool_batch = sprite2d::Batch::new(self.cursors.texture.w, self.cursors.texture.h);
        let mut paste_batch = sprite2d::Batch::new(self.paste.texture.w, self.paste.texture.h);
        let mut checker_batch =
            sprite2d::Batch::new(self.checker.texture.w, self.checker.texture.h);

        // Handle view operations.
        for v in session.views.values() {
            if !v.ops.is_empty() {
                self.handle_view_ops(&v, r);
            }
        }

        Self::draw_brush(&session, &mut ui_batch);
        Self::draw_paste(&session, &self.paste, &mut paste_batch);
        Self::draw_ui(&session, &mut ui_batch, &mut text_batch);
        Self::draw_overlay(
            &session,
            avg_frametime,
            &mut overlay_batch,
            execution.clone(),
        );
        Self::draw_palette(&session, &mut ui_batch);
        Self::draw_cursor(&session, &mut cursor_sprite, &mut tool_batch);
        Self::draw_checker(&session, &mut checker_batch);

        let ui_buf = ui_batch.finish(&r);
        let cursor_buf = cursor_sprite.finish(&r);
        let tool_buf = tool_batch.finish(&r);
        let checker_buf = checker_batch.finish(&r);
        let text_buf = text_batch.finish(&r);
        let overlay_buf = overlay_batch.finish(&r);
        let staging_buf = if self.staging_batch.is_empty() {
            None
        } else {
            Some(self.staging_batch.buffer(&r))
        };
        let final_buf = if self.final_batch.is_empty() {
            None
        } else {
            Some(self.final_batch.buffer(&r))
        };
        let paste_buf = if paste_batch.is_empty() {
            None
        } else {
            Some(paste_batch.finish(&r))
        };

        // Start the render frame.
        let mut f = r.frame();

        self.update_view_animations(session, r);
        self.update_view_transforms(session.views.values(), session.offset, &r, &mut f);
        self.cursor2d.set_framebuffer(&self.screen_fb, r);

        let v = session.active_view();
        let view_data = self
            .view_data
            .get(&v.id)
            .expect("the view data for the active view must exist");
        let view_ortho = kit::ortho(v.width(), v.height());
        let ortho = kit::ortho(self.window.width as u32, self.window.height as u32);

        if self.cache.ortho.map_or(true, |m| m != ortho) {
            r.update_pipeline(&self.shape2d, ortho, &mut f);
            r.update_pipeline(&self.sprite2d, ortho, &mut f);
            r.update_pipeline(&self.framebuffer2d, ortho, &mut f);
            r.update_pipeline(&self.cursor2d, ortho, &mut f);

            self.cache.ortho = Some(ortho);
        }
        if self.cache.view_ortho.map_or(true, |m| m != view_ortho) {
            r.update_pipeline(&self.brush2d, view_ortho, &mut f);
            r.update_pipeline(&self.const2d, view_ortho, &mut f);
            r.update_pipeline(&self.paste2d, view_ortho, &mut f);

            self.cache.view_ortho = Some(view_ortho);
        }

        let present = &textures.next();

        {
            // Draw to view staging buffer.
            {
                // Always clear the active view staging buffer. We do this because
                // it may not get drawn to this frame, and hence may remain dirty
                // from a previous frame.
                let mut p = f.pass(PassOp::Clear(Rgba::TRANSPARENT), &view_data.staging_fb);

                // Render brush strokes to view staging framebuffers.
                if let Some(buf) = &staging_buf {
                    self.render_brush_strokes(buf, &Blending::default(), &mut p);
                }
                // Draw paste buffer to view staging buffer.
                if let Some(buf) = paste_buf {
                    // Nb. Strangely enough, when the paste texture is being
                    // re-created at a different size within this frame,
                    // it is displayed for a single frame at the wrong size.
                    // Perhaps because there is some stale state in the render
                    // pipeline... To prevent this, we don't allow the texture
                    // to be resized and displayed within the same frame.
                    if self.paste.ready {
                        p.set_pipeline(&self.paste2d);
                        p.draw(&buf, &self.paste.binding);
                    } else {
                        self.paste.ready = true;
                    }
                }
            }

            // Draw to view display buffer.
            {
                let mut p = f.pass(PassOp::Load(), &view_data.fb);

                // Render brush strokes to view framebuffers.
                if let Some(buf) = &final_buf {
                    self.render_brush_strokes(buf, &self.blending, &mut p);
                }
                // Draw paste buffer to view framebuffer.
                if !self.paste.outputs.is_empty() {
                    p.set_pipeline(&self.paste2d);

                    for out in self.paste.outputs.drain(..) {
                        p.draw(&out, &self.paste.binding);
                    }
                }
            }
        }

        {
            let mut p = f.pass(PassOp::Clear(Rgba::TRANSPARENT), &self.screen_fb);

            // Draw view checkers to screen framebuffer.
            if session.settings["checker"].is_set() {
                p.set_pipeline(&self.sprite2d);
                p.draw(&checker_buf, &self.checker.binding);
            }

            // Draw view framebuffers to screen framebuffer.
            p.set_pipeline(&self.framebuffer2d);
            self.render_views(&mut p);

            // Draw UI elements to screen framebuffer.
            p.set_pipeline(&self.shape2d);
            p.draw_buffer(&ui_buf);

            // Draw text & cursor to screen framebuffer.
            p.set_pipeline(&self.sprite2d);
            p.draw(&text_buf, &self.font.binding);
            p.draw(&tool_buf, &self.cursors.binding);

            // Draw view animations to screen framebuffer.
            if session.settings["animation"].is_set() {
                self.render_view_animations(&session.views, &mut p);
            }
            // Draw help menu.
            if session.mode == Mode::Help {
                self.render_help(session, r, &mut p);
            }
        }

        {
            // Present screen framebuffer to screen.
            let bg = Rgba::from(session.settings["background"].color());
            let mut p = f.pass(PassOp::Clear(bg), present);

            p.set_pipeline(&self.screen2d);
            p.set_binding(&self.screen_binding, &[]);
            p.draw_buffer(&self.screen_vb);

            if session.settings["debug"].is_set() || !execution.borrow().is_normal() {
                p.set_pipeline(&self.sprite2d);
                p.draw(&overlay_buf, &self.font.binding);
            }

            {
                if let (Some(fb), Some(cursor)) = (
                    &self.cursor2d.framebuffer_binding,
                    &self.cursor2d.cursor_binding,
                ) {
                    p.set_pipeline(&self.cursor2d);
                    p.set_binding(cursor, &[]);
                    p.set_binding(fb, &[]);
                    p.draw_buffer(&cursor_buf);
                }
            }
        }

        // Submit frame to device.
        r.present(f);

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            let id = v.id;
            let extent = v.extent();
            let resources = self.resources.clone();

            r.read(&view_data.fb, move |data| {
                if let Some(s) = resources.lock_mut().get_view_mut(id) {
                    s.push_snapshot(data, extent);
                }
            });
        }

        if !execution.borrow().is_normal() {
            r.read(&self.screen_fb, move |data| {
                execution.clone().borrow_mut().record(data);
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
                Effect::ViewBlendingChanged(blending) => {
                    self.blending = blending;
                }
                Effect::ViewPaintDraft(shapes) => {
                    shapes.into_iter().for_each(|s| self.staging_batch.add(s));
                }
                Effect::ViewPaintFinal(shapes) => {
                    shapes.into_iter().for_each(|s| self.final_batch.add(s));
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
            let view_data = ViewData::new(vw, vh, &self.framebuffer2d, &self.sprite2d, r);

            // We don't want the lock to be held when `submit` is called below,
            // because in some cases it'll trigger the read-back which claims
            // a write lock on resources.
            let (sw, sh, pixels) = {
                let resources = self.resources.lock();
                let (snapshot, pixels) = resources.get_snapshot(v.id);
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
                let (_, pixels) = rs.get_snapshot(v.id);
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
                    let pixels = {
                        let resources = self.resources.lock();
                        let (snapshot, pixels) = resources.get_snapshot(v.id);

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
                        let mut pixels: Vec<Rgba8> = Vec::with_capacity(buffer.len());
                        for c in buffer.into_iter() {
                            pixels.push(c.into());
                        }
                        assert!(pixels.len() == w * h);

                        if self.paste.texture.w != w as u32 || self.paste.texture.h != h as u32 {
                            self.paste.ready = false;
                            self.paste.texture = r.texture(w as u32, h as u32);
                            self.paste.binding =
                                self.paste2d.binding(r, &self.paste.texture, &self.sampler);
                        }
                        pixels
                    };
                    r.submit(&[Op::Fill(&self.paste.texture, &pixels)]);
                }
                ViewOp::Paste(dst) => {
                    let buffer = sprite2d::Batch::singleton(
                        self.paste.texture.w,
                        self.paste.texture.h,
                        self.paste.texture.rect(),
                        dst.map(|n| n as f32),
                        ZDepth::default(),
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
            let (s, pixels) = resources.get_snapshot(*id);
            let (w, h) = (s.width(), s.height());

            let view_data = ViewData::new(w, h, &self.framebuffer2d, &self.sprite2d, r);

            debug_assert!(!pixels.is_empty());
            r.submit(&[
                Op::Clear(&view_data.fb, Bgra8::TRANSPARENT),
                Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT),
                Op::Fill(&view_data.fb, &pixels),
            ]);

            self.view_data.insert(*id, view_data);
        }
    }

    fn draw_ui(session: &Session, canvas: &mut shape2d::Batch, text: &mut TextBatch) {
        let view = session.active_view();

        if let Some(selection) = session.selection {
            let fill = match session.mode {
                Mode::Visual(VisualState::Selecting { .. }) => {
                    Rgba8::new(color::RED.r, color::RED.g, color::RED.b, 0x55)
                }
                // TODO: Handle different modes differently.
                _ => Rgba8::TRANSPARENT,
            };
            let stroke = color::RED;

            let r = selection.abs().bounds();
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
                text.add(&t, x + offset.x, y + offset.y, Renderer::TEXT_LAYER, stroke);
            }

            // Selection stroke.
            canvas.add(Shape::Rectangle(
                r.map(|n| n as f32) * view.zoom + offset,
                Renderer::UI_LAYER,
                Rotation::ZERO,
                Stroke::new(1., stroke.into()),
                Fill::Empty(),
            ));
            // Selection fill.
            if r.intersects(view.bounds()) {
                canvas.add(Shape::Rectangle(
                    r.intersection(view.bounds()).map(|n| n as f32) * view.zoom + offset,
                    Renderer::UI_LAYER,
                    Rotation::ZERO,
                    Stroke::NONE,
                    Fill::Solid(fill.into()),
                ));
            }
        }

        for (id, v) in session.views.iter() {
            let offset = v.offset + session.offset;

            // Frame lines
            for n in 1..v.animation.len() {
                let n = n as f32;
                let x = n * v.zoom * v.fw as f32 + offset.x;
                canvas.add(Shape::Line(
                    Line::new(x, offset.y, x, v.zoom * v.fh as f32 + offset.y),
                    Renderer::UI_LAYER,
                    Rotation::ZERO,
                    Stroke::new(1.0, Rgba::new(1., 1., 1., 0.6)),
                ));
            }
            // View border
            let r = v.rect();
            let border_color = if session.is_active(*id) {
                match session.mode {
                    // TODO: (rgx) Use `Rgba8::alpha`.
                    Mode::Visual(_) => {
                        Rgba8::new(color::RED.r, color::RED.g, color::RED.b, 0xdd).into()
                    }
                    _ => color::WHITE.into(),
                }
            } else if session.hover_view == Some(*id) {
                Rgba::new(0.7, 0.7, 0.7, 1.0)
            } else {
                Rgba::new(0.5, 0.5, 0.5, 1.0)
            };
            canvas.add(Shape::Rectangle(
                Rect::new(r.x1 - 1., r.y1 - 1., r.x2 + 1., r.y2 + 1.) + session.offset,
                Renderer::UI_LAYER,
                Rotation::ZERO,
                Stroke::new(1.0, border_color),
                Fill::Empty(),
            ));

            if session.settings["ui/view-info"].is_set() {
                // View info
                text.add(
                    &format!("{}x{}x{}", v.fw, v.fh, v.animation.len()),
                    offset.x,
                    offset.y - self::LINE_HEIGHT,
                    Renderer::TEXT_LAYER,
                    color::GREY,
                );
            }
        }
        if session.settings["ui/status"].is_set() {
            // Active view status
            text.add(
                &view.status(),
                MARGIN,
                MARGIN + self::LINE_HEIGHT,
                Renderer::TEXT_LAYER,
                Rgba8::WHITE,
            );

            // Session status
            text.add(
                &format!("{:>5}%", (view.zoom * 100.) as u32),
                session.width - MARGIN - 6. * 8.,
                MARGIN + self::LINE_HEIGHT,
                Renderer::TEXT_LAYER,
                Rgba8::WHITE,
            );

            if session.width >= 600. {
                let cursor = session.view_coords(view.id, session.cursor);
                let hover_color = session
                    .hover_color
                    .map_or(String::new(), |c| Rgb8::from(c).to_string());
                text.add(
                    &format!("{:>4},{:<4} {}", cursor.x, cursor.y, hover_color),
                    session.width * 0.5,
                    MARGIN + self::LINE_HEIGHT,
                    Renderer::TEXT_LAYER,
                    Rgba8::WHITE,
                );
            }
        }

        if session.settings["ui/switcher"].is_set() {
            if session.width >= 400. {
                // Fg color
                canvas.add(Shape::Rectangle(
                    Rect::origin(11., 11.)
                        .with_origin(session.width * 0.4, self::LINE_HEIGHT + self::MARGIN + 2.),
                    Renderer::UI_LAYER,
                    Rotation::ZERO,
                    Stroke::new(1.0, Rgba::WHITE),
                    Fill::Solid(session.fg.into()),
                ));
                // Bg color
                canvas.add(Shape::Rectangle(
                    Rect::origin(11., 11.).with_origin(
                        session.width * 0.4 + 25.,
                        self::LINE_HEIGHT + self::MARGIN + 2.,
                    ),
                    Renderer::UI_LAYER,
                    Rotation::ZERO,
                    Stroke::new(1.0, Rgba::WHITE),
                    Fill::Solid(session.bg.into()),
                ));
            }
        }

        // Command-line & message
        if session.mode == Mode::Command {
            let s = format!("{}", &session.cmdline.input());
            text.add(&s, MARGIN, MARGIN, Renderer::TEXT_LAYER, Rgba8::WHITE);
        } else if !session.message.is_replay() && session.settings["ui/message"].is_set() {
            let s = format!("{}", &session.message);
            text.add(
                &s,
                MARGIN,
                MARGIN,
                Renderer::TEXT_LAYER,
                session.message.color(),
            );
        }
    }

    fn draw_overlay(
        session: &Session,
        avg_frametime: &time::Duration,
        text: &mut TextBatch,
        exec: Rc<RefCell<Execution>>,
    ) {
        match &*exec.borrow() {
            Execution::Recording { path, .. } => {
                text.add(
                    &format!("* recording: {} (<End> to stop)", path.display()),
                    MARGIN * 2.,
                    session.height - self::LINE_HEIGHT - MARGIN,
                    ZDepth::ZERO,
                    color::RED,
                );
            }
            Execution::Replaying { events, path, .. } => {
                if let Some(event) = events.front() {
                    text.add(
                        &format!(
                            "> replaying: {}: {:32} (<Esc> to stop)",
                            path.display(),
                            String::from(event.clone()),
                        ),
                        MARGIN * 2.,
                        session.height - self::LINE_HEIGHT - MARGIN,
                        ZDepth::ZERO,
                        color::LIGHT_GREEN,
                    );
                }
            }
            Execution::Normal => {}
        }

        if session.settings["debug"].is_set() {
            let mem = crate::ALLOCATOR.allocated();

            // Frame-time
            let txt = &format!(
                "{:3.2}ms {:3.2}ms {}MB {}KB {}",
                avg_frametime.as_micros() as f64 / 1000.,
                session.avg_time.as_micros() as f64 / 1000.,
                mem / (1024 * 1024),
                mem / 1024 % (1024),
                session.mode,
            );
            text.add(
                txt,
                MARGIN,
                session.height - MARGIN - self::LINE_HEIGHT,
                ZDepth::ZERO,
                Rgba8::WHITE,
            );
        }

        if session.message.is_replay() {
            text.add(
                &format!("{}", session.message),
                MARGIN,
                MARGIN,
                ZDepth::ZERO,
                session.message.color(),
            );
        }
    }

    fn draw_palette(session: &Session, batch: &mut shape2d::Batch) {
        if !session.settings["ui/palette"].is_set() {
            return;
        }

        let p = &session.palette;
        for (i, color) in p.colors.iter().cloned().enumerate() {
            let x = if i >= 16 { p.cellsize } else { 0. };
            let y = (i % 16) as f32 * p.cellsize;

            let mut stroke = shape2d::Stroke::NONE;
            if let (Tool::Sampler, Some(c)) = (&session.tool, p.hover) {
                if c == color {
                    stroke = shape2d::Stroke::new(1., Rgba::WHITE);
                }
            }

            batch.add(Shape::Rectangle(
                Rect::new(p.x + x, p.y + y, p.x + x + p.cellsize, p.y + y + p.cellsize),
                Renderer::PALETTE_LAYER,
                Rotation::ZERO,
                stroke,
                shape2d::Fill::Solid(color.into()),
            ));
        }
    }

    fn draw_checker(session: &Session, batch: &mut sprite2d::Batch) {
        if session.settings["checker"].is_set() {
            for (_, v) in session.views.iter() {
                let ratio = v.width() as f32 / v.height() as f32;
                let rx = v.zoom * ratio;
                let ry = v.zoom;

                batch.add(
                    Checker::rect(),
                    v.rect() + session.offset,
                    Renderer::CHECKER_LAYER,
                    Rgba::TRANSPARENT,
                    1.,
                    kit::Repeat::new(rx, ry),
                );
            }
        }
    }

    fn draw_cursor(session: &Session, sprite: &mut cursor2d::Sprite, batch: &mut sprite2d::Batch) {
        if !session.settings["ui/cursor"].is_set() {
            return;
        }

        // TODO: Cursor should be greyed out in command mode.
        match session.mode {
            Mode::Present | Mode::Help => {}
            Mode::Visual(mode) => {
                if let VisualState::Selecting { .. } = mode {
                    let c = session.cursor;
                    let v = session.active_view();
                    if v.contains(c - session.offset) {
                        if session.is_selected(session.view_coords(v.id, c).into()) {
                            if let Some(rect) = Cursors::rect(&Tool::Move) {
                                let offset = Cursors::offset(&Tool::Move);
                                batch.add(
                                    rect,
                                    rect.with_origin(c.x, c.y) + offset,
                                    Renderer::CURSOR_LAYER,
                                    Rgba::TRANSPARENT,
                                    1.,
                                    kit::Repeat::default(),
                                );
                                return;
                            }
                        }
                    }
                }

                if let Some(rect) = Cursors::rect(&Tool::default()) {
                    let offset = Cursors::offset(&Tool::default());
                    let cursor = session.cursor;
                    sprite.set(
                        rect,
                        rect.with_origin(cursor.x, cursor.y) + offset,
                        Renderer::CURSOR_LAYER,
                    );
                }
            }
            Mode::Normal | Mode::Command => {
                if let Some(rect) = Cursors::rect(&session.tool) {
                    let offset = Cursors::offset(&session.tool);
                    let cursor = session.cursor;

                    if Cursors::invert(&session.tool) {
                        sprite.set(
                            rect,
                            rect.with_origin(cursor.x, cursor.y) + offset,
                            Renderer::CURSOR_LAYER,
                        );
                    } else {
                        batch.add(
                            rect,
                            rect.with_origin(cursor.x, cursor.y) + offset,
                            Renderer::CURSOR_LAYER,
                            Rgba::TRANSPARENT,
                            1.,
                            kit::Repeat::default(),
                        );
                    }
                }
            }
        }
    }

    fn draw_brush(session: &Session, shapes: &mut shape2d::Batch) {
        if session.palette.hover.is_some() {
            return;
        }
        if !session.settings["input/mouse"].is_set() {
            return;
        }
        let v = session.active_view();
        let c = session.cursor;

        match session.mode {
            Mode::Visual(VisualState::Selecting { .. }) => {
                if session.is_selected(session.view_coords(v.id, c).into()) {
                    return;
                }

                if v.contains(c - session.offset) {
                    let z = v.zoom;
                    let c = session.snap(c, v.offset.x, v.offset.y, z);
                    shapes.add(Shape::Rectangle(
                        Rect::new(c.x, c.y, c.x + z, c.y + z),
                        Renderer::UI_LAYER,
                        Rotation::ZERO,
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

                        let view_coords = session.active_view_coords(c);
                        for p in brush.expand(view_coords.into(), v.extent()) {
                            shapes.add(brush.shape(
                                *session.session_coords(v.id, p.into()),
                                Renderer::UI_LAYER,
                                stroke,
                                fill,
                                v.zoom,
                                Origin::BottomLeft,
                            ));
                        }

                        // X-Ray brush mode.
                        if brush.state == BrushState::NotDrawing && brush.is_set(BrushMode::XRay) {
                            let p: ViewCoords<u32> = view_coords.into();
                            let center = session.color_at(v.id, p);
                            let top =
                                session.color_at(v.id, ViewCoords::new(p.x, p.y + 1)) != center;
                            let bottom =
                                session.color_at(v.id, ViewCoords::new(p.x, p.y - 1)) != center;
                            let left =
                                session.color_at(v.id, ViewCoords::new(p.x - 1, p.y)) != center;
                            let right =
                                session.color_at(v.id, ViewCoords::new(p.x + 1, p.y)) != center;

                            let p = *session
                                .session_coords(v.id, ViewCoords::new(p.x as f32, p.y as f32));
                            let w = brush.size as f32 * v.zoom;

                            let t: f32 = 1.;
                            let stroke = Stroke::new(
                                t,
                                center
                                    .map(|c| c.alpha(0xff))
                                    .unwrap_or(Rgba8::TRANSPARENT)
                                    .into(),
                            );

                            if top {
                                shapes.add(Shape::Line(
                                    Line::new(p.x, p.y + w - t / 2., p.x + w, p.y + w - t / 2.),
                                    Renderer::UI_LAYER,
                                    Rotation::ZERO,
                                    stroke,
                                ));
                            }
                            if bottom {
                                shapes.add(Shape::Line(
                                    Line::new(p.x, p.y + t / 2., p.x + w, p.y + t / 2.),
                                    Renderer::UI_LAYER,
                                    Rotation::ZERO,
                                    stroke,
                                ));
                            }
                            if left {
                                shapes.add(Shape::Line(
                                    Line::new(p.x + t / 2., p.y, p.x + t / 2., p.y + w),
                                    Renderer::UI_LAYER,
                                    Rotation::ZERO,
                                    stroke,
                                ));
                            }
                            if right {
                                shapes.add(Shape::Line(
                                    Line::new(p.x + w - t / 2., p.y, p.x + w - t / 2., p.y + w),
                                    Renderer::UI_LAYER,
                                    Rotation::ZERO,
                                    stroke,
                                ));
                            }
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
                            Renderer::UI_LAYER,
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

    fn draw_paste(session: &Session, paste: &Paste, batch: &mut sprite2d::Batch) {
        if let (Mode::Visual(VisualState::Pasting), Some(s)) = (session.mode, session.selection) {
            batch.add(
                paste.texture.rect(),
                Rect::new(s.x1 as f32, s.y1 as f32, s.x2 as f32 + 1., s.y2 as f32 + 1.),
                ZDepth::default(),
                Rgba::TRANSPARENT,
                0.9,
                kit::Repeat::default(),
            );
        }
    }

    fn render_views(&self, p: &mut core::Pass) {
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
        for (id, v) in self.view_data.iter() {
            if let (Some(vb), Some(view)) = (&v.anim_vb, views.get(id)) {
                if view.animation.len() > 1 {
                    p.draw(vb, &v.anim_binding);
                }
            }
        }
    }

    fn render_brush_strokes(
        &self,
        paint_buf: &core::VertexBuffer,
        blending: &Blending,
        p: &mut core::Pass,
    ) {
        if blending == &Blending::constant() {
            p.set_pipeline(&self.const2d);
        } else {
            p.set_pipeline(&self.brush2d);
        }
        p.draw_buffer(&paint_buf);
    }

    pub fn handle_resized(&mut self, size: platform::LogicalSize, r: &core::Renderer) {
        let (w, h) = (size.width as u32, size.height as u32);

        self.window = size;
        self.screen_fb = r.framebuffer(w, h);
        self.screen_binding = self.screen2d.binding(r, &self.screen_fb, &self.sampler);
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
                Matrix4::from_translation((offset + v.offset).extend(*Renderer::VIEW_LAYER))
                    * Matrix4::from_nonuniform_scale(v.zoom, v.zoom, 1.0),
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
                Rect::new(-(v.fw as f32), 0., 0., v.fh as f32) * v.zoom + (s.offset + v.offset),
                Renderer::VIEW_LAYER,
                Rgba::TRANSPARENT,
                1.,
                kit::Repeat::default(),
            )
            .finish(&r);

            if let Some(d) = self.view_data.get_mut(&id) {
                d.anim_vb = Some(buf);
            }
        }
    }
}
