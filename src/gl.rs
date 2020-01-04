use crate::cursor2d;
use crate::draw;
use crate::execution::Execution;
use crate::font::TextBatch;
use crate::platform::{self, LogicalSize};
use crate::renderer;
use crate::resources::{Pixels, ResourceManager};
use crate::session::{self, Effect, Session};
use crate::view::{View, ViewId, ViewManager, ViewOp};
use crate::{data, image};

use rgx::core::{Blending, PresentMode, Rgba};
use rgx::kit::{self, shape2d, sprite2d, Bgra8, Origin, Rgba8, ZDepth};
use rgx::math::Matrix4;
use rgx::rect::Rect;

use luminance::blending::{Equation, Factor};
use luminance::context::GraphicsContext;
use luminance::depth_test::DepthComparison;
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::{BoundTexture, Builder, PipelineState};
use luminance::pixel;
use luminance::render_state::RenderState;
use luminance::shader::program::{Program, Uniform};
use luminance::state::GraphicsState;
use luminance::tess::{Mode, Tess, TessBuilder};
use luminance::texture::{Dim2, Flat, GenMipmaps, MagFilter, MinFilter, Sampler, Texture, Wrap};

use luminance_derive::{Semantics, UniformInterface, Vertex};

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::mem;
use std::rc::Rc;
use std::time;

const SAMPLER: Sampler = Sampler {
    wrap_r: Wrap::Repeat,
    wrap_s: Wrap::Repeat,
    wrap_t: Wrap::Repeat,
    min_filter: MinFilter::Nearest,
    mag_filter: MagFilter::Nearest,
    depth_comparison: None,
};

#[derive(UniformInterface)]
struct Sprite2dInterface {
    tex: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
    ortho: Uniform<[[f32; 4]; 4]>,
    transform: Uniform<[[f32; 4]; 4]>,
}

#[derive(Copy, Clone, Debug, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "position", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "uv", repr = "[f32; 2]", wrapper = "VertexUV")]
    UV,
    #[sem(name = "color", repr = "[u8; 4]", wrapper = "VertexColor")]
    Color,
    #[sem(name = "opacity", repr = "[f32; 1]", wrapper = "VertexOpacity")]
    Opacity,
    #[sem(name = "angle", repr = "[f32; 1]", wrapper = "VertexAngle")]
    Angle,
    #[sem(name = "center", repr = "[f32; 2]", wrapper = "VertexCenter")]
    Center,
}

#[derive(Vertex)]
#[vertex(sem = "VertexSemantics")]
struct Sprite2dVertex {
    #[allow(dead_code)]
    position: VertexPosition,
    #[allow(dead_code)]
    uv: VertexUV,
    #[allow(dead_code)]
    #[vertex(normalized = "true")]
    color: VertexColor,
    #[allow(dead_code)]
    opacity: VertexOpacity,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Shape2dInterface {
    ortho: Uniform<[[f32; 4]; 4]>,
    transform: Uniform<[[f32; 4]; 4]>,
}

#[derive(Vertex)]
#[vertex(sem = "VertexSemantics")]
struct Shape2dVertex {
    #[allow(dead_code)]
    position: VertexPosition,
    #[allow(dead_code)]
    angle: VertexAngle,
    #[allow(dead_code)]
    center: VertexCenter,
    #[allow(dead_code)]
    #[vertex(normalized = "true")]
    color: VertexColor,
}

#[derive(UniformInterface)]
struct Cursor2dInterface {
    cursor: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
    framebuffer: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
    ortho: Uniform<[[f32; 4]; 4]>,
    scale: Uniform<f32>,
}

#[derive(Vertex)]
#[vertex(sem = "VertexSemantics")]
struct Cursor2dVertex {
    #[allow(dead_code)]
    position: VertexPosition,
    #[allow(dead_code)]
    uv: VertexUV,
}

#[derive(UniformInterface)]
struct Screen2dInterface {
    #[allow(dead_code)]
    framebuffer: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
}

pub struct Renderer {
    pub win_size: LogicalSize,

    ctx: Context,
    hidpi_factor: f64,
    scale: f64,
    _present_mode: PresentMode,
    resources: ResourceManager,
    present_fb: Framebuffer<Flat, Dim2, (), ()>,
    screen_fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F>,
    render_st: RenderState,
    pipeline_st: PipelineState,
    blending: Blending,

