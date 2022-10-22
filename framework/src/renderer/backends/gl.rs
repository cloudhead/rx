//! A 2D pixel-perfect renderer.
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;

use luminance::backend::shader::Uniformable;
use luminance::blending::{Blending, Equation, Factor};
use luminance::context::GraphicsContext;
use luminance::depth_stencil::Comparison;
use luminance::framebuffer::Framebuffer;
use luminance::pipeline::Pipeline;
use luminance::pipeline::{PipelineError, PipelineState, TextureBinding};
use luminance::pixel;
use luminance::pixel::{ColorPixel, Pixel, PixelFormat, RenderablePixel};
use luminance::render_state::RenderState;
use luminance::shader::UniformType;
use luminance::shader::{Program, Uniform};
use luminance::shading_gate::ShadingGate;
use luminance::tess::{Mode, Tess, TessBuilder};
use luminance::texture::{Dim2, MagFilter, MinFilter, Sampler, TexelUpload, Texture, Wrap};
use luminance_derive::{Semantics, UniformInterface, Vertex};
use luminance_gl::gl33;

use crate::gfx::prelude::*;
use crate::platform::{self, LogicalSize};
use crate::renderer;
use crate::renderer::{Effect, Paint, TextureId, TextureStore};

/// GL backend.
type Gl = gl33::GL33;

/// Texture sampler.
const SAMPLER: Sampler = Sampler {
    wrap_r: Wrap::Repeat,
    wrap_s: Wrap::Repeat,
    wrap_t: Wrap::Repeat,
    min_filter: MinFilter::Nearest,
    mag_filter: MagFilter::Nearest,
    depth_comparison: None,
};

#[derive(Copy, Clone, Debug, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "position", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "uv", repr = "[f32; 2]", wrapper = "VertexUv")]
    Uv,
    #[sem(name = "color", repr = "[u8; 4]", wrapper = "VertexColor")]
    Color,
    #[sem(name = "opacity", repr = "[f32; 1]", wrapper = "VertexOpacity")]
    Opacity,
    #[sem(name = "angle", repr = "[f32; 1]", wrapper = "VertexAngle")]
    Angle,
    #[sem(name = "center", repr = "[f32; 2]", wrapper = "VertexCenter")]
    Center,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Sprite2dInterface {
    tex: Uniform<TextureBinding<Dim2, pixel::NormUnsigned>>,
    ortho: Uniform<Transform3D>,
    transform: Uniform<Transform3D>,
}

#[repr(C)]
#[derive(Copy, Clone, Vertex, Debug)]
#[vertex(sem = "VertexSemantics")]
struct Sprite2dVertex {
    position: VertexPosition,
    uv: VertexUv,
    #[vertex(normalized = "true")]
    color: VertexColor,
    opacity: VertexOpacity,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Shape2dInterface {
    ortho: Uniform<Transform3D>,
    transform: Uniform<Transform3D>,
}

#[repr(C)]
#[derive(Copy, Clone, Vertex, Debug)]
#[vertex(sem = "VertexSemantics")]
struct Shape2dVertex {
    position: VertexPosition,
    angle: VertexAngle,
    center: VertexCenter,
    #[vertex(normalized = "true")]
    color: VertexColor,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Cursor2dInterface {
    cursor: Uniform<TextureBinding<Dim2, pixel::NormUnsigned>>,
    framebuffer: Uniform<TextureBinding<Dim2, pixel::NormUnsigned>>,
    invert: Uniform<bool>,
    ortho: Uniform<Transform3D>,
    transform: Uniform<Transform3D>,
}

#[repr(C)]
#[derive(Copy, Clone, Vertex)]
#[vertex(sem = "VertexSemantics")]
struct Cursor2dVertex {
    position: VertexPosition,
    uv: VertexUv,
}

////////////////////////////////////////////////////////////

#[derive(UniformInterface)]
struct Screen2dInterface {
    framebuffer: Uniform<TextureBinding<Dim2, pixel::NormUnsigned>>,
}

////////////////////////////////////////////////////////////

unsafe impl Pixel for Rgba8 {
    type Encoding = Rgba8;
    type RawEncoding = u8;
    type SamplerType = pixel::NormUnsigned;

    fn pixel_format() -> PixelFormat {
        pixel::SRGBA8UI::pixel_format()
    }
}

unsafe impl RenderablePixel for Rgba8 {}
unsafe impl ColorPixel for Rgba8 {}

unsafe impl<'a> Uniformable<'a, Transform3D> for Gl {
    type Target = Transform3D;

