use crate::draw;
use crate::execution::Execution;
use crate::font::TextBatch;
use crate::platform::{self, LogicalSize};
use crate::renderer;
use crate::resources::{Pixels, ResourceManager};
use crate::session::{self, Blending, Effect, PresentMode, Session};
use crate::sprite;
use crate::util;
use crate::view::layer::{FrameRange, LayerId};
use crate::view::{ViewId, ViewOp, ViewState};
use crate::{data, data::Assets, image};

use nonempty::NonEmpty;

use rgx::kit::{self, shape2d, sprite2d, Bgra8, Origin, Rgba, Rgba8, ZDepth};
use rgx::math::{Matrix4, Vector2};
use rgx::rect::Rect;

use luminance::blending::{Equation, Factor};
use luminance::context::GraphicsContext;
use luminance::depth_test::DepthComparison;
use luminance::framebuffer::Framebuffer;
use luminance::linear;
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
use std::error::Error;
use std::fmt;
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
    ortho: Uniform<linear::M44>,
    transform: Uniform<linear::M44>,
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
#[rustfmt::skip]
struct Sprite2dVertex {
    #[allow(dead_code)] position: VertexPosition,
    #[allow(dead_code)] uv: VertexUV,
    #[vertex(normalized = "true")]
    #[allow(dead_code)] color: VertexColor,
    #[allow(dead_code)] opacity: VertexOpacity,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Shape2dInterface {
    ortho: Uniform<linear::M44>,
    transform: Uniform<linear::M44>,
}

#[derive(Vertex)]
#[vertex(sem = "VertexSemantics")]
#[rustfmt::skip]
struct Shape2dVertex {
    #[allow(dead_code)] position: VertexPosition,
    #[allow(dead_code)] angle: VertexAngle,
    #[allow(dead_code)] center: VertexCenter,
    #[vertex(normalized = "true")]
    #[allow(dead_code)] color: VertexColor,
}

#[derive(UniformInterface)]
struct Cursor2dInterface {
    cursor: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
    framebuffer: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
    ortho: Uniform<linear::M44>,
    scale: Uniform<f32>,
}

#[derive(Vertex)]
#[vertex(sem = "VertexSemantics")]
#[rustfmt::skip]
struct Cursor2dVertex {
    #[allow(dead_code)] position: VertexPosition,
    #[allow(dead_code)] uv: VertexUV,
}

#[derive(UniformInterface)]
struct Screen2dInterface {
    framebuffer: Uniform<&'static BoundTexture<'static, Flat, Dim2, pixel::NormUnsigned>>,
}

pub struct Renderer {
    pub win_size: LogicalSize,

    ctx: Context,
    draw_ctx: draw::Context,
    scale_factor: f64,
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

    sprite2d: Program<VertexSemantics, (), Sprite2dInterface>,
    shape2d: Program<VertexSemantics, (), Shape2dInterface>,
    cursor2d: Program<VertexSemantics, (), Cursor2dInterface>,
    screen2d: Program<VertexSemantics, (), Screen2dInterface>,

    view_data: BTreeMap<ViewId, ViewData>,
}

struct LayerData {
    fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F>,
    tess: Tess,
}

impl LayerData {
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

        fb.color_slot().clear(GenMipmaps::No, (0, 0, 0, 0)).unwrap();

        if let Some(pixels) = pixels {
            let aligned = util::align_u8(pixels);
            fb.color_slot().upload_raw(GenMipmaps::No, aligned).unwrap();
        }