    staging_batch: shape2d::Batch,
    final_batch: shape2d::Batch,

    font: Texture<Flat, Dim2, pixel::SRGBA8UI>,
    cursors: Texture<Flat, Dim2, pixel::SRGBA8UI>,
    checker: Texture<Flat, Dim2, pixel::SRGBA8UI>,
    paste: Texture<Flat, Dim2, pixel::SRGBA8UI>,
    paste_outputs: Vec<Tess>,
    paste_ready: bool,

    sprite2d: Program<VertexSemantics, (), Sprite2dInterface>,
    shape2d: Program<VertexSemantics, (), Shape2dInterface>,
    cursor2d: Program<VertexSemantics, (), Cursor2dInterface>,
    screen2d: Program<VertexSemantics, (), Screen2dInterface>,

    view_data: BTreeMap<ViewId, ViewData>,
}

struct ViewData {
    fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F>,
    #[allow(dead_code)]
    staging_fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F>,
    tess: Tess,
    anim_tess: Option<Tess>,
}

impl ViewData {
    fn new(w: u32, h: u32, pixels: Option<&[Rgba8]>, ctx: &mut Context) -> Self {
        let batch = sprite2d::Batch::singleton(
            w,
            h,
            Rect::origin(w as f32, h as f32),
            Rect::origin(w as f32, h as f32),
            ZDepth::default(),
            Rgba::TRANSPARENT,
            1.,
            kit::Repeat::default(),
        );

        let verts: Vec<Sprite2dVertex> = batch
            .vertices()
            .iter()
            .map(|v| unsafe { mem::transmute(*v) })
            .collect();
        let tess = TessBuilder::new(ctx)
            .add_vertices(verts)
            .set_mode(Mode::Triangle)
            .build()
            .unwrap();

        let fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F> =
            Framebuffer::new(ctx, [w, h], 0, self::SAMPLER).unwrap();
        let staging_fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F> =
            Framebuffer::new(ctx, [w, h], 0, self::SAMPLER).unwrap();

        fb.color_slot().clear(GenMipmaps::No, (0, 0, 0, 0)).unwrap();
        staging_fb
            .color_slot()
            .clear(GenMipmaps::No, (0, 0, 0, 0))
            .unwrap();

        if let Some(pixels) = pixels {
            let (head, aligned, tail) = unsafe { pixels.align_to::<u8>() };
            assert!(head.is_empty() && tail.is_empty());
            fb.color_slot().upload_raw(GenMipmaps::No, aligned).unwrap();
        }

        ViewData {
            fb,
            staging_fb,
            tess,
            anim_tess: None,
        }
    }
}

struct Context {
    gs: Rc<RefCell<GraphicsState>>,
}

unsafe impl GraphicsContext for Context {
    fn state(&self) -> &Rc<RefCell<GraphicsState>> {
        &self.gs
    }