    const SIZE: usize = 1;

    unsafe fn ty() -> UniformType {
        UniformType::M44
    }

    unsafe fn update(
        _program: &mut Self::ProgramRepr,
        uniform: &'a Uniform<Transform3D>,
        transform: Self::Target,
    ) {
        let mat4: [[f32; 4]; 4] = transform.into();

        gl::UniformMatrix4fv(uniform.index(), 1, gl::FALSE, mat4.as_ptr() as _);
    }
}

impl From<renderer::Blending> for Blending {
    fn from(other: renderer::Blending) -> Self {
        match other {
            renderer::Blending::Alpha => Blending {
                equation: Equation::Additive,
                src: Factor::SrcAlpha,
                dst: Factor::SrcAlphaComplement,
            },
            renderer::Blending::Constant => Blending {
                equation: Equation::Additive,
                src: Factor::One,
                dst: Factor::Zero,
            },
        }
    }
}

////////////////////////////////////////////////////////////

/// Map containing all render targets, keyed by texture id.
type RenderTargets = HashMap<TextureId, RenderTarget>;

/// Map containing all textures.
#[derive(Default)]
struct Textures {
    storage: HashMap<TextureId, Texture<Gl, Dim2, Rgba8>>,
}

impl Textures {
    /// Put a texture into the storage.
    fn put(
        &mut self,
        id: TextureId,
        image: Image,
        backend: &mut Backend,
    ) -> Result<&Texture<Gl, Dim2, Rgba8>, Error> {
        let texture: Texture<Gl, Dim2, Rgba8> = Texture::new(
            backend,
            image.size.into(),
            SAMPLER,
            TexelUpload::BaseLevel {
                texels: &image.pixels,
                mipmaps: 0,
            },
        )?;

        Ok(self.storage.entry(id).or_insert(texture))
    }
}

/// Texture used as off-screen render target.
struct RenderTarget {
    /// Size of framebuffer texture.
    pub size: Size<u32>,
    /// The target framebuffer onto which to render.
    fb: Framebuffer<Gl, Dim2, Rgba8, pixel::Depth32F>,
}

impl RenderTarget {
    /// Create a new render target.
    fn new(size: Size<u32>, texels: &[Rgba8], backend: &mut Backend) -> Result<Self, Error> {
        let mut fb: Framebuffer<Gl, Dim2, Rgba8, pixel::Depth32F> =
            Framebuffer::new(backend, size.into(), 0, self::SAMPLER)?;

        fb.color_slot()
            .upload(TexelUpload::BaseLevel { texels, mipmaps: 0 })?;

        Ok(Self { size, fb })
    }

    fn upload_part(
        &mut self,
        offset: impl Into<Vector2D<u32>>,
        size: impl Into<Size<u32>>,
        texels: &[Rgba8],
    ) -> Result<(), Error> {
        let offset = offset.into();
        let size = size.into();

        self.fb
            .color_slot()
            .upload_part(
                offset.into(),
                size.into(),
                TexelUpload::BaseLevel { texels, mipmaps: 0 },
            )
            .map_err(Error::from)
    }

    fn upload(&mut self, texels: &[Rgba8]) -> Result<(), Error> {
        self.fb
            .color_slot()
            .upload(TexelUpload::BaseLevel { texels, mipmaps: 0 })
            .map_err(Error::from)
    }

    fn pixels(&mut self) -> Result<Vec<Rgba8>, Error> {
        let texels = self.fb.color_slot().get_raw_texels()?;

        Ok(Rgba8::align(&texels).to_vec())
    }

    fn resized(mut self, size: Size<u32>, backend: &mut Backend) -> Result<Self, Error> {
        let texels = self.pixels()?;
        let texels = texels.as_slice();
        let blank = Image::blank(size);

        // Transfer size.
        // Ensure not to transfer more data than can fit in the view buffer, since we
        // may be creating a smaller texture.
        let tw = u32::min(size.w, self.size.w);
        let th = u32::min(size.h, self.size.h);

        let mut texture = self.fb.into_color_slot();
        texture.resize(size.into(), TexelUpload::Reserve { mipmaps: 0 })?;

        let mut resized = Self {
            fb: Framebuffer::new(backend, size.into(), 0, self::SAMPLER)?,
            size,
        };
        resized.upload(&blank.pixels)?;
        resized.upload_part([0, size.h - th], [tw, th], texels)?;

        Ok(resized)
    }
}

/// OpenGL 2D renderer.
pub struct Renderer {
    /// Window size in logical pixels.
    pub win_size: LogicalSize,
    /// Window/device scale.
    pub win_scale: f64,
    /// UI scale (user-defined).
    pub ui_scale: f32,