        Self { fb, tess }
    }

    fn clear(&self) -> Result<(), RendererError> {
        self.fb
            .color_slot()
            .clear(GenMipmaps::No, (0, 0, 0, 0))
            .map_err(RendererError::Texture)
    }

    fn upload_part(
        &self,
        offset: [u32; 2],
        size: [u32; 2],
        texels: &[u8],
    ) -> Result<(), RendererError> {
        self.fb
            .color_slot()
            .upload_part_raw(GenMipmaps::No, offset, size, texels)
            .map_err(RendererError::Texture)
    }

    fn upload(&self, texels: &[u8]) -> Result<(), RendererError> {
        self.fb
            .color_slot()
            .upload_raw(GenMipmaps::No, texels)
            .map_err(RendererError::Texture)
    }

    fn pixels(&self) -> Pixels {
        let texels = self.fb.color_slot().get_raw_texels();
        Pixels::from_rgba8(Rgba8::align(&texels).into())
    }
}

struct ViewData {
    layers: NonEmpty<LayerData>,
    staging_fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F>,
    anim_tess: Option<Tess>,
    layer_tess: Option<Tess>,
    w: u32,
    h: u32,
}

impl ViewData {
    fn new(w: u32, h: u32, pixels: Option<&[Rgba8]>, ctx: &mut Context) -> Self {
        let staging_fb: Framebuffer<Flat, Dim2, pixel::SRGBA8UI, pixel::Depth32F> =
            Framebuffer::new(ctx, [w, h], 0, self::SAMPLER).unwrap();

        staging_fb
            .color_slot()
            .clear(GenMipmaps::No, (0, 0, 0, 0))
            .unwrap();

        Self {
            layers: NonEmpty::new(LayerData::new(w, h, pixels, ctx)),
            staging_fb,
            anim_tess: None,
            layer_tess: None,
            w,
            h,
        }
    }

    fn layer_pixels(&self) -> impl Iterator<Item = (LayerId, Pixels)> + '_ {
        self.layers.iter().enumerate().map(|(i, l)| (i, l.pixels()))
    }

    fn get_layer(&self, index: usize) -> &LayerData {
        self.layers
            .get(index)
            .unwrap_or_else(|| panic!("layer #{} must exist", index))
    }

    fn add_layer(
        &mut self,
        _range: &FrameRange,
        pixels: Option<&[Rgba8]>,
        ctx: &mut Context,
    ) -> LayerId {
        let layer = LayerData::new(self.w, self.h, pixels, ctx);
        self.layers.push(layer);

        self.layers.len() - 1
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

#[derive(Debug)]
enum RendererError {
    Initialization,
    Texture(luminance::texture::TextureError),
    Framebuffer(luminance::framebuffer::FramebufferError),
    State(luminance::state::StateQueryError),
}

impl From<RendererError> for io::Error {
    fn from(err: RendererError) -> io::Error {
        io::Error::new(io::ErrorKind::Other, err)
    }
}

impl fmt::Display for RendererError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::Initialization => write!(f, "initialization error"),
            Self::Texture(e) => write!(f, "texture error: {}", e),
            Self::Framebuffer(e) => write!(f, "framebuffer error: {}", e),
            Self::State(e) => write!(f, "state error: {}", e),
        }
    }
}

