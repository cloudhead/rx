use std::path::Path;
use std::{env, fs, io, process};

use anyhow::Context as _;
use rx::gfx::{Rgba8, Size};

fn main() {
    if let Err(err) = execute() {
        eprintln!("{:#}", err);
        process::exit(1);
    }
}

fn execute() -> anyhow::Result<()> {
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