    /// Presentation framebuffer (back buffer), this is what is rendered to screen.
    present_fb: Framebuffer<Gl, Dim2, (), ()>,
    /// Screen framebuffer, this is the virtual screen on which almost everything is
    /// drawn before it is written to the back buffer.
    screen_fb: Framebuffer<Gl, Dim2, Rgba8, pixel::Depth32F>,
    /// Rendering pipeline state.
    pipeline_st: PipelineState,
    /// OpenGL backend.
    backend: Backend,
    /// Render context used when drawing.
    context: RenderContext,
}

/// Render context.
struct RenderContext {
    textures: Textures,
    targets: RenderTargets,
    render_st: RenderState,

    sprite2d: Program<Gl, VertexSemantics, (), Sprite2dInterface>,
    shape2d: Program<Gl, VertexSemantics, (), Shape2dInterface>,
    screen2d: Program<Gl, VertexSemantics, (), Screen2dInterface>,
    cursor2d: Program<Gl, VertexSemantics, (), Cursor2dInterface>,
}

impl RenderContext {
    fn render(
        &mut self,
        op: RenderOp,
        identity: Transform3D,
        ortho: Transform3D,
        pipeline: &Pipeline<'_, Gl>,
        shd_gate: &mut ShadingGate<'_, Gl>,
    ) -> Result<(), Error> {
        match op {
            RenderOp::Shape {
                tess,
                transform,
                blending,
            } => {
                shd_gate.shade(&mut self.shape2d, |mut iface, uni, mut rdr_gate| {
                    iface.set(&uni.ortho, ortho);
                    iface.set(&uni.transform, identity * transform);

                    rdr_gate.render(
                        &self.render_st.clone().set_blending(blending),
                        |mut tess_gate| tess_gate.render(&tess),
                    )?;

                    Ok::<_, PipelineError>(())
                })?;
            }
            RenderOp::Sprite {
                tess,
                transform,
                texture,
                blending,
            } => {
                let texture = if let Some(texture) = self.textures.storage.get_mut(&texture) {
                    texture
                } else if let Some(target) = self.targets.get_mut(&texture) {
                    target.fb.color_slot()
                } else {
                    return Err(Error::TextureNotFound(texture));
                };

                shd_gate.shade(&mut self.sprite2d, |mut iface, uni, mut rdr_gate| {
                    let bound = pipeline.bind_texture(texture)?;

                    iface.set(&uni.ortho, ortho);
                    iface.set(&uni.transform, identity * transform);
                    iface.set(&uni.tex, bound.binding());

                    rdr_gate.render(
                        &self.render_st.clone().set_blending(blending),
                        |mut tess_gate| tess_gate.render(&tess),
                    )?;

                    Ok::<_, PipelineError>(())
                })?;
            }
        }
        Ok(())
    }
}

/// Graphics backend.
struct Backend {
    gl: Gl,
}

unsafe impl GraphicsContext for Backend {
    type Backend = self::Gl;

    fn backend(&mut self) -> &mut Self::Backend {
        &mut self.gl
    }
}

impl Backend {
    /// Create a shader program.
    fn program<T>(
        &mut self,
        vert: &str,
        frag: &str,
    ) -> Result<Program<Gl, VertexSemantics, (), T>, Error>
    where
        T: luminance::shader::UniformInterface<Gl>,
    {
        Ok(self
            .new_shader_program()
            .from_strings(vert, None, None, frag)?
            .ignore_warnings())
    }

