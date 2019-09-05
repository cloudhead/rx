use crate::brush::BrushMode;
use crate::font::{Font, TextBatch};
use crate::framebuffer2d;
use crate::gpu;
use crate::platform;
use crate::resources::ResourceManager;
use crate::screen2d;
use crate::session::{self, Mode, Rgb8, Session, Tool};
use crate::view::{View, ViewId, ViewOp};

use rgx::core;
use rgx::core::{AbstractPipeline, Blending, Filter, Op, PassOp, Rect, Rgba};
use rgx::kit;
use rgx::kit::shape2d;
use rgx::kit::shape2d::{Fill, Line, Shape, Stroke};
use rgx::kit::sprite2d;
use rgx::kit::{Origin, Rgba8};

use cgmath::prelude::*;
use cgmath::{Matrix4, Vector2};

use std::collections::{BTreeMap, HashSet};
use std::time;

use image;
use image::ImageDecoder;

pub struct Renderer {
    pub width: u32,
    pub height: u32,

    active_view_id: ViewId,

    font: Font,
    cursors: Cursors,
    checker: Checker,

    view_transforms: Vec<Matrix4<f32>>,
    view_transforms_buf: gpu::TransformBuffer,

    sampler: core::Sampler,

    shape2d: kit::shape2d::Pipeline,
    sprite2d: kit::sprite2d::Pipeline,
    framebuffer2d: framebuffer2d::Pipeline,
    view2d: kit::shape2d::Pipeline,
    const2d: kit::shape2d::Pipeline,
    screen2d: screen2d::Pipeline,

    screen_fb: core::Framebuffer,
    screen_vb: core::VertexBuffer,
    screen_binding: core::BindingGroup,

    resources: ResourceManager,
    view_data: BTreeMap<ViewId, ViewData>,
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

pub struct ViewData {
    fb: core::Framebuffer,
    vb: core::VertexBuffer,
    binding: core::BindingGroup,
    anim_vb: Option<core::VertexBuffer>,
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
        let fb = r.framebuffer(w, h);
        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);
        let binding = framebuffer2d.binding(r, &fb, &sampler);
        let anim_binding = sprite2d.binding(r, &fb.texture, &sampler);
        let vb = framebuffer2d::Pipeline::vertex_buffer(w, h, r);