    fn pipeline_builder(&mut self) -> Builder<Context> {
        Builder::new(self)
    }
}

impl renderer::Renderer for Renderer {
    fn new<T>(
        win: &mut platform::backend::Window<T>,
        win_size: LogicalSize,
        hidpi_factor: f64,
        _present_mode: PresentMode,
        resources: ResourceManager,
    ) -> io::Result<Self> {
        gl::load_with(|s| win.get_proc_address(s) as *const _);

        let gs = GraphicsState::new().unwrap();
        let mut ctx = Context {
            gs: Rc::new(RefCell::new(gs)),
        };

        let (font_img, font_w, font_h) = image::decode(data::GLYPHS)?;
        let (cursors_img, cursors_w, cursors_h) = image::decode(data::CURSORS)?;

        let font = Texture::new(&mut ctx, [font_w, font_h], 0, self::SAMPLER).unwrap();
        let cursors = Texture::new(&mut ctx, [cursors_w, cursors_h], 0, self::SAMPLER).unwrap();
        let paste = Texture::new(&mut ctx, [8, 8], 0, self::SAMPLER).unwrap();
        let checker = Texture::new(&mut ctx, [2, 2], 0, self::SAMPLER).unwrap();

        font.upload_raw(GenMipmaps::No, &font_img).unwrap();
        cursors.upload_raw(GenMipmaps::No, &cursors_img).unwrap();
        checker.upload_raw(GenMipmaps::No, &draw::CHECKER).unwrap();

        let sprite2d = self::program::<Sprite2dInterface>(
            include_str!("gl/data/sprite.vert"),
            include_str!("gl/data/sprite.frag"),
        );
        let shape2d = self::program::<Shape2dInterface>(
            include_str!("gl/data/shape.vert"),
            include_str!("gl/data/shape.frag"),
        );
        let cursor2d = self::program::<Cursor2dInterface>(
            include_str!("gl/data/cursor.vert"),
            include_str!("gl/data/cursor.frag"),
        );
        let screen2d = self::program::<Screen2dInterface>(
            include_str!("gl/data/screen.vert"),
            include_str!("gl/data/screen.frag"),
        );

        let physical = win_size.to_physical(hidpi_factor);
        let present_fb =
            Framebuffer::back_buffer(&mut ctx, [physical.width as u32, physical.height as u32]);
        let screen_fb = Framebuffer::new(
            &mut ctx,
            [win_size.width as u32, win_size.height as u32],
            0,
            self::SAMPLER,
        )
        .unwrap();

        let render_st = RenderState::default()
            .set_blending((
                Equation::Additive,
                Factor::SrcAlpha,
                Factor::SrcAlphaComplement,
            ))
            .set_depth_test(Some(DepthComparison::LessOrEqual));
        let pipeline_st = PipelineState::default()
            .set_clear_color([0., 0., 0., 0.])
            .enable_srgb(true)
            .enable_clear_depth(true)
            .enable_clear_color(true);

        Ok(Renderer {
            ctx,
            win_size,
            hidpi_factor,
            scale: 1.0,
            _present_mode,
            blending: Blending::default(),
            resources,
            present_fb,
            screen_fb,
            render_st,
            pipeline_st,
            sprite2d,
            shape2d,
            cursor2d,
            screen2d,
            font,
            cursors,
            checker,
            paste,
            paste_outputs: Vec::new(),
            paste_ready: false,
            staging_batch: shape2d::Batch::new(),
            final_batch: shape2d::Batch::new(),
            view_data: BTreeMap::new(),
        })
    }