impl Error for RendererError {
    fn description(&self) -> &str {
        "Renderer error"
    }

    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

impl<'a> renderer::Renderer<'a> for Renderer {
    fn new(
        win: &mut platform::backend::Window,
        win_size: LogicalSize,
        scale_factor: f64,
        _present_mode: PresentMode,
        resources: ResourceManager,
        assets: Assets<'a>,
    ) -> io::Result<Self> {
        use RendererError as Error;

        gl::load_with(|s| win.get_proc_address(s) as *const _);

        let gs = GraphicsState::new().map_err(Error::State)?;
        let mut ctx = Context {
            gs: Rc::new(RefCell::new(gs)),
        };

        let (font_img, font_w, font_h) = image::read(assets.glyphs)?;
        let (cursors_img, cursors_w, cursors_h) = image::read(data::CURSORS)?;
        let (checker_w, checker_h) = (2, 2);
        let (paste_w, paste_h) = (8, 8);

        let font =
            Texture::new(&mut ctx, [font_w, font_h], 0, self::SAMPLER).map_err(Error::Texture)?;
        let cursors = Texture::new(&mut ctx, [cursors_w, cursors_h], 0, self::SAMPLER)
            .map_err(Error::Texture)?;
        let paste =
            Texture::new(&mut ctx, [paste_w, paste_h], 0, self::SAMPLER).map_err(Error::Texture)?;
        let checker = Texture::new(&mut ctx, [checker_w, checker_h], 0, self::SAMPLER)
            .map_err(Error::Texture)?;

        font.upload_raw(GenMipmaps::No, &font_img)
            .map_err(Error::Texture)?;
        cursors
            .upload_raw(GenMipmaps::No, &cursors_img)
            .map_err(Error::Texture)?;
        checker
            .upload_raw(GenMipmaps::No, &draw::CHECKER)
            .map_err(Error::Texture)?;

        let sprite2d = self::program::<Sprite2dInterface>(
            include_str!("data/sprite.vert"),
            include_str!("data/sprite.frag"),
        );
        let shape2d = self::program::<Shape2dInterface>(
            include_str!("data/shape.vert"),
            include_str!("data/shape.frag"),
        );
        let cursor2d = self::program::<Cursor2dInterface>(
            include_str!("data/cursor.vert"),
            include_str!("data/cursor.frag"),
        );
        let screen2d = self::program::<Screen2dInterface>(
            include_str!("data/screen.vert"),
            include_str!("data/screen.frag"),
        );

        let physical = win_size.to_physical(scale_factor);
        let present_fb =
            Framebuffer::back_buffer(&mut ctx, [physical.width as u32, physical.height as u32]);
        let screen_fb = Framebuffer::new(
            &mut ctx,
            [win_size.width as u32, win_size.height as u32],
            0,
            self::SAMPLER,
        )
        .map_err(Error::Framebuffer)?;

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

        let draw_ctx = draw::Context {
            ui_batch: shape2d::Batch::new(),
            text_batch: self::text_batch(font.size()),
            overlay_batch: self::text_batch(font.size()),
            cursor_sprite: sprite::Sprite::new(cursors_w, cursors_h),
            tool_batch: sprite2d::Batch::new(cursors_w, cursors_h),
            paste_batch: sprite2d::Batch::new(paste_w, paste_h),
            checker_batch: sprite2d::Batch::new(checker_w, checker_h),
        };

        Ok(Renderer {
            ctx,
            draw_ctx,
            win_size,
            scale_factor,
            scale: 1.0,
            _present_mode,
            blending: Blending::Alpha,
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
            staging_batch: shape2d::Batch::new(),
            final_batch: shape2d::Batch::new(),
            view_data: BTreeMap::new(),
        })
    }

    fn init(&mut self, effects: Vec<Effect>) {
        self.handle_effects(effects).unwrap();
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

        self.handle_effects(effects).unwrap();
        self.update_view_animations(session);
        self.update_view_composites(session);

        let ortho: linear::M44 = unsafe {
            mem::transmute(kit::ortho(
                self.screen_fb.width(),
                self.screen_fb.height(),
                Origin::TopLeft,
            ))
        };
        let identity: Matrix4<f32> = Matrix4::identity();
        let identity: linear::M44 = unsafe { mem::transmute(identity) };

        let Self {
            draw_ctx,
            font,
            cursors,
            checker,
            sprite2d,
            shape2d,
            cursor2d,
            screen2d,
            scale_factor,
            present_fb,
            blending,
            screen_fb,
            render_st,
            pipeline_st,
            paste,
            paste_outputs,
            view_data,
            ..
        } = self;

        draw_ctx.clear();
        draw_ctx.draw(&session, avg_frametime, execution.clone());

        let text_tess =
            self::tessellation::<_, Sprite2dVertex>(&mut self.ctx, &draw_ctx.text_batch.vertices());
        let overlay_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            &draw_ctx.overlay_batch.vertices(),
        );
        let ui_tess =
            self::tessellation::<_, Shape2dVertex>(&mut self.ctx, &draw_ctx.ui_batch.vertices());
        let tool_tess =
            self::tessellation::<_, Sprite2dVertex>(&mut self.ctx, &draw_ctx.tool_batch.vertices());
        let cursor_tess = self::tessellation::<_, Cursor2dVertex>(
            &mut self.ctx,
            &draw_ctx.cursor_sprite.vertices(),
        );
        let checker_tess = self::tessellation::<_, Sprite2dVertex>(
            &mut self.ctx,
            &draw_ctx.checker_batch.vertices(),
        );
        let screen_tess = TessBuilder::new(&mut self.ctx)
            .set_vertex_nb(6)
            .set_mode(Mode::Triangle)
            .build()
            .unwrap();

