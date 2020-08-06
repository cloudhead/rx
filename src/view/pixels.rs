use rgx::color::{Bgra8, Rgba8};

#[derive(Debug, Copy, Clone)]
pub enum PixelFormat {
    Rgba8,
    Bgra8,
}

#[derive(Debug, Clone)]
pub struct Pixels {
    pub format: PixelFormat,
    buf: Box<[u32]>,
}

impl Pixels {
    pub fn blank(w: usize, h: usize) -> Self {
        let buf = vec![Rgba8::TRANSPARENT; w * h];
        Pixels::from_rgba8(buf.into())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn from_rgba8(buf: Box<[Rgba8]>) -> Self {
        let buf = unsafe { std::mem::transmute(buf) };
        Self {
            format: PixelFormat::Rgba8,
            buf,
        }
    }

    pub fn from_bgra8(buf: Box<[Bgra8]>) -> Self {
        let buf = unsafe { std::mem::transmute(buf) };
        Self {
            format: PixelFormat::Bgra8,
            buf,
        }
    }

    pub fn slice(&self, r: core::ops::Range<usize>) -> Vec<Rgba8> {
        match self.format {
            PixelFormat::Bgra8 => {
                let slice = &self.buf[r];
                slice.iter().map(|u| Bgra8::from(*u).into()).collect()
            }
            PixelFormat::Rgba8 => Rgba8::align(&self.buf[r]).to_vec(),
        }
    }

    pub fn get(&self, idx: usize) -> Option<Rgba8> {
        match self.format {
            PixelFormat::Rgba8 => self.buf.get(idx).cloned().map(Rgba8::from),
            PixelFormat::Bgra8 => self.buf.get(idx).cloned().map(|u| Bgra8::from(u).into()),
        }
    }

    pub fn into_rgba8(self) -> Vec<Rgba8> {
        match self.format {
            PixelFormat::Rgba8 => Rgba8::align(&self.buf).to_vec(),
            PixelFormat::Bgra8 => self
                .buf
                .iter()
                .cloned()
                .map(|u| Bgra8::from(u).into())
                .collect(),
        }
    }

    pub fn into_bgra8(self) -> Vec<Bgra8> {
        match self.format {
            PixelFormat::Rgba8 => self
                .buf
                .iter()
                .cloned()
                .map(|u| Rgba8::from(u).into())
                .collect(),
            PixelFormat::Bgra8 => Bgra8::align(&self.buf).to_vec(),
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = Rgba8> + 'a {
        self.buf.iter().cloned().map(move |u| match self.format {
            PixelFormat::Rgba8 => Rgba8::from(u),
            PixelFormat::Bgra8 => Bgra8::from(u).into(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        let (head, body, tail) = unsafe { self.buf.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        body
    }

    pub fn as_rgba8(&self) -> Option<&[Rgba8]> {
        match self.format {
            PixelFormat::Rgba8 => Some(Rgba8::align(&self.buf)),
            PixelFormat::Bgra8 => None,
        }
    }
}