    fn init(&mut self, effects: Vec<Effect>, views: &ViewManager) {
        self.handle_effects(effects, &views);
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

        self.handle_effects(effects, &session.views);
        self.update_view_animations(session);

        let ortho = kit::ortho(
            self.screen_fb.width(),
            self.screen_fb.height(),
            Origin::TopLeft,
        );
        let ortho_flip = kit::ortho(
            self.screen_fb.width(),
            self.screen_fb.height(),
            Origin::BottomLeft,
        );
        let identity: Matrix4<f32> = Matrix4::identity();

        let mut ctx = draw::DrawContext {
            ui_batch: shape2d::Batch::new(),
            text_batch: TextBatch::new(
                self.font.size()[0],
                self.font.size()[1],
                draw::GLYPH_WIDTH,
                draw::GLYPH_HEIGHT,
            ),
            overlay_batch: TextBatch::new(
                self.font.size()[0],
                self.font.size()[1],
                draw::GLYPH_WIDTH,
                draw::GLYPH_HEIGHT,
            ),
            cursor_sprite: cursor2d::Sprite::new(self.cursors.size()[0], self.cursors.size()[1]),
            tool_batch: sprite2d::Batch::new(self.cursors.size()[0], self.cursors.size()[1]),
            paste_batch: sprite2d::Batch::new(self.paste.size()[0], self.paste.size()[1]),
            checker_batch: sprite2d::Batch::new(self.checker.size()[0], self.checker.size()[1]),
        };

        // Handle view operations.
        for v in session.views.values() {
            if !v.ops.is_empty() {
                self.handle_view_ops(&v);
            }
        }

        let font = &self.font;
        let cursors = &self.cursors;
        let checker = &self.checker;
        let sprite2d = &self.sprite2d;
        let shape2d = &self.shape2d;
        let cursor2d = &self.cursor2d;
        let screen2d = &self.screen2d;
        let present_fb = &self.present_fb;
        let blending = &self.blending;
        let screen_fb = &self.screen_fb;
        let render_st = &self.render_st;
        let pipeline_st = self.pipeline_st.clone();
        let paste = &self.paste;
        let paste_outputs = &mut self.paste_outputs;
        let mut paste_ready = self.paste_ready;
        let view_data = &mut self.view_data;

        ctx.draw(&session, avg_frametime, execution.clone());

        let text_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            ctx.text_batch.raw.vertices().as_slice(),
        );
        let overlay_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            ctx.overlay_batch.raw.vertices().as_slice(),
        );
        let ui_tess = self::tessellation::<_, Shape2dVertex>(
            &mut self.ctx,
            ctx.ui_batch.vertices().as_slice(),
        );
        let tool_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            ctx.tool_batch.vertices().as_slice(),
        );
        let cursor_tess = self::tessellation::<_, Cursor2dVertex>(
            &mut self.ctx,
            ctx.cursor_sprite.vertices().as_slice(),
        );
        let checker_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            ctx.checker_batch.vertices().as_slice(),
        );
        let screen_tess = TessBuilder::new(&mut self.ctx)
            .set_vertex_nb(6)
            .set_mode(Mode::Triangle)
            .build()
            .unwrap();

        let paste_tess = if ctx.paste_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Sprite2dVertex>(
                &mut self.ctx,
                ctx.paste_batch.vertices().as_slice(),
            ))
        };
        let staging_tess = if self.staging_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Shape2dVertex>(
                &mut self.ctx,
                self.staging_batch.vertices().as_slice(),
            ))
        };
        let final_tess = if self.final_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Shape2dVertex>(
                &mut self.ctx,
                self.final_batch.vertices().as_slice(),
            ))
        };

        let help_tess = if session.mode == session::Mode::Help {
            let mut win = shape2d::Batch::new();
            let mut text = TextBatch::new(
                font.size()[0],
                font.size()[1],
                draw::GLYPH_WIDTH,
                draw::GLYPH_HEIGHT,
            );
            draw::draw_help(session, &mut text, &mut win);

            let win_tess =
                self::tessellation::<_, Shape2dVertex>(&mut self.ctx, win.vertices().as_slice());
            let text_tess = self::tessellation::<_, Sprite2dVertex>(
                &mut self.ctx,
                text.raw.vertices().as_slice(),
            );
            Some((win_tess, text_tess))
        } else {
            None
        };

        let v = session.active_view();
        let v_data = view_data.get(&v.id).unwrap();
        let view_ortho = kit::ortho(v.width(), v.height(), Origin::TopLeft);

        let mut builder = self.ctx.pipeline_builder();

        // Render to view staging buffer.
        builder.pipeline(
            &v_data.staging_fb,
            &pipeline_st,
            |pipeline, mut shd_gate| {
                // Render staged brush strokes.
                if let Some(tess) = staging_tess {
                    shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                        iface.ortho.update(unsafe { mem::transmute(view_ortho) });
                        iface.transform.update(unsafe { mem::transmute(identity) });

                        rdr_gate.render(render_st, |mut tess_gate| {
                            tess_gate.render(&tess);
                        });
                    });
                }
                // Render staging paste buffer.
                if let Some(tess) = paste_tess {
                    if paste_ready {
                        let bound_paste = pipeline.bind_texture(paste);
                        shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                            iface.ortho.update(unsafe { mem::transmute(view_ortho) });
                            iface.transform.update(unsafe { mem::transmute(identity) });
                            iface.tex.update(&bound_paste);

                            rdr_gate.render(render_st, |mut tess_gate| {
                                tess_gate.render(&tess);
                            });
                        });
                    } else {
                        paste_ready = true;
                    }
                }
            },
        );

        // Render to view final buffer.
        builder.pipeline(
            &v_data.fb,
            &pipeline_st.clone().enable_clear_color(false),
            |pipeline, mut shd_gate| {
                let bound_paste = pipeline.bind_texture(paste);

                // Render final brush strokes.
                if let Some(tess) = final_tess {
                    shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                        iface.ortho.update(unsafe { mem::transmute(view_ortho) });
                        iface.transform.update(unsafe { mem::transmute(identity) });

                        let render_st = if blending == &Blending::constant() {
                            render_st.clone().set_blending((
                                Equation::Additive,
                                Factor::One,
                                Factor::Zero,
                            ))
                        } else {
                            render_st.clone()
                        };

                        rdr_gate.render(&render_st, |mut tess_gate| {
                            tess_gate.render(&tess);
                        });
                    });
                }
                if !paste_outputs.is_empty() {
                    shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                        iface.ortho.update(unsafe { mem::transmute(view_ortho) });
                        iface.transform.update(unsafe { mem::transmute(identity) });
                        iface.tex.update(&bound_paste);

                        for out in paste_outputs.drain(..) {
                            rdr_gate.render(render_st, |mut tess_gate| {
                                tess_gate.render(&out);
                            });
                        }
                    });
                }
            },
        );

        // Render to screen framebuffer.
        builder.pipeline(screen_fb, &pipeline_st, |pipeline, mut shd_gate| {
            // Draw view checkers to screen framebuffer.
            if session.settings["checker"].is_set() {
                shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                    let bound_checker = pipeline.bind_texture(checker);

                    iface.tex.update(&bound_checker);
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&checker_tess);
                    });
                });
            }

            for (id, v) in view_data.iter() {
                if let Some(view) = session.views.get(id) {
                    let bound_view = pipeline.bind_texture(v.fb.color_slot());
                    let bound_view_staging = pipeline.bind_texture(v.staging_fb.color_slot());
                    let transform = Matrix4::from_translation(
                        (session.offset + view.offset).extend(*draw::VIEW_LAYER),
                    ) * Matrix4::from_nonuniform_scale(view.zoom, view.zoom, 1.0);

                    // Render views.
                    shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                        iface.tex.update(&bound_view);
                        iface.ortho.update(unsafe { mem::transmute(ortho) });
                        iface.transform.update(unsafe { mem::transmute(transform) });

                        rdr_gate.render(render_st, |mut tess_gate| {
                            tess_gate.render(&v.tess);
                        });

                        iface.tex.update(&bound_view_staging);
                        rdr_gate.render(render_st, |mut tess_gate| {
                            tess_gate.render(&v.tess);
                        });
                    });
                }
            }

            // Render UI.
            shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                iface.ortho.update(unsafe { mem::transmute(ortho) });
                iface.transform.update(unsafe { mem::transmute(identity) });

                rdr_gate.render(render_st, |mut tess_gate| {
                    tess_gate.render(&ui_tess);
                });
            });

            // Render text, tool & view animations.
            shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                {
                    let bound_font = pipeline.bind_texture(font);

                    iface.tex.update(&bound_font);
                    iface.ortho.update(unsafe { mem::transmute(ortho) });
                    iface.transform.update(unsafe { mem::transmute(identity) });

                    // Render text.
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&text_tess);
                    });
                }
                {
                    let bound_tool = pipeline.bind_texture(cursors);
                    iface.tex.update(&bound_tool);

                    // Render text.
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&tool_tess);
                    });
                }

                // Render view animations.
                if session.settings["animation"].is_set() {
                    for (id, v) in view_data.iter() {
                        match (&v.anim_tess, session.views.get(id)) {
                            (Some(tess), Some(view)) if view.animation.len() > 1 => {
                                let bound_view = pipeline.bind_texture(&v.fb.color_slot());

                                iface.tex.update(&bound_view);

                                rdr_gate.render(render_st, |mut tess_gate| {
                                    tess_gate.render(tess);
                                });
                            }
                            _ => (),
                        }
                    }
                }
            });

            // Render help.
            if let Some((win_tess, text_tess)) = help_tess {
                shd_gate.shade(&shape2d, |_iface, mut rdr_gate| {
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&win_tess);
                    });
                });
                shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                    let bound_font = pipeline.bind_texture(font);

                    iface.tex.update(&bound_font);
                    iface.ortho.update(unsafe { mem::transmute(ortho) });
                    iface.transform.update(unsafe { mem::transmute(identity) });

                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&text_tess);
                    });
                });
            }
        });

        // Render to back buffer.
        builder.pipeline(present_fb, &pipeline_st, |pipeline, mut shd_gate| {
            // Render screen framebuffer.
            let bound_screen = pipeline.bind_texture(screen_fb.color_slot());
            shd_gate.shade(&screen2d, |iface, mut rdr_gate| {
                iface.framebuffer.update(&bound_screen);

                rdr_gate.render(render_st, |mut tess_gate| {
                    tess_gate.render(&screen_tess);
                });
            });

            if session.settings["debug"].is_set() || !execution.borrow().is_normal() {
                let bound_font = pipeline.bind_texture(font);

                shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                    iface.tex.update(&bound_font);
                    iface.ortho.update(unsafe { mem::transmute(ortho_flip) });

                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&overlay_tess);
                    });
                });
            }

            // Render cursor.
            let bound_cursors = pipeline.bind_texture(cursors);
            shd_gate.shade(&cursor2d, |iface, mut rdr_gate| {
                iface.cursor.update(&bound_cursors);
                iface.framebuffer.update(&bound_screen);
                iface.ortho.update(unsafe { mem::transmute(ortho) });
                iface.scale.update(session.settings["scale"].clone().into());

                rdr_gate.render(render_st, |mut tess_gate| {
                    tess_gate.render(&cursor_tess);
                });
            });
        });

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            if let Some(s) = self.resources.lock_mut().get_view_mut(v.id) {
                let texels = v_data.fb.color_slot().get_raw_texels();
                s.push_snapshot(Pixels::Rgba(Rgba8::align(&texels).into()), v.extent());
            }
        }

        if !execution.borrow().is_normal() {
            let texels = screen_fb.color_slot().get_raw_texels();
            let texels = Rgba8::align(&texels);

            execution
                .borrow_mut()
                .record(&texels.iter().cloned().map(Bgra8::from).collect::<Vec<_>>());
        }

        self.paste_ready = paste_ready;
    }

    fn update_present_mode(&mut self, _present_mode: PresentMode) {}
}

