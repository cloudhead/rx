use std::convert::TryFrom;
use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{self, PathBuf};

use crate::gfx::pixels;
use crate::gfx::prelude::{Rgba8, Size};

pub fn open<P: AsRef<path::Path>>(path: P) -> io::Result<(Vec<Rgba8>, Size<u32>)> {
    let f = File::open(&path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("error opening {}: {}", path.as_ref().display(), e),
        )
    })?;

    self::read(f).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("error loading {}: {}", path.as_ref().display(), e),
        )
    })
}

pub fn read<R: io::Read>(reader: R) -> io::Result<(Vec<Rgba8>, Size<u32>)> {
    let decoder = png::Decoder::new(reader);

    let mut reader = decoder
        .read_info()
        .map_err(|_e| io::Error::new(io::ErrorKind::InvalidData, "decoding failed"))?;
    let info = reader.info();
    let size = Size::new(info.width as u32, info.height as u32);

    if info.color_type != png::ColorType::Rgba {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only 8-bit RGBA images are supported",
        ));
    }

    let mut buffer: Vec<u8> = vec![0; reader.output_buffer_size()];
    reader
        .next_frame(&mut buffer)
        .map_err(|_e| io::Error::new(io::ErrorKind::InvalidData, "decoding failed"))?;

    let pixels = Rgba8::into_vec(buffer);

    Ok((pixels, size))
}

pub fn save_as<P: AsRef<path::Path>>(
    path: P,
    w: u32,
    h: u32,
    scale: u32,
    pixels: &[Rgba8],
) -> io::Result<()> {
    let f = File::create(path.as_ref())?;
    let out = &mut io::BufWriter::new(f);

    self::write(out, w, h, scale, pixels)
}

pub fn write<W: io::Write>(out: W, w: u32, h: u32, scale: u32, pixels: &[Rgba8]) -> io::Result<()> {
    let width = w * scale;
    let height = h * scale;
    let mut encoder = png::Encoder::new(out, width, height);

    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;

    if scale == 1 {
        let pixels = self::align_u8(pixels);

        return writer
            .write_image_data(pixels)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
    }
    let scaled = pixels::scale(pixels, w, h, scale);
    let pixels = self::align_u8(scaled.as_slice());

    writer
        .write_image_data(pixels)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}

fn align_u8<T>(data: &[T]) -> &[u8] {
    let (head, body, tail) = unsafe { data.align_to::<u8>() };

    assert!(head.is_empty());
    assert!(tail.is_empty());

    body
}
