//! PNG to RGBA converter.
use std::fs::File;
use std::path::Path;
use std::{env, fs, io, process};

use anyhow::Context as _;
use rx::gfx::{Rgba8, Size};

fn main() {
    if let Err(err) = png_to_rgba() {
        eprintln!("{:#}", err);
        process::exit(1);
    }
}

fn png_to_rgba() -> anyhow::Result<()> {
    let arg = env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("missing .png file path"))?;
    let path = Path::new(&arg);
    let file = fs::read(path).context(format!("`{}`", path.display()))?;
    let (data, size) = decode(file.as_slice())?;
    let rgba = fs::File::create(path.with_extension("rgba"))?;

    rx::gfx::Image::new(data, size).write(rgba)?;

    Ok(())
}

fn rgba_to_png() -> anyhow::Result<()> {
    let arg = env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("missing .rgba file path"))?;
    let path = Path::new(&arg);
    let file = fs::read(path).context(format!("`{}`", path.display()))?;
    let image = rx::gfx::Image::try_from(file.as_slice())?;

    save_as(
        path.with_extension("png"),
        image.size.w,
        image.size.h,
        &image.pixels,
    )?;

    Ok(())
}

pub fn save_as<P: AsRef<Path>>(path: P, w: u32, h: u32, pixels: &[Rgba8]) -> io::Result<()> {
    let f = File::create(path.as_ref())?;
    let out = &mut io::BufWriter::new(f);

    self::write(out, w, h, pixels)
}

pub fn write<W: io::Write>(out: W, w: u32, h: u32, pixels: &[Rgba8]) -> io::Result<()> {
    let width = w;
    let height = h;
    let mut encoder = png::Encoder::new(out, width, height);

    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header()?;
    let pixels = self::align_u8(pixels);

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

fn decode<R: io::Read>(reader: R) -> io::Result<(Vec<Rgba8>, Size<u32>)> {
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

    Ok((Rgba8::into_vec(buffer), size))
}