        ViewData {
            fb,
            vb,
            binding,
            anim_vb: None,
            anim_binding,
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

// TODO: Move all of these inside modules.
const CURSORS: &'static [u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/cursors.png"));
const GLYPHS: &'static [u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/glyphs.png"));

const GLYPH_WIDTH: f32 = 8.;
const GLYPH_HEIGHT: f32 = 14.;

const LINE_HEIGHT: f32 = GLYPH_HEIGHT + 4.;
const MARGIN: f32 = 10.;

impl Renderer {
    pub fn new(
        r: &mut core::Renderer,
        win_w: u32,
        win_h: u32,
        resources: ResourceManager,
    ) -> Self {
        let sprite2d: kit::sprite2d::Pipeline =
            r.pipeline(win_w, win_h, Blending::default());
        let shape2d: kit::shape2d::Pipeline =
            r.pipeline(win_w, win_h, Blending::default());
        let framebuffer2d: framebuffer2d::Pipeline =
            r.pipeline(win_w, win_h, Blending::default());
        let screen2d: screen2d::Pipeline =
            r.pipeline(win_w, win_h, Blending::default());

        let sampler = r.sampler(Filter::Nearest, Filter::Nearest);

        let view_transforms_buf = gpu::TransformBuffer::with_capacity(
            Session::MAX_VIEWS,
            &framebuffer2d.pipeline.layout.sets[1],
            &r.device,
        );
        let view_transforms = Vec::with_capacity(Session::MAX_VIEWS);

        let (font, font_img) = {
            let decoder = image::png::PNGDecoder::new(GLYPHS).unwrap();
            let (width, height) = decoder.dimensions();

            let img = decoder.read_image().unwrap();
            let texture = r.texture(width as u32, height as u32);
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
            let decoder = image::png::PNGDecoder::new(CURSORS).unwrap();
            let (width, height) = decoder.dimensions();

            let img = decoder.read_image().unwrap();
            let texture = r.texture(width as u32, height as u32);
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

        let view2d = r.pipeline(
            Session::DEFAULT_VIEW_W,
            Session::DEFAULT_VIEW_H,
            Blending::default(),
        );
        let const2d = r.pipeline(
            Session::DEFAULT_VIEW_W,
            Session::DEFAULT_VIEW_H,
            Blending::constant(),
        );

        let screen_vb = screen2d::Pipeline::vertex_buffer(r);
        let screen_fb = r.framebuffer(win_w, win_h);
        let screen_binding = screen2d.binding(r, &screen_fb, &sampler);

        r.prepare(&[
            Op::Fill(&font.texture, font_img.as_slice()),
            Op::Fill(&cursors.texture, cursors_img.as_slice()),
            Op::Fill(&checker.texture, &checker_img),
        ]);

        Self {
            width: win_w,
            height: win_h,
            font,
            cursors,
            checker,
            view_transforms,
            view_transforms_buf,
            sampler,
            shape2d,
            sprite2d,
            framebuffer2d,
            view2d,
            const2d,
            screen2d,
            resources,
            screen_fb,
            screen_vb,
            screen_binding,
            view_data: BTreeMap::new(),
            active_view_id: ViewId(0),
        }
    }

    pub fn init(&mut self, session: &Session, r: &mut core::Renderer) {
        self.update_views(session, r);
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
                session::VERSION,
                platform::Key::Escape,
            ),
            left_margin,
            self.height as f32 - self::MARGIN - self::LINE_HEIGHT,
            Rgba8::new(0xaa, 0xaa, 0xaa, 0xff),
        );

        for (i, (display, kb)) in session
            .key_bindings
            .iter()
            .filter_map(|kb| kb.display.as_ref().map(|d| (d, kb)))
            .enumerate()
        {
            let y = self.height as f32 - (i + 4) as f32 * self::LINE_HEIGHT;

            text.add(
                display,
                left_margin,
                y,
                Rgba8::new(0xb1, 0x3e, 0x53, 0xff),
            );
            text.add(
                &format!("{}", kb.command),
                left_margin + column_offset,
                y,
                Rgba8::new(0xaa, 0xaa, 0xaa, 0xff),
            );
        }
        for (i, l) in session::HELP.lines().enumerate() {
            let y = self.height as f32 - (i + 4) as f32 * self::LINE_HEIGHT;

            text.add(
                l,
                left_margin + column_offset * 3. + 64.,
                y,
                Rgba8::new(0x94, 0xb0, 0xc2, 0xff),
            );
        }
        let buf = text.finish(&r);

        let mut f = r.frame();
        r.update_pipeline(&self.sprite2d, Matrix4::identity(), &mut f);
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
        r.submit(f);
    }

    pub fn frame(
        &mut self,
        session: &Session,
        avg_frametime: &time::Duration,
        r: &mut core::Renderer,
        textures: &mut core::SwapChain,
        draw: &shape2d::Batch,
    ) {
        if !session.is_running {
            return;
        }
        if session.mode == Mode::Help {
            self.render_help(session, r, textures);
            return;
        }
        let out = &textures.next();

        let mut ui_batch = shape2d::Batch::new();
        let mut text_batch = TextBatch::new(&self.font);
        let mut cursor_batch = sprite2d::Batch::new(
            self.cursors.texture.w,
            self.cursors.texture.h,
        );
        let mut checker_batch = sprite2d::Batch::new(
            self.checker.texture.w,
            self.checker.texture.h,
        );

        Self::draw_palette(&session, &mut ui_batch);
        Self::draw_brush(&session, &mut ui_batch);
        Self::draw_ui(&session, avg_frametime, &mut ui_batch, &mut text_batch);
        Self::draw_cursor(&session, &mut cursor_batch);
        Self::draw_checker(&session, &mut checker_batch);

        let ui_buf = ui_batch.finish(&r);
        let cursor_buf = cursor_batch.finish(&r);
        let checker_buf = checker_batch.finish(&r);
        let text_buf = text_batch.finish(&r);
        let draw_buf = if draw.is_empty() {
            None
        } else {
            Some(draw.buffer(&r))
        };

        let v = session.active_view();
        if v.id != self.active_view_id || v.is_dirty() {
            self.resize_view_pipelines(v.width(), v.height());
            self.active_view_id = v.id;
        }
        if session.dirty || !v.is_okay() {
            self.update_views(session, r);
        }

        let mut f = r.frame();

        self.update_view_animations(session, r);
        self.update_view_transforms(
            session.views.values(),
            session.offset,
            &r,
            &mut f,
        );

        r.update_pipeline(&self.shape2d, Matrix4::identity(), &mut f);
        r.update_pipeline(&self.framebuffer2d, (), &mut f);

        {
            r.update_pipeline(&self.sprite2d, Matrix4::identity(), &mut f);
            let mut p =
                f.pass(PassOp::Clear(Rgba::TRANSPARENT), &self.screen_fb);

            // Draw view checkers.
            if session.settings["checker"].is_set() {
                p.set_pipeline(&self.sprite2d);
                p.draw(&checker_buf, &self.checker.binding);
            }
        }

        // Draw brush strokes to view framebuffers.
        if let Some(draw_buf) = draw_buf {
            let ViewData { fb: view_fb, .. } =
                self.view_data.get(&session.active_view_id).unwrap();

            r.update_pipeline(&self.view2d, Matrix4::identity(), &mut f);
            r.update_pipeline(&self.const2d, Matrix4::identity(), &mut f);

            let mut p = f.pass(
                if v.is_damaged() {
                    PassOp::Clear(Rgba::TRANSPARENT)
                } else {
                    PassOp::Load()
                },
                view_fb,
            );

            // FIXME: There must be a better way.
            if let Tool::Brush(ref b) = session.tool {
                if b.is_set(BrushMode::Erase) {
                    p.set_pipeline(&self.const2d);
                } else {
                    p.set_pipeline(&self.view2d);
                }
            }
            p.draw_buffer(&draw_buf);
        }

        // Draw view framebuffers to screen.
        r.update_pipeline(&self.sprite2d, session.transform(), &mut f);
        self.render_views(&mut f, &self.screen_fb);
        if session.settings["animation"].is_set() {
            self.render_view_animations(&mut f, &self.screen_fb);
        }

        {
            r.update_pipeline(&self.sprite2d, Matrix4::identity(), &mut f);

            let mut p = f.pass(PassOp::Load(), &self.screen_fb);

            // Draw UI elements to screen.
            p.set_pipeline(&self.shape2d);
            p.draw_buffer(&ui_buf);

            // Draw text & cursor to screen.
            p.set_pipeline(&self.sprite2d);
            p.draw(&text_buf, &self.font.binding);
            p.draw(&cursor_buf, &self.cursors.binding);
        }

        {
            // Render screen framebuffer to screen.
            let mut p = f.pass(PassOp::Clear(Rgba::TRANSPARENT), out);

            p.set_pipeline(&self.screen2d);
            p.set_binding(&self.screen_binding, &[]);
            p.draw_buffer(&self.screen_vb)
        }

        // Submit frame for presenting.
        r.submit(f);

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            let id = v.id;
            let nframes = v.animation.len();
            let fb = &self.view_data.get(&id).unwrap().fb;
            let resources = self.resources.clone();
            let (fw, fh) = (v.fw, v.fh);

            r.read(fb, move |data| {
                if let Some(s) = resources.lock_mut().data.get_mut(&id) {
                    s.push(data.to_owned(), fw, fh, nframes);
                }
            });
        }
    }

