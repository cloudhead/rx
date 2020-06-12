use rgx::color::Rgba8;

use std::convert::TryFrom;
use std::ffi::OsStr;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{self, PathBuf};

enum Encoding {
    Png,
}

impl Encoding {
    fn from(s: &OsStr) -> Option<Self> {
        if s == "png" {
            Some(Self::Png)
        } else {
            None
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Png => "png",
        }
    }
}

pub struct Path {
    parent: PathBuf,
    name: String,
    encoding: Encoding,
}

#[allow(dead_code)]
impl Path {
    pub fn file_stem(&self) -> &str {
        self.name.as_str()
    }

    pub fn extension(&self) -> &str {
        self.encoding.as_str()
    }

    pub fn parent(&self) -> &path::Path {
        self.parent.as_path()
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}.{}",
            self.parent.join(&self.name).display(),
            self.encoding.as_str()
        )
    }
}

impl TryFrom<&path::Path> for Path {
    type Error = String;

    fn try_from(p: &path::Path) -> Result<Self, Self::Error> {
        p.parent()
            .and_then(|parent| {
                p.file_stem().and_then(|name| {
                    p.extension().and_then(|ext| {
                        Encoding::from(ext).map(|encoding| Self {
                            parent: parent.into(),
                            name: name.to_string_lossy().into_owned(),
                            encoding,
                        })
                    })
                })
            })
            .ok_or_else(|| format!("`{}` is not a valid path", p.display()))
    }
}

pub fn load<P: AsRef<path::Path>>(path: P) -> io::Result<(Vec<u8>, u32, u32)> {
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

pub fn read<R: io::Read>(reader: R) -> io::Result<(Vec<u8>, u32, u32)> {
    let decoder = png::Decoder::new(reader);

    let (info, mut reader) = decoder
        .read_info()
        .map_err(|_e| io::Error::new(io::ErrorKind::InvalidData, "decoding failed"))?;

    if info.color_type != png::ColorType::RGBA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "only 8-bit RGBA images are supported",
        ));
    }

    let (width, height) = (info.width as u32, info.height as u32);

    let mut buffer: Vec<u8> = vec![0; info.buffer_size()];
    reader
        .next_frame(&mut buffer)
        .map_err(|_e| io::Error::new(io::ErrorKind::InvalidData, "decoding failed"))?;

    Ok((buffer, width, height))
}

pub fn save<P: AsRef<path::Path>>(path: P, w: u32, h: u32, pixels: &[Rgba8]) -> io::Result<()> {
    let f = File::create(path.as_ref())?;
    let out = &mut io::BufWriter::new(f);

    self::write(out, w, h, pixels)
}

pub fn write<W: io::Write>(out: W, w: u32, h: u32, pixels: &[Rgba8]) -> io::Result<()> {
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

#[cfg(test)]
mod test {
    use super::Path;
    use std::convert::TryFrom;
    use std::path;

    #[test]
    fn test_image_path() {
        assert!(Path::try_from(path::Path::new("/")).is_err());
        assert!(Path::try_from(path::Path::new("")).is_err());
        assert!(Path::try_from(path::Path::new("..")).is_err());
        assert!(Path::try_from(path::Path::new("acme")).is_err());
        assert!(Path::try_from(path::Path::new("/acme/..")).is_err());
        assert!(Path::try_from(path::Path::new("acme/rx")).is_err());
        assert!(Path::try_from(path::Path::new(".png")).is_err());
        assert!(Path::try_from(path::Path::new("acme.yaml")).is_err());

        for p in [
            "../../acme.png",
            "/acme.png",
            "acme.png",
            "rx/acme.png",
            "acme.beb.png",
            ".acme.png",
        ]
        .iter()
        {
            assert_eq!(Path::try_from(path::Path::new(p)).unwrap().to_string(), *p);
        }
    }
}
