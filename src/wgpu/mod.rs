mod cursor2d;
mod framebuffer2d;
mod screen2d;

use crate::data;
use crate::draw;
use crate::execution::Execution;
use crate::font::TextBatch;
use crate::image;
use crate::platform::{self, LogicalSize};
use crate::renderer;
use crate::resources::{Pixels, ResourceManager};
use crate::session::{self, Effect, Mode, PresentMode, Session};
use crate::sprite;
use crate::view::{View, ViewId, ViewManager, ViewOp};

use rgx::core::transform;
use rgx::core::Renderable;
use rgx::core::{self, Blending, Filter, Op, PassOp, Rgba};
use rgx::kit::{self, shape2d, sprite2d};
use rgx::kit::{Bgra8, Origin, Rgba8, ZDepth};
use rgx::math::{Matrix4, Vector2};
use rgx::rect::Rect;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time;

/// 2D Renderer. Renders the [`Session`] to screen.
pub struct Renderer {
    /// Renderer backend.
    r: core::Renderer,
    /// Swap chain.
    swap_chain: core::SwapChain,
    /// Presentation mode, eg. vsync.
    present_mode: PresentMode,
    /// HiDPI scaling factor.
    scale_factor: f64,
    /// UI scaling factor.
    scale: f64,
    /// Window size.
    pub win_size: LogicalSize,

    /// The font used to render text.
    font: Font,
    cursors: Cursors,
    checker: Checker,
    /// View transforms. These are sorted by [`ViewId`].
    view_transforms: Vec<Matrix4<f32>>,
    /// View transform buffer, created from the transform matrices. This is bound
    /// as a dynamic uniform buffer, to render all views in a single pass.
    view_transforms_buf: transform::TransformBuffer,
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
}

pub struct Font {
    gw: f32,
    gh: f32,

    width: u32,
    height: u32,

    binding: core::BindingGroup,
    texture: core::Texture,
}