    pub fn resize_view_pipelines(&mut self, w: u32, h: u32) {
        assert!(
            self.view2d.width() == self.const2d.width()
                && self.view2d.height() == self.const2d.height(),
            "the view pipelines must always have the same size"
        );
        if self.view2d.width() != w || self.view2d.height() != h {
            self.view2d.resize(w, h);
            self.const2d.resize(w, h);
        }
    }

    pub fn resize_view_framebuffer(
        &mut self,
        v: &View,
        r: &mut core::Renderer,
    ) {
        self.resize_view_pipelines(v.width(), v.height());

        let resources = self.resources.lock();
        let snapshot = resources.get_snapshot(&v.id);
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        let (vw, vh) = (v.width(), v.height());
        let (sw, sh) = (snapshot.width(), snapshot.height());

        // View size changed. Re-create view resources.
        // This condition is triggered when the size of the view doesn't match the size
        // of the view framebuffer. This can happen in two cases:
        //
        //      1. The view was resized.
        //      2. A snapshot was restored with a different size than the view.
        //
        if fb.width() != vw || fb.height() != vh {
            let view_data =
                ViewData::new(vw, vh, &self.framebuffer2d, &self.sprite2d, r);

            // Ensure not to transfer more data than can fit
            // in the view buffer.
            let tw = u32::min(sw, vw);
            let th = u32::min(sh, vh);

            r.prepare(&[
                Op::Clear(&view_data.fb, Rgba::TRANSPARENT),
                Op::Transfer(
                    &view_data.fb,
                    snapshot.pixels.as_slice(),
                    sw, // Source width
                    sh, // Source height
                    tw, // Transfer width
                    th, // Transfer height
                ),
            ]);
            self.view_data.insert(v.id, view_data);
        } else if v.is_damaged() {
            r.prepare(&[Op::Fill(fb, snapshot.pixels.as_slice())]);
        }
    }

