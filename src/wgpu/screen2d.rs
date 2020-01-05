use rgx::core;
use rgx::core::*;

pub struct Pipeline {
    pipeline: core::Pipeline,
}

impl<'a> AbstractPipeline<'a> for Pipeline {
    type PrepareContext = ();
    type Uniforms = ();

    fn description() -> PipelineDescription<'a> {
        core::PipelineDescription {
            vertex_layout: &[VertexFormat::Float2, VertexFormat::Float2],
            pipeline_layout: &[Set(&[
                Binding {
                    binding: BindingType::SampledTexture,
                    stage: ShaderStage::Fragment,
                },
                Binding {
                    binding: BindingType::Sampler,
                    stage: ShaderStage::Fragment,
                },
            ])],
            // TODO: Use `env("CARGO_MANIFEST_DIR")`
            vertex_shader: include_bytes!("data/screen.vert.spv"),
            fragment_shader: include_bytes!("data/screen.frag.spv"),
        }
    }

    fn setup(pipeline: core::Pipeline, _dev: &Device) -> Self {
        Self { pipeline }
    }

    fn apply(&self, pass: &mut Pass) {
        pass.set_pipeline(&self.pipeline);
    }

    fn prepare(&'a self, _ctx: ()) -> Option<(&'a UniformBuffer, Vec<()>)> {
        None
    }
}

impl Pipeline {
    pub fn binding(
        &self,
        renderer: &Renderer,
        framebuffer: &Framebuffer,
        sampler: &Sampler,
    ) -> core::BindingGroup {
        renderer
            .device
            .create_binding_group(&self.pipeline.layout.sets[0], &[framebuffer, sampler])
    }

    pub fn vertex_buffer(r: &Renderer) -> VertexBuffer {
        #[rustfmt::skip]
        let vertices: &[(f32, f32, f32, f32)] = &[
            (-1.0, -1.0, 0.0, 1.0),
            ( 1.0, -1.0, 1.0, 1.0),
            ( 1.0,  1.0, 1.0, 0.0),
            (-1.0, -1.0, 0.0, 1.0),
            (-1.0,  1.0, 0.0, 0.0),
            ( 1.0,  1.0, 1.0, 0.0),
        ];
        r.vertex_buffer(vertices)
    }
}