    /// Create a tessellation.
    fn tessellation<T, S>(&mut self, verts: &[T]) -> Result<Tess<Gl, S>, Error>
    where
        S: luminance::vertex::Vertex + Sized,
    {
        let (head, body, tail) = unsafe { verts.align_to::<S>() };

        assert!(head.is_empty());
        assert!(tail.is_empty());

        TessBuilder::new(self)
            .set_vertices(body)
            .set_mode(Mode::Triangle)
            .build()
            .map_err(Error::from)
    }
}

/// Renderer error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("initialization error")]
    Initialization,
    #[error("texture error: {0}")]
    Texture(#[from] luminance::texture::TextureError),
    #[error("framebuffer error: {0}")]
    Framebuffer(#[from] luminance::framebuffer::FramebufferError),
    #[error("pipeline error: {0}")]
    Pipeline(#[from] PipelineError),
    #[error("state query error: {0}")]
    State(#[from] luminance_gl::gl33::StateQueryError),
    #[error("tesselation error: {0}")]
    Tess(#[from] luminance::tess::TessError),
    #[error("{0} not found")]
    TextureNotFound(TextureId),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
    #[error("program error: {0}")]
    Program(#[from] luminance::shader::ProgramError),
    #[error("error: {0}")]
    Custom(&'static str),
}

#[derive(Debug)]
enum RenderOp {
    /// Draw a shape.
    Shape {
        tess: Tess<Gl, Shape2dVertex>,
        transform: Transform3D,
        blending: Blending,
    },
    /// Draw a sprite.
    Sprite {
        tess: Tess<Gl, Sprite2dVertex>,
        transform: Transform3D,
        texture: TextureId,
        blending: Blending,
    },
}

/// Render frame.
#[derive(Default)]
struct Frame {
    /// On-screen operations.
    onscreen: Vec<RenderOp>,
    /// Off-screen operations.
    offscreen: HashMap<TextureId, Vec<RenderOp>>,
    /// Textures to clear.
    clear: HashMap<TextureId, Rgba>,
    /// Textures to upload.
    upload: HashMap<TextureId, Arc<[Rgba8]>>,
    /// Textures that were modified.
    dirty: HashSet<TextureId>,
}

impl Frame {
    pub fn new(
        effects: impl Iterator<Item = Effect>,
        context: &mut RenderContext,
        backend: &mut Backend,
    ) -> Result<Self, Error> {
        let mut frame = Frame::default();

        for eff in effects {
            match eff {
                Effect::Paint { paint, blending } => {
                    frame.paint(paint, blending.into(), backend)?;
                }
                Effect::Clear { id, color } => {
                    frame.clear(id, color);
                }
                Effect::Texture {
                    id,
                    image,
                    offscreen,
                } => {
                    if offscreen {
                        frame.framebuffer(id, image, context, backend)?;
                    } else {
                        frame.texture(id, image, context, backend)?;
                    }
                }
                Effect::Resize { id, size } => {
                    frame.resize(id, size, context, backend)?;
                }
                Effect::Upload { id, texels } => {
                    frame.upload.insert(id, texels);
                    frame.offscreen.entry(id).or_default();
                }
            }
        }
        Ok(frame)
    }

    /// Load a texture into graphics memory.
    fn texture(
        &mut self,
        id: TextureId,
        image: Image,
        context: &mut RenderContext,
        backend: &mut Backend,
    ) -> Result<(), Error> {
        context.textures.put(id, image, backend)?;

        Ok(())
    }

    /// Create a render target.
    fn framebuffer(
        &mut self,
        id: TextureId,
        image: Image,
        context: &mut RenderContext,
        backend: &mut Backend,
    ) -> Result<(), Error> {
        if let Entry::Vacant(e) = context.targets.entry(id) {
            let rt = RenderTarget::new(image.size, image.pixels.as_ref(), backend)?;
            e.insert(rt);
        }
        self.offscreen.entry(id).or_default();

        Ok(())
    }

    fn clear(&mut self, texture: TextureId, color: Rgba8) {
        self.clear.insert(texture, Rgba::from(color));
        self.offscreen.entry(texture).or_default();
    }

    fn resize(
        &mut self,
        texture: TextureId,
        size: Size<u32>,
        context: &mut RenderContext,
        backend: &mut Backend,
    ) -> Result<(), Error> {
        if let Some(rt) = context.targets.remove(&texture) {
            let rt = rt.resized(size, backend)?;
            context.targets.insert(texture, rt);
        }
        Ok(())
    }

    fn paint(
        &mut self,
        paint: Paint,
        blending: Blending,
        backend: &mut Backend,
    ) -> Result<(), Error> {
        match paint {
            Paint::Shape {
                transform,
                vertices,
                target,
            } if !vertices.is_empty() => {
                let tess = backend.tessellation::<_, Shape2dVertex>(&vertices)?;
                let ops = if let Some(target) = target {
                    self.dirty.insert(target);
                    self.offscreen.entry(target).or_default()
                } else {
                    &mut self.onscreen
                };

                ops.push(RenderOp::Shape {
                    tess,
                    transform: transform.into(),
                    blending,
                });
            }

            Paint::Sprite {
                transform,
                vertices,
                texture,
                target,
            } if !vertices.is_empty() => {
                let tess = backend.tessellation::<_, Sprite2dVertex>(&vertices)?;
                let ops = if let Some(target) = target {
                    self.dirty.insert(target);
                    self.offscreen.entry(target).or_default()
                } else {
                    &mut self.onscreen
                };

                ops.push(RenderOp::Sprite {
                    tess,
                    transform: transform.into(),
                    texture,
                    blending,
                });
            }

            _ => {}
        }

        Ok(())
    }
}

impl renderer::Renderer for Renderer {
    type Error = Error;

    fn new(
        win: &mut platform::backend::Window,
        win_size: LogicalSize,
        win_scale: f64,
        ui_scale: f32,
    ) -> Result<Self, Error> {
        gl::load_with(|s| win.get_proc_address(s) as *const _);

        let gl = Gl::new()?;
        let mut backend = Backend { gl };

        let sprite2d = backend.program::<Sprite2dInterface>(
            include_str!("gl/data/sprite.vert"),
            include_str!("gl/data/sprite.frag"),
        )?;
        let shape2d = backend.program::<Shape2dInterface>(
            include_str!("gl/data/shape.vert"),
            include_str!("gl/data/shape.frag"),
        )?;
        let cursor2d = backend.program::<Cursor2dInterface>(
            include_str!("gl/data/cursor.vert"),
            include_str!("gl/data/cursor.frag"),
        )?;
        let screen2d = backend.program::<Screen2dInterface>(
            include_str!("gl/data/screen.vert"),
            include_str!("gl/data/screen.frag"),
        )?;

        let physical = win_size.to_physical(win_scale);
        let present_fb = Framebuffer::back_buffer(
            &mut backend,
            [physical.width as u32, physical.height as u32],
        )?;
        let screen_fb = Framebuffer::new(
            &mut backend,
            [win_size.width as u32, win_size.height as u32],
            0,
            self::SAMPLER,
        )?;

        let render_st = RenderState::default()
            .set_blending(Some(renderer::Blending::default().into()))
            .set_depth_test(Some(Comparison::LessOrEqual));
        let pipeline_st = PipelineState::default()
            .set_clear_color([0., 0., 0., 0.])
            .set_clear_depth(1.)
            .enable_srgb(true);

        let context = RenderContext {
            render_st,
            sprite2d,
            shape2d,
            cursor2d,
            screen2d,
            textures: Textures::default(),
            targets: HashMap::new(),
        };

        Ok(Renderer {
            context,
            pipeline_st,
            present_fb,
            screen_fb,
            backend,
            win_size,
            win_scale,
            ui_scale,
        })
    }

    fn frame<E, T>(
        &mut self,
        effects: E,
        cursor: cursor2d::Sprite,
        store: &mut T,
    ) -> Result<(), Error>
    where
        E: Iterator<Item = Effect>,
        T: TextureStore,
    {
        let frame = Frame::new(effects, &mut self.context, &mut self.backend)?;

        let cursor_tess = self
            .backend
            .tessellation::<_, Cursor2dVertex>(&cursor.vertices())?;
        let screen_tess = TessBuilder::<Gl, ()>::new(&mut self.backend)
            .set_render_vertex_nb(6)
            .set_mode(Mode::Triangle)
            .build()?;
        let mut builder = self.backend.new_pipeline_gate();

        ////////////////////////////////////////////////////////////////////////
        // Render to offscreen textures.
        ////////////////////////////////////////////////////////////////////////

        for (id, ops) in frame.offscreen {
            // Temporarily remove the target from the map to be able to use it
            // as a render target, while also mutably using the other targets
            // for sampling. We return the target to the map at the end of the
            // loop.
            let mut target = if let Some(t) = self.context.targets.remove(&id) {
                t
            } else {
                continue;
            };

            let ortho = Transform3D::<f32, (), ()>::ortho(target.fb.size(), Origin::BottomLeft);
            let clear = frame.clear.get(&id);

            if let Some(texels) = frame.upload.get(&id) {
                target.upload(&*texels)?;

                if clear.is_some() {
                    warn!("ignoring `clear` operation due to `upload` on {}", id);
                }
                if !ops.is_empty() {
                    warn!("ignoring paint operations due to `upload` on {}", id);
                }
            } else {
                let offscreen_st = self
                    .pipeline_st
                    .clone()
                    .set_clear_color(clear.map(|c| (*c).into()));

                builder.pipeline(&target.fb, &offscreen_st, |pipeline, mut shd_gate| {
                    for op in ops {
                        self.context.render(
                            op,
                            Transform3D::identity(),
                            ortho,
                            &pipeline,
                            &mut shd_gate,
                        )?;
                    }
                    Ok::<_, Error>(())
                });
            }
            // Return the target to the map.
            self.context.targets.insert(id, target);
        }

        ////////////////////////////////////////////////////////////////////////
        // Render to screen framebuffer.
        ////////////////////////////////////////////////////////////////////////

        let ortho = Transform3D::<f32, (), ()>::ortho(self.screen_fb.size(), Origin::TopLeft);
        let identity = Transform3D::from_nonuniform_scale(
            self.win_scale as f32 * self.ui_scale,
            self.win_scale as f32 * self.ui_scale,
            0.,
        );

        builder.pipeline(
            &self.screen_fb,
            &self.pipeline_st,
            |pipeline, mut shd_gate| {
                for op in frame.onscreen {
                    self.context
                        .render(op, identity, ortho, &pipeline, &mut shd_gate)?;
                }
                Ok::<_, Error>(())
            },
        );

        ////////////////////////////////////////////////////////////////////////
        // Render to back buffer.                                             //
        ////////////////////////////////////////////////////////////////////////

        let cursors = self
            .context
            .textures
            .storage
            .get_mut(&TextureId::default_cursors())
            .ok_or(Error::Custom("missing cursors texture"))?;

        builder.pipeline::<PipelineError, _, _, _, _>(
            &self.present_fb,
            &self.pipeline_st,
            |pipeline, mut shd_gate| {
                // Render screen framebuffer.
                let bound_screen = pipeline.bind_texture(self.screen_fb.color_slot())?;
                shd_gate.shade(
                    &mut self.context.screen2d,
                    |mut iface, uni, mut rdr_gate| {
                        iface.set(&uni.framebuffer, bound_screen.binding());
                        rdr_gate.render(&self.context.render_st, |mut tess_gate| {
                            tess_gate.render(&screen_tess)
                        })
                    },
                )?;

                // Render cursor.
                let bound_cursors = pipeline.bind_texture(cursors)?;
                shd_gate.shade(
                    &mut self.context.cursor2d,
                    |mut iface, uni, mut rdr_gate| {
                        iface.set(&uni.cursor, bound_cursors.binding());
                        iface.set(&uni.framebuffer, bound_screen.binding());
                        iface.set(&uni.ortho, ortho);
                        iface.set(&uni.transform, identity);
                        iface.set(&uni.invert, false);

                        rdr_gate.render(&self.context.render_st, |mut tess_gate| {
                            tess_gate.render(&cursor_tess)
                        })
                    },
                )
            },
        );

        ////////////////////////////////////////////////////////////////////////
        // Read modified framebuffer textures into host memory.               //
        ////////////////////////////////////////////////////////////////////////

        for target in &frame.dirty {
            if let Some(fb) = self.context.targets.get_mut(target) {
                store.put(*target, Image::new(fb.pixels()?, fb.size));
            }
        }

        Ok(())
    }

    fn scale(&mut self, factor: f32) -> f32 {
        self.ui_scale *= factor;
        self.ui_scale
    }

    fn handle_scale_factor_changed(&mut self, win_scale: f64) {
        self.win_scale = win_scale;
        self.handle_resized(self.win_size);
    }
}

impl Renderer {
    pub fn handle_resized(&mut self, size: platform::LogicalSize) {
        let physical = size.to_physical(self.win_scale);

        self.present_fb = Framebuffer::back_buffer(
            &mut self.backend,
            [physical.width as u32, physical.height as u32],
        )
        .unwrap();

        self.win_size = size;
        self.handle_scale_changed(self.win_scale);
    }

    pub fn handle_scale_changed(&mut self, scale: f64) {
        self.screen_fb = Framebuffer::new(
            &mut self.backend,
            [
                (self.win_size.width / scale) as u32,
                (self.win_size.height / scale) as u32,
            ],
            0,
            self::SAMPLER,
        )
        .unwrap();
    }
}
