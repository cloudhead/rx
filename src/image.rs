use png;
use rgx::core::Rgba8;

use std::fs::File;
use std::io;
use std::path::Path;

pub fn decode<R: io::Read>(r: R) -> io::Result<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(r);
    let (info, mut reader) = decoder.read_info()?;
    let mut img = vec![0; info.buffer_size()];
    reader.next_frame(&mut img)?;

    Ok((img, info.width, info.height))
}

pub fn load<P: AsRef<Path>>(path: P) -> io::Result<(Vec<u8>, u32, u32)> {
    let f = File::open(&path)?;
    let decoder = png::Decoder::new(f);

    let (info, mut reader) = decoder.read_info().map_err(|_e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("couldn't decode `{}`", path.as_ref().display()),
        )
    })?;

    if info.color_type != png::ColorType::RGBA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "couldn't decode `{}`, only 8-bit RGBA images are supported",
                path.as_ref().display()
            ),
        ));
    }

    let (width, height) = (info.width as u32, info.height as u32);

    let mut buffer: Vec<u8> = vec![0; info.buffer_size()];
    reader.next_frame(&mut buffer).map_err(|_e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("couldn't decode `{}`", path.as_ref().display()),
        )
    })?;

    Ok((buffer, width, height))
}

pub fn save<P: AsRef<Path>>(path: P, w: u32, h: u32, pixels: &[Rgba8]) -> io::Result<()> {
    let f = File::create(path.as_ref())?;
    let out = &mut io::BufWriter::new(f);
    let mut encoder = png::Encoder::new(out, w, h);

    encoder.set_color(png::ColorType::RGBA);
    encoder.set_depth(png::BitDepth::Eight);

    let (head, pixels, tail) = unsafe { pixels.align_to::<u8>() };
    assert!(head.is_empty() && tail.is_empty());

    let mut writer = encoder.write_header()?;

    writer
        .write_image_data(pixels)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))
}