        let paste_tess = if draw_ctx.paste_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Sprite2dVertex>(
                &mut self.ctx,
                &draw_ctx.paste_batch.vertices(),
            ))
        };
        let staging_tess = if self.staging_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Shape2dVertex>(
                &mut self.ctx,
                &self.staging_batch.vertices(),
            ))
        };
        let final_tess = if self.final_batch.is_empty() {
            None
        } else {
            Some(self::tessellation::<_, Shape2dVertex>(
                &mut self.ctx,
                &self.final_batch.vertices(),
            ))
        };

        let help_tess = if session.mode == session::Mode::Help {
            let mut win = shape2d::Batch::new();
            let mut text = self::text_batch(font.size());
            draw::draw_help(session, &mut text, &mut win);

            let win_tess =
                self::tessellation::<_, Shape2dVertex>(&mut self.ctx, win.vertices().as_slice());
            let text_tess =
                self::tessellation::<_, Sprite2dVertex>(&mut self.ctx, text.vertices().as_slice());
            Some((win_tess, text_tess))
        } else {
            None
        };

        let v = session.active_view();
        let l = v.active_layer_id;
        let v_data = view_data.get(&v.id).unwrap();
        let l_data = v_data.get_layer(l);
        let view_ortho: linear::M44 =
            unsafe { mem::transmute(kit::ortho(v.width(), v.height(), Origin::TopLeft)) };

        let mut builder = self.ctx.pipeline_builder();

        // Render to view staging buffer.
        builder.pipeline(
            &v_data.staging_fb,
            &pipeline_st,
            |pipeline, mut shd_gate| {
                // Render staged brush strokes.
                if let Some(tess) = staging_tess {
                    shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                        iface.ortho.update(view_ortho);
                        iface.transform.update(identity);

                        rdr_gate.render(render_st, |mut tess_gate| {
                            tess_gate.render(&tess);
                        });
                    });
                }
                // Render staging paste buffer.
                if let Some(tess) = paste_tess {
                    let bound_paste = pipeline.bind_texture(paste);
                    shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                        iface.ortho.update(view_ortho);
                        iface.transform.update(identity);
                        iface.tex.update(&bound_paste);

                        rdr_gate.render(render_st, |mut tess_gate| {
                            tess_gate.render(&tess);
                        });
                    });
                }
            },
        );

        // Render to view final buffer.
        builder.pipeline(
            &l_data.fb,
            &pipeline_st.clone().enable_clear_color(false),
            |pipeline, mut shd_gate| {
                let bound_paste = pipeline.bind_texture(paste);

                // Render final brush strokes.
                if let Some(tess) = final_tess {
                    shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                        iface.ortho.update(view_ortho);
                        iface.transform.update(identity);

                        let render_st = if blending == &Blending::Constant {
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
                        iface.ortho.update(view_ortho);
                        iface.transform.update(identity);
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
        let bg = Rgba::from(session.settings["background"].to_rgba8());
        let screen_st = &pipeline_st
            .clone()
            .set_clear_color([bg.r, bg.g, bg.b, bg.a]);
        builder.pipeline(screen_fb, &screen_st, |pipeline, mut shd_gate| {
            // Draw view checkers to screen framebuffer.
            if session.settings["checker"].is_set() {
                shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                    let bound_checker = pipeline.bind_texture(checker);

                    iface.ortho.update(ortho);
                    iface.transform.update(identity);
                    iface.tex.update(&bound_checker);
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&checker_tess);
                    });
                });
            }

            for (id, v) in view_data.iter() {
                if let Some(view) = session.views.get(*id) {
                    for (layer_id, _) in view.layers.iter().enumerate() {
                        let l = v.get_layer(layer_id);
                        let layer_offset = view.layer_offset(layer_id, view.zoom);
                        let bound_view = pipeline.bind_texture(l.fb.color_slot());
                        let transform = Matrix4::from_translation(
                            (session.offset + view.offset + layer_offset).extend(*draw::VIEW_LAYER),
                        ) * Matrix4::from_nonuniform_scale(
                            view.zoom, view.zoom, 1.0,
                        );

                        // Render views.
                        shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                            iface.tex.update(&bound_view);
                            iface.ortho.update(ortho);
                            iface.transform.update(unsafe { mem::transmute(transform) });

                            rdr_gate.render(render_st, |mut tess_gate| {
                                tess_gate.render(&l.tess);
                            });

                            if layer_id == view.active_layer_id {
                                // TODO: We only need to render this on the active view.
                                let bound_view_staging =
                                    pipeline.bind_texture(v.staging_fb.color_slot());

                                iface.tex.update(&bound_view_staging);
                                rdr_gate.render(render_st, |mut tess_gate| {
                                    tess_gate.render(&l.tess);
                                });
                            }
                        });
                    }
                }
            }

            // Render UI.
            shd_gate.shade(&shape2d, |iface, mut rdr_gate| {
                iface.ortho.update(ortho);
                iface.transform.update(identity);

                rdr_gate.render(render_st, |mut tess_gate| {
                    tess_gate.render(&ui_tess);
                });
            });

            // Render text, tool & view animations.
            shd_gate.shade(&sprite2d, |iface, mut rdr_gate| {
                iface.ortho.update(ortho);
                iface.transform.update(identity);

                // Render view composites.
                for (id, v) in view_data.iter() {
                    match (&v.layer_tess, session.views.get(*id)) {
                        (Some(tess), Some(view)) if view.layers.len() > 1 => {
                            for (layer_id, _) in view.layers.iter().enumerate() {
                                let l = v.get_layer(layer_id);
                                let bound_view = pipeline.bind_texture(l.fb.color_slot());

                                iface.tex.update(&bound_view);

                                rdr_gate.render(render_st, |mut tess_gate| {
                                    tess_gate.render(tess);
                                });
                            }
                        }
                        _ => (),
                    }
                }

                // Render view animations.
                if session.settings["animation"].is_set() {
                    for (id, v) in view_data.iter() {
                        match (&v.anim_tess, session.views.get(*id)) {
                            (Some(tess), Some(view)) if view.animation.len() > 1 => {
                                let composite_t = unsafe {
                                    mem::transmute(Matrix4::from_translation(
                                        Vector2::new(0., -(v.h as f32 * view.zoom)).extend(0.),
                                    ))
                                };

                                for (i, _) in view.layers.iter().enumerate() {
                                    let l = v.get_layer(i);
                                    let bound_layer = pipeline.bind_texture(l.fb.color_slot());
                                    let layer_offset = v.h as usize * i;
                                    let t = Matrix4::from_translation(
                                        Vector2::new(0., layer_offset as f32 * view.zoom)
                                            .extend(0.),
                                    );

                                    // Render layer animation.
                                    iface.tex.update(&bound_layer);
                                    iface.transform.update(unsafe { mem::transmute(t) });
                                    rdr_gate.render(render_st, |mut tess_gate| {
                                        tess_gate.render(tess);
                                    });

                                    // Render composite animation.
                                    if view.layers.len() > 1 {
                                        iface.transform.update(composite_t);
                                        rdr_gate.render(render_st, |mut tess_gate| {
                                            tess_gate.render(tess);
                                        });
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }

                {
                    let bound_font = pipeline.bind_texture(font);
                    iface.tex.update(&bound_font);
                    iface.transform.update(identity);

                    // Render text.
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&text_tess);
                    });
                }
                {
                    let bound_tool = pipeline.bind_texture(cursors);
                    iface.tex.update(&bound_tool);

                    // Render tool.
                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&tool_tess);
                    });
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
                    iface.ortho.update(ortho);
                    iface.transform.update(identity);

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
                    iface.ortho.update(unsafe {
                        mem::transmute::<_, linear::M44>(kit::ortho(
                            screen_fb.width(),
                            screen_fb.height(),
                            Origin::BottomLeft,
                        ))
                    });

                    rdr_gate.render(render_st, |mut tess_gate| {
                        tess_gate.render(&overlay_tess);
                    });
                });
            }

            // Render cursor.
            let bound_cursors = pipeline.bind_texture(cursors);
            shd_gate.shade(&cursor2d, |iface, mut rdr_gate| {
                let ui_scale = session.settings["scale"].to_f64();
                let pixel_ratio = platform::pixel_ratio(*scale_factor);

                iface.cursor.update(&bound_cursors);
                iface.framebuffer.update(&bound_screen);
                iface.ortho.update(ortho);
                iface.scale.update((ui_scale * pixel_ratio) as f32);

                rdr_gate.render(render_st, |mut tess_gate| {
                    tess_gate.render(&cursor_tess);
                });
            });
        });

        // If active view is dirty, record a snapshot of it.
        if v.is_dirty() {
            if let Some(vr) = self.resources.lock_mut().get_view_mut(v.id) {
                match v.state {
                    ViewState::Dirty(_) if v.is_resized() => {
                        vr.record_view_resized(v_data.layer_pixels().collect(), v.extent());
                    }
                    ViewState::Dirty(_) => {
                        vr.record_view_painted(v_data.layer_pixels().collect());
                    }
                    ViewState::LayerDirty(layer_id) => {
                        let pixels = v_data.get_layer(layer_id).pixels();
                        vr.record_layer_painted(layer_id, pixels, v.extent());
                    }
                    ViewState::Okay | ViewState::Damaged(_) | ViewState::LayerDamaged(_) => {}
                }
            }
        }

        if !execution.borrow().is_normal() {
            let texels = screen_fb.color_slot().get_raw_texels();
            let texels = Rgba8::align(&texels);

            execution
                .borrow_mut()
                .record(&texels.iter().cloned().map(Bgra8::from).collect::<Vec<_>>());
        }
    }

    fn handle_present_mode_changed(&mut self, _present_mode: PresentMode) {}

    fn handle_scale_factor_changed(&mut self, scale_factor: f64) {
        self.scale_factor = scale_factor;
        self.handle_resized(self.win_size);
    }
}