    pub fn handle_view_ops(&mut self, v: &View, r: &mut core::Renderer) {
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        for op in &v.ops {
            match op {
                ViewOp::Blit(src, dst) => {
                    r.prepare(&[Op::Blit(fb, *src, *dst)]);
                }
            }
        }
    }

    pub fn update_views(&mut self, session: &Session, r: &mut core::Renderer) {
        let data_keys: HashSet<ViewId> =
            self.view_data.keys().cloned().collect();
        let session_keys: HashSet<ViewId> =
            session.views.keys().cloned().collect();

        let added = session_keys.difference(&data_keys);
        let removed = data_keys.difference(&session_keys);

        for v in session.views.values() {
            if !v.is_okay() {
                self.resize_view_framebuffer(v, r);
                self.handle_view_ops(v, r);
            }
        }

        for id in added {
            let resources = self.resources.lock();
            let s = resources.get_snapshot(id);
            let (w, h) = (s.width(), s.height());

            let view_data =
                ViewData::new(w, h, &self.framebuffer2d, &self.sprite2d, r);

            assert!(!s.pixels.is_empty());
            r.prepare(&[Op::Fill(&view_data.fb, &s.pixels)]);

            self.view_data.insert(*id, view_data);
        }

        for id in removed {
            self.view_data.remove(id);
        }
    }

