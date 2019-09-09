use rgx::core;
use rgx::kit::AlignedBuffer;
use rgx::math::Matrix4;

pub struct TransformBuffer {
    pub binding: core::BindingGroup,

    buf: core::UniformBuffer,
    size: usize,
    cap: usize,
}

impl TransformBuffer {
    #[allow(dead_code)]
    pub fn new(
        layout: &core::BindingGroupLayout,
        transforms: &[Matrix4<f32>],
        dev: &core::Device,
    ) -> Self {
        let aligned = Self::aligned(transforms);
        let buf = dev.create_uniform_buffer(aligned.as_slice());
        let binding = dev.create_binding_group(&layout, &[&buf]);
        let size = transforms.len();

        Self {
            buf,
            binding,
            size,
            cap: size,
        }
    }

    pub fn with_capacity(
        cap: usize,
        layout: &core::BindingGroupLayout,
        dev: &core::Device,
    ) -> Self {
        let aligned = AlignedBuffer::new(Matrix4::identity());

        let mut transforms = Vec::with_capacity(cap);
        transforms.resize(cap, aligned);

        let buf = dev.create_uniform_buffer(transforms.as_slice());
        let binding = dev.create_binding_group(&layout, &[&buf]);

        Self {
            buf,
            binding,
            size: 0,
            cap,
        }
    }

    pub fn iter_offset(&self) -> std::iter::StepBy<std::ops::Range<u64>> {
        let max: u64 = self.size as u64 * AlignedBuffer::ALIGNMENT;
        (0..max).step_by(AlignedBuffer::ALIGNMENT as usize)
    }

    pub fn update(
        &mut self,
        transforms: &[Matrix4<f32>],
        r: &core::Renderer,
        f: &mut core::Frame,
    ) {
        let len = transforms.len();
        assert!(len <= self.cap, "fatal: capacity exceeded");

        let data = Self::aligned(transforms);
        let src = r.uniform_buffer(data.as_slice());

        f.copy(&src, &self.buf);

        self.size = len;
    }

    fn aligned(transforms: &[Matrix4<f32>]) -> Vec<AlignedBuffer> {
        let mut aligned = Vec::with_capacity(transforms.len());
        for t in transforms {
            aligned.push(AlignedBuffer::new(*t));
        }
        aligned
    }
}