impl Renderer {
    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        let physical = size.to_physical(self.scale_factor);

        self.present_fb = Framebuffer::back_buffer(
            &mut self.ctx,
            [physical.width as u32, physical.height as u32],
        );
        self.win_size = size;
        self.handle_session_scale_changed(self.scale);
    }

    pub fn handle_session_scale_changed(&mut self, scale: f64) {
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

    fn handle_effects(&mut self, mut effects: Vec<Effect>) -> Result<(), RendererError> {
        for eff in effects.drain(..) {
            match eff {
                Effect::SessionResized(size) => {
                    self.handle_resized(size);
                }
                Effect::SessionScaled(scale) => {
                    self.handle_session_scale_changed(scale);
                }
                Effect::ViewActivated(_) => {}
                Effect::ViewAdded(id) => {
                    let resources = self.resources.lock();

                    if let Some((s, pixels)) = resources.get_snapshot_safe(id, LayerId::default()) {
                        let (w, h) = (s.width(), s.height());

                        self.view_data.insert(
                            id,
                            ViewData::new(w, h, Some(&pixels.clone().into_rgba8()), &mut self.ctx),
                        );
                    }
                }
                Effect::ViewRemoved(id) => {
                    self.view_data.remove(&id);
                }
                Effect::ViewOps(id, ops) => {
                    self.handle_view_ops(id, &ops)?;
                }
                Effect::ViewDamaged(id, Some(extent)) => {
                    self.handle_view_resized(id, extent.width(), extent.height())?;
                }
                Effect::ViewDamaged(id, None) => {
                    self.handle_view_damaged(id)?;
                }
                Effect::ViewLayerDamaged(id, layer) => {
                    self.handle_view_layer_damaged(id, layer)?;
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
                Effect::ViewTouched(_) => {}
            }
        }
        Ok(())
    }

    fn handle_view_ops(&mut self, id: ViewId, ops: &[ViewOp]) -> Result<(), RendererError> {
        use RendererError as Error;

        for op in ops {
            match op {
                ViewOp::Resize(w, h) => {
                    self.resize_view(id, *w, *h)?;
                }
                ViewOp::AddLayer(layer_id, range) => {
                    let resources = self.resources.lock();

                    if let Some((_, pixels)) = resources.get_snapshot_safe(id, *layer_id) {
                        if let Some(pixels) = pixels.as_rgba8() {
                            self.view_data
                                .get_mut(&id)
                                .expect("views must have associated view data")
                                .add_layer(range, Some(pixels), &mut self.ctx);
                        } else {
                            unimplemented!()
                        }
                    }
                }
                ViewOp::RemoveLayer(_layer_id) => {}
                ViewOp::Clear(color) => {
                    let view = self
                        .view_data
                        .get(&id)
                        .expect("views must have associated view data");

                    for l in view.layers.iter() {
                        l.fb.color_slot()
                            .clear(GenMipmaps::No, (color.r, color.g, color.b, color.a))
                            .map_err(Error::Texture)?;
                    }
                }
                ViewOp::Blit(src, dst) => {
                    let view = &self
                        .view_data
                        .get(&id)
                        .expect("views must have associated view data");

                    for (l_id, l) in view.layers.iter().enumerate() {
                        let (_, texels) = self
                            .resources
                            .lock()
                            .get_snapshot_rect(id, l_id, &src.map(|n| n as i32))
                            .unwrap(); // TODO: Handle this nicely?
                        let texels = util::align_u8(&texels);

                        l.fb.color_slot()
                            .upload_part_raw(
                                GenMipmaps::No,
                                [dst.x1 as u32, dst.y1 as u32],
                                [src.width() as u32, src.height() as u32],
                                &texels,
                            )
                            .map_err(Error::Texture)?;
                    }
                }
                ViewOp::Yank(layer_id, src) => {
                    let resources = self.resources.lock();
                    let (_, pixels) = resources.get_snapshot_rect(id, *layer_id, src).unwrap(); // TODO: Handle this nicely?
                    let (w, h) = (src.width() as u32, src.height() as u32);
                    let [paste_w, paste_h] = self.paste.size();

                    if paste_w != w || paste_h != h {
                        self.paste =
                            Texture::new(&mut self.ctx, [w as u32, h as u32], 0, self::SAMPLER)
                                .map_err(Error::Texture)?;
                    }
                    let body = util::align_u8(&pixels);

                    self.paste
                        .upload_raw(GenMipmaps::No, body)
                        .map_err(Error::Texture)?;
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
                ViewOp::SetPixel(layer_id, rgba, x, y) => {
                    let fb = &self
                        .view_data
                        .get(&id)
                        .expect("views must have associated view data")
                        .get_layer(*layer_id)
                        .fb;
                    let texels = &[*rgba];
                    let texels = util::align_u8(texels);
                    fb.color_slot()
                        .upload_part_raw(GenMipmaps::No, [*x as u32, *y as u32], [1, 1], texels)
                        .map_err(Error::Texture)?;
                }
            }
        }
        Ok(())
    }

    fn handle_view_layer_damaged(
        &mut self,
        id: ViewId,
        layer_id: LayerId,
    ) -> Result<(), RendererError> {
        let layer = &self
            .view_data
            .get(&id)
            .expect("views must have associated view data")
            .get_layer(layer_id);

        let pixels = {
            let rs = self.resources.lock();
            let (_, pixels) = rs.get_snapshot(id, layer_id);
            pixels.to_owned()
        };

        layer.clear()?;
        layer.upload(pixels.as_bytes())?;

        Ok(())
    }

    fn handle_view_damaged(&mut self, id: ViewId) -> Result<(), RendererError> {
        let n = self
            .view_data
            .get(&id)
            .expect("views must have associated view data")
            .layers
            .len();

        for layer_id in 0..n {
            self.handle_view_layer_damaged(id, layer_id)?;
        }

        Ok(())
    }

    fn handle_view_resized(&mut self, id: ViewId, vw: u32, vh: u32) -> Result<(), RendererError> {
        self.resize_view(id, vw, vh)
    }

    fn resize_view(&mut self, id: ViewId, vw: u32, vh: u32) -> Result<(), RendererError> {
        // View size changed. Re-create view resources.
        let (ew, eh) = {
            let resources = self.resources.lock();
            let extent = resources.get_view(id).expect("view must exist").extent;
            (extent.width(), extent.height())
        };

        // Ensure not to transfer more data than can fit in the view buffer.
        let tw = u32::min(ew, vw);
        let th = u32::min(eh, vh);

        let mut view_data = ViewData::new(vw, vh, None, &mut self.ctx);

        let resources = self.resources.lock();
        let view_resource = resources.get_view(id).unwrap();

        // Create n-1 layers, since layer #0 is created when a `ViewData` is created.
        for _ in view_resource.layers().skip(1) {
            view_data.add_layer(&FrameRange::default(), None, &mut self.ctx);
        }

        let trect = Rect::origin(tw as i32, th as i32);
        for (layer_id, layer) in view_resource.layers() {
            // The following sequence of commands will try to copy a rect that isn't contained
            // in the snapshot, hence we must skip the uploading in that case:
            //
            //     :f/add
            //     :f/remove
            //     :l/add
            //     :undo
            //
            if let Some((_, texels)) = layer.get_snapshot_rect(&trect) {
                let texels = util::align_u8(&texels);
                let l = view_data.get_layer(*layer_id);

                l.upload_part([0, vh - th], [tw, th], texels)?;
            }
        }

        self.view_data.insert(id, view_data);

        Ok(())
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
            let batch = draw::draw_view_animation(s, &v);

            if let Some(vd) = self.view_data.get_mut(&v.id) {
                vd.anim_tess = Some(self::tessellation::<_, Sprite2dVertex>(
                    &mut self.ctx,
                    batch.vertices().as_slice(),
                ));
            }
        }
    }

    fn update_view_composites(&mut self, s: &Session) {
        for v in s.views.iter() {
            let batch = draw::draw_view_composites(s, &v);

            if let Some(vd) = self.view_data.get_mut(&v.id) {
                vd.layer_tess = Some(self::tessellation::<_, Sprite2dVertex>(
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

fn text_batch([w, h]: [u32; 2]) -> TextBatch {
    TextBatch::new(w, h, draw::GLYPH_WIDTH, draw::GLYPH_HEIGHT)
}