    fn draw_ui(
        session: &Session,
        avg_frametime: &time::Duration,
        canvas: &mut shape2d::Batch,
        text: &mut TextBatch,
    ) {
        for (_, v) in &session.views {
            // Frame lines
            for n in 0..v.animation.len() {
                let n = n as f32;
                canvas.add(Shape::Line(
                    // TODO: This shouldn't be so painful.
                    Line::new(
                        n * v.zoom * v.fw as f32
                            + v.offset.x
                            + session.offset.x,
                        session.offset.y + v.offset.y,
                        n * v.zoom * v.fw as f32
                            + v.offset.x
                            + session.offset.x,
                        v.zoom * v.fh as f32 + v.offset.y + session.offset.y,
                    ),
                    Stroke::new(1.0, Rgba::new(0.4, 0.4, 0.4, 1.0)),
                ));
            }
            // View border
            let border_color = if v.id == session.active_view_id {
                Rgba::WHITE
            } else {
                Rgba::new(0.5, 0.5, 0.5, 1.0)
            };
            canvas.add(Shape::Rectangle(
                v.rect() + session.offset,
                Stroke::new(1.0, border_color),
                Fill::Empty(),
            ));

            // View info
            text.add(
                &format!("{}x{}x{}", v.fw, v.fh, v.animation.len()),
                session.offset.x + v.offset.x,
                session.offset.y + v.offset.y - self::LINE_HEIGHT,
                Rgba8::new(0x88, 0x88, 0x88, 0xff),
            );
        }

        if session.settings["debug"].is_set() {
            // Frame-time
            text.add(
                &format!("{:3.2}ms", avg_frametime.as_micros() as f64 / 1000.),
                MARGIN,
                session.height - MARGIN - self::LINE_HEIGHT,
                Rgba8::WHITE,
            );
        }

        // Active view status
        text.add(
            &session.active_view().status(),
            MARGIN,
            MARGIN + self::LINE_HEIGHT,
            Rgba8::WHITE,
        );

        {
            // Session status
            let cursor = session.active_view_coords(session.cursor);
            let hover_color = session
                .hover_color
                .map_or(String::new(), |c| Rgb8::from(c).to_string());
            text.add(
                &format!("{:>4},{:<4} {}", cursor.x, cursor.y, hover_color),
                session.width - MARGIN - 36. * 8.,
                MARGIN + self::LINE_HEIGHT,
                Rgba8::WHITE,
            );
            text.add(
                &format!("{:>5}%", (session.active_view().zoom * 100.) as u32),
                session.width - MARGIN - 6. * 8.,
                MARGIN + self::LINE_HEIGHT,
                Rgba8::WHITE,
            );
            // Fg color
            canvas.add(Shape::Rectangle(
                Rect::origin(11., 11.)
                    .translate(300., self::LINE_HEIGHT + self::MARGIN + 2.),
                Stroke::new(1.0, Rgba::WHITE),
                Fill::Solid(session.fg.into()),
            ));
            // Bg color
            canvas.add(Shape::Rectangle(
                Rect::origin(11., 11.)
                    .translate(330., self::LINE_HEIGHT + self::MARGIN + 2.),
                Stroke::new(1.0, Rgba::WHITE),
                Fill::Solid(session.bg.into()),
            ));
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

    pub fn draw_palette(session: &Session, batch: &mut shape2d::Batch) {
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
            Mode::Normal | Mode::Command | Mode::Visual => {
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
                        rect.translate(cursor.x, cursor.y) + offset,
                        Rgba::TRANSPARENT,
                        1.,
                        kit::Repeat::default(),
                    );
                }
            }
        }
    }

    fn draw_brush(session: &Session, batch: &mut shape2d::Batch) {
        // TODO: Handle zoom by scaling everything in CPU?
        if let Tool::Brush(ref brush) = session.tool {
            let v = session.active_view();
            let p = session.cursor;
            let s = session.snap(p, v.offset.x, v.offset.y, v.zoom);

            // Draw enabled brush
            if v.contains(p - session.offset) {
                let (stroke, fill) = if brush.is_set(BrushMode::Erase) {
                    (Stroke::new(1.0, Rgba::WHITE), Fill::Empty())
                } else {
                    (Stroke::NONE, Fill::Solid(session.fg.into()))
                };

                if brush.is_set(BrushMode::Multi) {
                    let view_coords =
                        session.active_view_coords(session.cursor);
                    let frame_index = (view_coords.x as u32 / v.fw) as usize;

                    for i in 0..v.animation.len() - frame_index {
                        let offset = Vector2::new(
                            (i as f32 * v.fw as f32 * v.zoom).floor(),
                            0.,
                        );
                        batch.add(brush.stroke(
                            *(s + offset),
                            stroke,
                            fill,
                            v.zoom,
                            Origin::BottomLeft,
                        ));
                    }
                } else {
                    batch.add(brush.stroke(
                        *s,
                        stroke,
                        fill,
                        v.zoom,
                        Origin::BottomLeft,
                    ));
                }
            // Draw disabled brush
            } else {
                let color = if brush.is_set(BrushMode::Erase) {
                    Rgba8::new(128, 128, 128, 255)
                } else {
                    session.fg
                };
                batch.add(brush.stroke(
                    *p,
                    Stroke::new(1.0, color.into()),
                    Fill::Empty(),
                    v.zoom,
                    Origin::Center,
                ));
            }
        } else {
            // TODO
        }
    }

    fn render_views<T: core::TextureView>(&self, f: &mut core::Frame, out: &T) {
        let mut p = f.pass(PassOp::Load(), out);
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
        }
    }

    fn render_view_animations<T: core::TextureView>(
        &self,
        f: &mut core::Frame,
        out: &T,
    ) {
        let mut p = f.pass(PassOp::Load(), out);
        p.set_pipeline(&self.sprite2d);

        for (_, v) in self.view_data.iter() {
            if let Some(ref vb) = v.anim_vb {
                p.draw(vb, &v.anim_binding);
            }
        }
    }

    pub fn handle_resized(
        &mut self,
        size: platform::LogicalSize,
        r: &core::Renderer,
    ) {
        let (w, h) = (size.width as u32, size.height as u32);
        self.width = w;
        self.height = h;
        self.framebuffer2d.resize(w, h);
        self.sprite2d.resize(w, h);
        self.shape2d.resize(w, h);
        self.screen2d.resize(w, h);

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
        if s.paused || !s.settings["animation"].is_set() {
            return;
        }
        for (id, v) in &s.views {
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
