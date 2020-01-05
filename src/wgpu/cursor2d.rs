use rgx::core;
use rgx::core::*;
use rgx::math::Matrix4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Uniforms {
    ortho: Matrix4<f32>,
    scale: f32,
}

pub fn context(ortho: Matrix4<f32>, scale: f32) -> Uniforms {
    Uniforms { ortho, scale }
}

pub struct Pipeline {
    pipeline: core::Pipeline,

    pub cursor_binding: Option<core::BindingGroup>,
    pub framebuffer_binding: Option<core::BindingGroup>,

    uniform_buffer: core::UniformBuffer,
    uniform_binding: core::BindingGroup,
}

impl<'a> AbstractPipeline<'a> for Pipeline {
    type PrepareContext = self::Uniforms;
    type Uniforms = self::Uniforms;

    fn description() -> PipelineDescription<'a> {
        core::PipelineDescription {
            vertex_layout: &[VertexFormat::Float3, VertexFormat::Float2],
            pipeline_layout: &[
                Set(&[Binding {
                    // Ortho matrix & scaling factor.
                    binding: BindingType::UniformBuffer,
                    stage: ShaderStage::Vertex,
                }]),
                Set(&[
                    // Cursor texture.
                    Binding {
                        binding: BindingType::SampledTexture,
                        stage: ShaderStage::Fragment,
                    },
                    Binding {
                        binding: BindingType::Sampler,
                        stage: ShaderStage::Fragment,
                    },
                ]),
                Set(&[
                    // Screen framebuffer.
                    Binding {
                        binding: BindingType::SampledTexture,
                        stage: ShaderStage::Fragment,
                    },
                ]),
            ],
            // TODO: Use `env("CARGO_MANIFEST_DIR")`
            vertex_shader: include_bytes!("data/cursor.vert.spv"),
            fragment_shader: include_bytes!("data/cursor.frag.spv"),
        }
    }

    fn setup(pipeline: core::Pipeline, dev: &core::Device) -> Self {
        // XXX You can use any type here, and it won't complain!
        let m: Self::Uniforms = self::context(Matrix4::identity(), 1.0);
        let uniform_buffer = dev.create_uniform_buffer(&[m]);
        let uniform_binding =
            dev.create_binding_group(&pipeline.layout.sets[0], &[&uniform_buffer]);
        let framebuffer_binding = None;
        let cursor_binding = None;

        Self {
            pipeline,
            uniform_buffer,
            uniform_binding,
            framebuffer_binding,
            cursor_binding,
        }
    }

    fn apply(&self, pass: &mut Pass) {
        pass.set_pipeline(&self.pipeline);
        pass.set_binding(&self.uniform_binding, &[]);
    }

    fn prepare(
        &'a self,
        ctx: Self::Uniforms,
    ) -> Option<(&'a core::UniformBuffer, Vec<Self::Uniforms>)> {
        Some((&self.uniform_buffer, vec![ctx]))
    }
}

impl Pipeline {
    pub fn set_cursor(&mut self, texture: &Texture, sampler: &Sampler, r: &Renderer) {
        self.cursor_binding = Some(
            r.device
                .create_binding_group(&self.pipeline.layout.sets[1], &[texture, sampler]),
        );
    }

    pub fn set_framebuffer(&mut self, fb: &Framebuffer, r: &Renderer) {
        self.framebuffer_binding = Some(
            r.device
                .create_binding_group(&self.pipeline.layout.sets[2], &[fb]),
        );
    }
}
