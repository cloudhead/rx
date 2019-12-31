use crate::cursor2d;
use crate::data;
use crate::draw;
use crate::execution::Execution;
use crate::font::{Font, TextBatch};
use crate::framebuffer2d;
use crate::image;
use crate::platform::{self, LogicalSize};
use crate::resources::ResourceManager;
use crate::screen2d;
use crate::session::{self, Effect, Mode, Session};
use crate::view::{View, ViewId, ViewManager, ViewOp};

use rgx::core;
use rgx::core::{Blending, Filter, Op, PassOp, Rgba};
use rgx::kit;
use rgx::kit::shape2d;
use rgx::kit::sprite2d;
use rgx::kit::{Bgra8, Rgba8, ZDepth};
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
    view_transforms_buf: kit::TransformBuffer,
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
    scale: f32,
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

struct Cursors {
    texture: core::Texture,
    binding: core::BindingGroup,
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

impl Renderer {
    pub fn new(r: &mut core::Renderer, window: LogicalSize, resources: ResourceManager) -> Self {
        let (win_w, win_h) = (window.width as u32, window.height as u32);

        let sprite2d: kit::sprite2d::Pipeline = r.pipeline(Blending::default());
        let shape2d: kit::shape2d::Pipeline = r.pipeline(Blending::default());
        let framebuffer2d: framebuffer2d::Pipeline = r.pipeline(Blending::default());
        let screen2d: screen2d::Pipeline = r.pipeline(Blending::default());

        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);

        let view_transforms_buf = kit::TransformBuffer::with_capacity(
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
                Font::new(texture, binding, draw::GLYPH_WIDTH, draw::GLYPH_HEIGHT),
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
            let texture = r.texture(2, 2);
            let binding = sprite2d.binding(r, &texture, &sampler);

            (Checker { texture, binding }, draw::CHECKER)
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
                scale: 0.,
            },
        }
    }

    pub fn init(&mut self, effects: Vec<Effect>, views: &ViewManager, r: &mut core::Renderer) {
        self.handle_effects(effects, &views, r);
    }

    fn render_help(&self, session: &Session, r: &mut core::Renderer, p: &mut core::Pass) {
        let mut win = shape2d::Batch::new();
        let mut text = TextBatch::new(&self.font);

        draw::draw_help(session, &mut text, &mut win);

        let win_buf = win.finish(&r);
        let text_buf = text.finish(&r);

        p.set_pipeline(&self.sprite2d);
        p.draw(&text_buf, &self.font.binding);

        p.set_pipeline(&self.shape2d);
        p.draw_buffer(&win_buf);
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

        draw::draw_brush(&session, &mut ui_batch);
        draw::draw_paste(&session, self.paste.texture.rect(), &mut paste_batch);
        draw::draw_grid(&session, &mut ui_batch);
        draw::draw_ui(&session, &mut ui_batch, &mut text_batch);
        draw::draw_overlay(
            &session,
            avg_frametime,
            &mut overlay_batch,
            execution.clone(),
        );
        draw::draw_palette(&session, &mut ui_batch);
        draw::draw_cursor(&session, &mut cursor_sprite, &mut tool_batch);
        draw::draw_checker(&session, &mut checker_batch);

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
        let scale: f32 = session.settings["scale"].clone().into();

        if (scale - self.cache.scale).abs() > std::f32::EPSILON {
            r.update_pipeline(&self.cursor2d, cursor2d::context(ortho, scale), &mut f);
            self.cache.scale = scale;
        }

        if self.cache.ortho.map_or(true, |m| m != ortho) {
            r.update_pipeline(&self.shape2d, ortho, &mut f);
            r.update_pipeline(&self.sprite2d, ortho, &mut f);
            r.update_pipeline(&self.framebuffer2d, ortho, &mut f);
            r.update_pipeline(
                &self.cursor2d,
                cursor2d::context(ortho, self.cache.scale),
                &mut f,
            );

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
                self.render_help(&session, r, &mut p);
            }
        }

        {
            // Present screen framebuffer to screen.
            let bg = Rgba::from(session.settings["background"].rgba8());
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
                execution.borrow_mut().record(data);
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

    fn render_views(&self, p: &mut core::Pass) {
        for ((_, v), off) in self
            .view_data
            .iter()
            .zip(self.view_transforms_buf.offsets())
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
                Matrix4::from_translation((offset + v.offset).extend(*draw::VIEW_LAYER))
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
            let buf = draw::draw_view_animation(s, &v).finish(&r);

            if let Some(d) = self.view_data.get_mut(&id) {
                d.anim_vb = Some(buf);
            }
        }
    }
}
