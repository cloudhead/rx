use crate::image;
use crate::view::ViewExtent;

use crate::gfx::color::Rgba8;

use std::io;
use std::path::Path;

#[allow(dead_code)]
#[derive(Debug)]
pub struct Manifest {
    pub extent: ViewExtent,
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Archive {
    pub layers: Vec<Vec<Vec<Rgba8>>>,
    pub manifest: Manifest,
}

pub fn load_image<P: AsRef<Path>>(path: P) -> io::Result<(u32, u32, Vec<Rgba8>)> {
    let (buffer, width, height) = image::load(path)?;
    let pixels = Rgba8::align(&buffer);

    // TODO: (perf) Avoid the copy?

    Ok((width, height, pixels.into()))
}