impl Renderer {
    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        let physical = size.to_physical(self.hidpi_factor);

        self.present_fb = Framebuffer::back_buffer(
            &mut self.ctx,
            [physical.width as u32, physical.height as u32],
        );
        self.win_size = size;
        self.handle_scaled(self.scale);
    }

    pub fn handle_scaled(&mut self, scale: f64) {
        self.scale = scale;
        self.screen_fb = Framebuffer::new(
            &mut self.ctx,
            [
                (self.win_size.width / scale) as u32,
                (self.win_size.height / scale) as u32,
            ],
            0,
            self::SAMPLER,
        )
        .unwrap();
    }

    fn handle_effects(&mut self, mut effects: Vec<Effect>, views: &ViewManager) {
        for eff in effects.drain(..) {
            match eff {
                Effect::SessionResized(size) => {
                    self.handle_resized(size);
                }
                Effect::SessionScaled(scale) => {
                    self.handle_scaled(scale);
                }
                Effect::ViewActivated(_) => {}
                Effect::ViewAdded(id) => {
                    let resources = self.resources.lock();
                    let (s, pixels) = resources.get_snapshot(id);
                    let (w, h) = (s.width(), s.height());

                    self.view_data.insert(
                        id,
                        ViewData::new(w, h, Some(&pixels.clone().into_rgba8()), &mut self.ctx),
                    );
                }
                Effect::ViewRemoved(id) => {
                    self.view_data.remove(&id);
                }
                Effect::ViewTouched(id) | Effect::ViewDamaged(id) => {
                    let v = views.get(&id).expect("view must exist");
                    self.handle_view_dirty(v);
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

    fn handle_view_ops(&mut self, v: &View) {
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        for op in &v.ops {
            match op {
                ViewOp::Clear(color) => {
                    fb.color_slot()
                        .clear(GenMipmaps::No, (color.r, color.g, color.b, color.a))
                        .unwrap();
                }
                ViewOp::Blit(src, dst) => {
                    let texels = self
                        .resources
                        .lock()
                        .get_snapshot_rect(v.id, &src.map(|n| n as i32));

                    let (head, texels, tail) = unsafe { texels.align_to::<u8>() };
                    assert!(head.is_empty() && tail.is_empty());

                    fb.color_slot()
                        .upload_part_raw(
                            GenMipmaps::No,
                            [dst.x1 as u32, dst.y1 as u32],
                            [src.width() as u32, src.height() as u32],
                            &texels,
                        )
                        .unwrap();
                }
                ViewOp::Yank(src) => {
                    let resources = self.resources.lock();
                    let pixels = resources.get_snapshot_rect(v.id, src);
                    let (w, h) = (src.width() as u32, src.height() as u32);
                    let [paste_w, paste_h] = self.paste.size();

                    if paste_w != w || paste_h != h {
                        self.paste_ready = false;
                        self.paste =
                            Texture::new(&mut self.ctx, [w as u32, h as u32], 0, self::SAMPLER)
                                .unwrap();
                    }
                    let (head, body, tail) = unsafe { pixels.align_to::<u8>() };
                    assert!(head.is_empty() && tail.is_empty());

                    self.paste.upload_raw(GenMipmaps::No, body).unwrap();
                }
                ViewOp::Paste(dst) => {
                    let [paste_w, paste_h] = self.paste.size();
                    let batch = sprite2d::Batch::singleton(
                        paste_w,
                        paste_h,
                        Rect::origin(paste_w as f32, paste_h as f32),
                        dst.map(|n| n as f32),
                        ZDepth::default(),
                        Rgba::TRANSPARENT,
                        1.,
                        kit::Repeat::default(),
                    );

                    self.paste_outputs
                        .push(self::tessellation::<_, Sprite2dVertex>(
                            &mut self.ctx,
                            batch.vertices().as_slice(),
                        ));
                }
            }
        }
    }

    fn handle_view_dirty(&mut self, v: &View) {
        let fb = &self
            .view_data
            .get(&v.id)
            .expect("views must have associated view data")
            .fb;

        let (vw, vh) = (v.width(), v.height());

        if fb.width() != vw || fb.height() != vh {
            // View size changed. Re-create view resources.
            let (sw, sh) = {
                let resources = self.resources.lock();
                let (snapshot, _) = resources.get_snapshot(v.id);
                (snapshot.width(), snapshot.height())
            };

            // Ensure not to transfer more data than can fit in the view buffer.
            let tw = u32::min(sw, vw);
            let th = u32::min(sh, vh);

            let view_data = ViewData::new(vw, vh, None, &mut self.ctx);

            let texels = self
                .resources
                .lock()
                .get_snapshot_rect(v.id, &Rect::origin(tw as i32, th as i32));

            let (head, texels, tail) = unsafe { texels.align_to::<u8>() };
            assert!(head.is_empty() && tail.is_empty());

            view_data
                .fb
                .color_slot()
                .clear(GenMipmaps::No, (0, 0, 0, 0))
                .unwrap();
            view_data
                .staging_fb
                .color_slot()
                .clear(GenMipmaps::No, (0, 0, 0, 0))
                .unwrap();
            view_data
                .fb
                .color_slot()
                .upload_part_raw(GenMipmaps::No, [0, vh - th], [tw, th], texels)
                .unwrap();

            self.view_data.insert(v.id, view_data);
        } else if v.is_damaged() {
            // View is damaged, but its size hasn't changed. This happens when a snapshot
            // with the same size as the view was restored.
            let pixels = {
                let rs = self.resources.lock();
                let (_, pixels) = rs.get_snapshot(v.id);
                pixels.to_owned()
            };

            fb.color_slot().clear(GenMipmaps::No, (0, 0, 0, 0)).unwrap();
            fb.color_slot()
                .upload_raw(GenMipmaps::No, pixels.as_bytes())
                .unwrap();
        }
    }

    fn update_view_animations(&mut self, s: &Session) {
        if !s.settings["animation"].is_set() {
            return;
        }
        for (id, v) in s.views.iter() {
            if !v.animation.is_playing() {
                continue;
            }
            // FIXME: When `v.animation.val()` doesn't change, we don't need
            // to re-create the buffer.
            let batch = draw::draw_view_animation(s, &v);

            if let Some(vd) = self.view_data.get_mut(&id) {
                vd.anim_tess = Some(self::tessellation::<_, Sprite2dVertex>(
                    &mut self.ctx,
                    batch.vertices().as_slice(),
                ));
            }
        }
    }
}

fn tessellation<T, S>(ctx: &mut Context, verts: &[T]) -> Tess
where
    S: luminance::vertex::Vertex + Sized,
{
    let (head, body, tail) = unsafe { verts.align_to::<S>() };

    assert!(head.is_empty());
    assert!(tail.is_empty());

    TessBuilder::new(ctx)
        .add_vertices(body)
        .set_mode(Mode::Triangle)
        .build()
        .unwrap()
}

fn program<T>(vert: &str, frag: &str) -> Program<VertexSemantics, (), T>
where
    T: luminance::shader::program::UniformInterface,
{
    Program::<VertexSemantics, (), T>::from_strings(None, vert, None, frag)
        .unwrap()
        .ignore_warnings()
}
