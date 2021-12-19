use super::*;
use crate::math::Matrix4;

#[derive(Copy, Clone)]
pub struct AlignedBuffer {
    // TODO: Make this generic when rust-lang#43408 is fixed.
    _data: Matrix4<f32>,
    _padding: [u8; AlignedBuffer::PAD],
}

impl AlignedBuffer {
    pub const ALIGNMENT: u64 = 256;
    pub const PAD: usize = Self::ALIGNMENT as usize - std::mem::size_of::<Matrix4<f32>>();

    pub fn new(data: Matrix4<f32>) -> Self {
        Self {
            _data: data,
            _padding: [0u8; AlignedBuffer::PAD],
        }
    }
}

pub struct TransformBuffer {
    pub binding: BindingGroup,

    buf: UniformBuffer,
    size: usize,
    cap: usize,
}

impl TransformBuffer {
    pub fn new(layout: &BindingGroupLayout, transforms: &[Matrix4<f32>], dev: &Device) -> Self {
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

    pub fn with_capacity(cap: usize, layout: &BindingGroupLayout, dev: &Device) -> Self {
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

    pub fn offsets(&self) -> std::iter::StepBy<std::ops::Range<u64>> {
        let max: u64 = self.size as u64 * AlignedBuffer::ALIGNMENT;
        (0..max).step_by(AlignedBuffer::ALIGNMENT as usize)
    }

    pub fn update(&mut self, transforms: &[Matrix4<f32>], r: &Renderer, f: &mut Frame) {
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
