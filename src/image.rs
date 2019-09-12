use png;

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