impl Font {
    pub fn new(texture: core::Texture, binding: core::BindingGroup, gw: f32, gh: f32) -> Font {
        let width = texture.w;
        let height = texture.h;

        Font {
            gw,
            gh,
            width,
            height,
            texture,
            binding,
        }
    }
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

impl session::Blending {
    fn to_wgpu(&self) -> core::Blending {
        match self {
            Self::Alpha => core::Blending::default(),
            Self::Constant => core::Blending::constant(),
        }
    }
}

impl PresentMode {
    fn to_wgpu(&self) -> core::PresentMode {
        match self {
            Self::Vsync => core::PresentMode::Vsync,
            Self::NoVsync => core::PresentMode::NoVsync,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

impl<'a> renderer::Renderer<'a> for Renderer {
    fn new(
        win: &mut platform::backend::Window,
        win_size: LogicalSize,
        scale_factor: f64,
        present_mode: PresentMode,
        resources: ResourceManager,
        assets: data::Assets<'a>,
    ) -> std::io::Result<Self> {
        let (win_w, win_h) = (win_size.width as u32, win_size.height as u32);
        let mut r = core::Renderer::new(win.handle())?;

        let sprite2d: kit::sprite2d::Pipeline = r.pipeline(Blending::default());
        let shape2d: kit::shape2d::Pipeline = r.pipeline(Blending::default());
        let framebuffer2d: framebuffer2d::Pipeline = r.pipeline(Blending::default());
        let screen2d: screen2d::Pipeline = r.pipeline(Blending::default());

        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);

        let view_transforms_buf = transform::TransformBuffer::with_capacity(
            Session::MAX_VIEWS,
            &framebuffer2d.pipeline.layout.sets[1],
            &r.device,
        );
        let view_transforms = Vec::with_capacity(Session::MAX_VIEWS);

        let (font, font_img) = {
            let (img, width, height) = image::decode(assets.glyphs).unwrap();
            let texture = r.texture(width, height);
            let binding = sprite2d.binding(&r, &texture, &sampler);

            (
                Font::new(texture, binding, draw::GLYPH_WIDTH, draw::GLYPH_HEIGHT),
                img,
            )
        };

        let mut cursor2d: cursor2d::Pipeline = r.pipeline(Blending::default());
        let (cursors, cursors_img) = {
            let (img, width, height) = image::decode(data::CURSORS).unwrap();
            let texture = r.texture(width, height);
            let binding = sprite2d.binding(&r, &texture, &sampler);

            cursor2d.set_cursor(&texture, &sampler, &r);

            (Cursors { texture, binding }, img)
        };

        let (checker, checker_img) = {
            let texture = r.texture(2, 2);
            let binding = sprite2d.binding(&r, &texture, &sampler);

            (Checker { texture, binding }, draw::CHECKER)
        };

        let brush2d = r.pipeline(Blending::default());
        let const2d = r.pipeline(Blending::constant());
        let paste2d: sprite2d::Pipeline = r.pipeline(Blending::default());

        let paste = {
            let texture = r.texture(1, 1);
            let binding = paste2d.binding(&r, &texture, &sampler);
            Paste {
                texture,
                binding,
                outputs: Vec::new(),
            }
        };

        let screen_vb = screen2d::Pipeline::vertex_buffer(&r);
        let screen_fb = r.framebuffer(win_w, win_h);
        let screen_binding = screen2d.binding(&r, &screen_fb, &sampler);

        r.submit(&[
            Op::Fill(&font.texture, Rgba8::align(&font_img)),
            Op::Fill(&cursors.texture, Rgba8::align(&cursors_img)),
            Op::Fill(&checker.texture, Rgba8::align(&checker_img)),
        ]);

        let physical = win_size.to_physical(scale_factor);
        let swap_chain = r.swap_chain(
            physical.width as u32,
            physical.height as u32,
            present_mode.to_wgpu(),
        );

        Ok(Self {
            r,
            swap_chain,
            present_mode,
            scale_factor,
            scale: 1.,
            win_size,
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
        })
    }

    fn init(&mut self, effects: Vec<Effect>) {
        self.handle_effects(effects);
    }

    fn frame(
        &mut self,
        session: &Session,
        execution: Rc<RefCell<Execution>>,
        effects: Vec<session::Effect>,
        avg_frametime: &time::Duration,
    ) {
        if session.state != session::State::Running {
            return;
        }
        self.staging_batch.clear();
        self.final_batch.clear();

        // Handle effects produced by the session.
        self.handle_effects(effects);

        let mut ctx = draw::Context {
            ui_batch: shape2d::Batch::new(),
            text_batch: TextBatch::new(
                self.font.width,
                self.font.height,
                self.font.gw,
                self.font.gh,
            ),
            overlay_batch: TextBatch::new(
                self.font.width,
                self.font.height,
                self.font.gw,
                self.font.gh,
            ),
            cursor_sprite: sprite::Sprite::new(self.cursors.texture.w, self.cursors.texture.h),
            tool_batch: sprite2d::Batch::new(self.cursors.texture.w, self.cursors.texture.h),
            paste_batch: sprite2d::Batch::new(self.paste.texture.w, self.paste.texture.h),
            checker_batch: sprite2d::Batch::new(self.checker.texture.w, self.checker.texture.h),
        };

        // TODO: Don't re-create the context every frame. Just clear it.
        ctx.clear();

        ctx.draw(&session, avg_frametime, execution.clone());

        let ui_buf = ctx.ui_batch.finish(&self.r);
        let cursor_buf = ctx.cursor_sprite.finish(&self.r);
        let tool_buf = if ctx.tool_batch.is_empty() {
            None
        } else {
            Some(ctx.tool_batch.finish(&self.r))
        };
        let checker_buf = ctx.checker_batch.finish(&self.r);
        let text_buf = ctx.text_batch.finish(&self.r);
        let overlay_buf = ctx.overlay_batch.finish(&self.r);
        let staging_buf = if self.staging_batch.is_empty() {
            None
        } else {
            Some(self.staging_batch.buffer(&self.r))
        };
        let final_buf = if self.final_batch.is_empty() {
            None
        } else {
            Some(self.final_batch.buffer(&self.r))
        };
        let paste_buf = if ctx.paste_batch.is_empty() {
            None
        } else {
            Some(ctx.paste_batch.finish(&self.r))
        };

        // Start the render frame.
        let mut f = self.r.frame();

        self.update_view_animations(session);
        self.update_view_transforms(session.views.iter(), session.offset, &mut f);
        self.cursor2d.set_framebuffer(&self.screen_fb, &self.r);

        let v = session.active_view();
        let view_data = self
            .view_data
            .get(&v.id)
            .expect("the view data for the active view must exist");
        let view_ortho = kit::ortho(v.width(), v.height(), Origin::TopLeft);
        let ortho = kit::ortho(
            self.screen_fb.width() as u32,
            self.screen_fb.height() as u32,
            Origin::TopLeft,
        );
        let pixel_ratio = platform::pixel_ratio(self.scale_factor);
        let scale = (session.settings["scale"].to_f64() * pixel_ratio) as f32;

        if (scale - self.cache.scale).abs() > std::f32::EPSILON {
            self.r
                .update_pipeline(&self.cursor2d, cursor2d::context(ortho, scale), &mut f);
            self.cache.scale = scale;
        }

        if self.cache.ortho.map_or(true, |m| m != ortho) {
            self.r.update_pipeline(&self.shape2d, ortho, &mut f);
            self.r.update_pipeline(&self.sprite2d, ortho, &mut f);
            self.r.update_pipeline(&self.framebuffer2d, ortho, &mut f);
            self.r.update_pipeline(
                &self.cursor2d,
                cursor2d::context(ortho, self.cache.scale),
                &mut f,
            );

            self.cache.ortho = Some(ortho);
        }
        if self.cache.view_ortho.map_or(true, |m| m != view_ortho) {
            self.r.update_pipeline(&self.brush2d, view_ortho, &mut f);
            self.r.update_pipeline(&self.const2d, view_ortho, &mut f);
            self.r.update_pipeline(&self.paste2d, view_ortho, &mut f);

            self.cache.view_ortho = Some(view_ortho);
        }

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
                    p.set_pipeline(&self.paste2d);
                    p.draw(&buf, &self.paste.binding);
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
            if let Some(tool_buf) = tool_buf {
                p.draw(&tool_buf, &self.cursors.binding);
            }

            // Draw view animations to screen framebuffer.
            if session.settings["animation"].is_set() {
                self.render_view_animations(&session.views, &mut p);
            }
            // Draw help menu.
            if session.mode == Mode::Help {
                self.render_help(&session, &mut p);
            }
        }

        // Present screen framebuffer to screen.
        let present = &self.swap_chain.next();
        {
            let bg = Rgba::from(session.settings["background"].to_rgba8());
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
        self.r.present(f);

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            let id = v.id;
            let extent = v.extent();
            let resources = self.resources.clone();

            self.r.read(&view_data.fb, move |data| {
                if let Some(s) = resources.lock_mut().get_view_mut(id) {
                    s.push_snapshot(Pixels::from_bgra8(data.into()), extent);
                }
            });
        }

        if !execution.borrow().is_normal() {
            self.r.read(&self.screen_fb, move |data| {
                execution.borrow_mut().record(data);
            });
        }
    }

    fn handle_present_mode_changed(&mut self, present_mode: PresentMode) {
        if self.present_mode == present_mode {
            return;
        }
        self.present_mode = present_mode;
        self.swap_chain = self.r.swap_chain(
            self.swap_chain.width as u32,
            self.swap_chain.height as u32,
            present_mode.to_wgpu(),
        );
    }

    fn handle_scale_factor_changed(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
        self.handle_resized(self.win_size);
    }
}

impl Renderer {
    fn handle_effects(&mut self, mut effects: Vec<Effect>) {
        for eff in effects.drain(..) {
            // When switching views, or when the view is dirty (eg. it has been resized),
            // we have to resize the brush pipelines, for the brush strokes to
            // render properly in the view framebuffer. When a snapshot is restored,
            // the view size might also have changed, and therefore we resize
            // on "damaged" too.
            match eff {
                Effect::SessionResized(size) => {
                    self.handle_resized(size);
                }
                Effect::SessionScaled(scale) => {
                    self.handle_session_scale_changed(scale);
                }
                Effect::ViewActivated(_) => {}
                Effect::ViewAdded(id) => {
                    self.add_views(&[id]);
                }
                Effect::ViewRemoved(id) => {
                    self.view_data.remove(&id);
                }
                Effect::ViewOps(id, ops) => {
                    self.handle_view_ops(id, &ops);
                }
                Effect::ViewTouched(_) => {}
                Effect::ViewDamaged(id, extent) => {
                    self.handle_view_damaged(id, extent.width(), extent.height());
                }
                Effect::ViewBlendingChanged(blending) => {
                    self.blending = blending.to_wgpu();
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

    fn handle_view_damaged(&mut self, id: ViewId, vw: u32, vh: u32) {
        let fb = &self
            .view_data
            .get(&id)
            .expect("views must have associated view data")
            .fb;

        if fb.width() != vw || fb.height() != vh {
            return self.resize_view(id, vw, vh);
        }

        // View is damaged, but its size hasn't changed. This happens when a snapshot
        // with the same size as the view was restored.
        let pixels = {
            let rs = self.resources.lock();
            let (_, pixels) = rs.get_snapshot(id);
            pixels.to_owned()
        };
        self.r.submit(&[Op::Fill(fb, &pixels.into_bgra8())]);
    }

    fn resize_view(&mut self, id: ViewId, vw: u32, vh: u32) {
        // View size changed. Re-create view resources.
        // This condition is triggered when the size of the view doesn't match the size
        // of the view framebuffer. This can happen in two cases:
        //
        //   1. The view was resized (it's dirty).
        //   2. A snapshot was restored with a different size than the view (it's damaged).
        //
        // Either way, we handle it equally, by re-creating the view-data and restoring
        // the current snapshot.
        let view_data = ViewData::new(vw, vh, &self.framebuffer2d, &self.sprite2d, &self.r);

        // We don't want the lock to be held when `submit` is called below,
        // because in some cases it'll trigger the read-back which claims
        // a write lock on resources.
        let (sw, sh) = {
            let resources = self.resources.lock();
            let (snapshot, _) = resources.get_snapshot(id);
            (snapshot.width(), snapshot.height())
        };

        // Ensure not to transfer more data than can fit
        // in the view buffer.
        let tw = u32::min(sw, vw);
        let th = u32::min(sh, vh);

        let (_, texels) = self
            .resources
            .lock()
            .get_snapshot_rect(id, &Rect::origin(tw as i32, th as i32));

        self.r.submit(&[
            Op::Clear(&view_data.fb, Bgra8::TRANSPARENT),
            Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT),
            Op::Transfer(
                &view_data.fb,
                &Pixels::from_rgba8(texels.into()).into_bgra8(),
                tw, // Source width
                th, // Source height
                Rect::origin(tw as i32, th as i32),
            ),
        ]);
        self.view_data.insert(id, view_data);
    }

    fn handle_view_ops(&mut self, id: ViewId, ops: &[ViewOp]) {
        for op in ops {
            match op {
                ViewOp::Resize(w, h) => {
                    self.resize_view(id, *w, *h);
                }
                ViewOp::Clear(color) => {
                    let fb = &self
                        .view_data
                        .get(&id)
                        .expect("views must have associated view data")
                        .fb;

                    self.r.submit(&[Op::Clear(fb, (*color).into())]);
                }
                ViewOp::Blit(src, dst) => {
                    let fb = &self
                        .view_data
                        .get(&id)
                        .expect("views must have associated view data")
                        .fb;

                    self.r.submit(&[Op::Blit(fb, *src, *dst)]);
                }
                ViewOp::Yank(src) => {
                    let resources = self.resources.lock();
                    let (_, pixels) = resources.get_snapshot_rect(id, src);
                    let (w, h) = (src.width() as u32, src.height() as u32);

                    if self.paste.texture.w != w || self.paste.texture.h != h {
                        self.paste.texture = self.r.texture(w as u32, h as u32);
                        self.paste.binding =
                            self.paste2d
                                .binding(&self.r, &self.paste.texture, &self.sampler);
                    }
                    self.r.submit(&[Op::Fill(&self.paste.texture, &pixels)]);
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
                    .finish(&self.r);

                    self.paste.outputs.push(buffer);
                }
            }
        }
    }

    fn add_views(&mut self, views: &[ViewId]) {
        for id in views {
            let resources = self.resources.lock();
            if let Some((s, pixels)) = resources.get_snapshot_safe(*id) {
                let (w, h) = (s.width(), s.height());

                let view_data = ViewData::new(w, h, &self.framebuffer2d, &self.sprite2d, &self.r);

                debug_assert!(!pixels.is_empty());
                self.r.submit(&[
                    Op::Clear(&view_data.fb, Bgra8::TRANSPARENT),
                    Op::Clear(&view_data.staging_fb, Bgra8::TRANSPARENT),
                    Op::Fill(&view_data.fb, &pixels.clone().into_bgra8()),
                ]);

                self.view_data.insert(*id, view_data);
            }
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
            if let (Some(vb), Some(view)) = (&v.anim_vb, views.get(*id)) {
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

    fn render_help(&self, session: &Session, p: &mut core::Pass) {
        let mut win = shape2d::Batch::new();
        let mut text = TextBatch::new(
            self.font.width,
            self.font.height,
            self.font.gw,
            self.font.gh,
        );

        draw::draw_help(session, &mut text, &mut win);

        let win_buf = win.finish(&self.r);
        let text_buf = text.finish(&self.r);

        p.set_pipeline(&self.shape2d);
        p.draw_buffer(&win_buf);

        p.set_pipeline(&self.sprite2d);
        p.draw(&text_buf, &self.font.binding);
    }

    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        let physical = size.to_physical(self.scale_factor);

        self.swap_chain = self.r.swap_chain(
            physical.width as u32,
            physical.height as u32,
            self.present_mode.to_wgpu(),
        );
        self.win_size = size;
        self.handle_session_scale_changed(self.scale);
    }

    pub fn handle_session_scale_changed(&mut self, scale: f64) {
        let (fb_w, fb_h) = (self.win_size.width / scale, self.win_size.height / scale);

        self.screen_fb = self.r.framebuffer(fb_w as u32, fb_h as u32);
        self.screen_binding = self
            .screen2d
            .binding(&self.r, &self.screen_fb, &self.sampler);
        self.scale = scale;
    }

    fn update_view_transforms<'a, I>(&mut self, views: I, offset: Vector2<f32>, f: &mut core::Frame)
    where
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
            .update(self.view_transforms.as_slice(), &self.r, f);
    }

    fn update_view_animations(&mut self, s: &Session) {
        if !s.settings["animation"].is_set() {
            return;
        }
        for v in s.views.iter() {
            if !v.animation.is_playing() {
                continue;
            }
            // FIXME: When `v.animation.val()` doesn't change, we don't need
            // to re-create the buffer.
            let buf = draw::draw_view_animation(s, &v).finish(&self.r);

            if let Some(d) = self.view_data.get_mut(&v.id) {
                d.anim_vb = Some(buf);
            }
        }
    }
}
