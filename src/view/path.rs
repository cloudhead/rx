use std::convert::TryFrom;
use std::io;
use std::ops::Deref;
use std::path;

#[derive(Debug, Copy, Clone)]
pub enum Format {
    Archive,
    Png,
    Gif,
}

#[derive(Debug, Clone)]
pub struct Path<'a, T: ?Sized> {
    pub format: Format,

    raw: &'a T,
}

impl<'a> Deref for Path<'a, path::Path> {
    type Target = path::Path;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl<'a> TryFrom<&'a path::Path> for Path<'a, path::Path> {
    type Error = io::Error;

    fn try_from(path: &'a path::Path) -> Result<Self, io::Error> {
        if path.is_dir() {
            return Err(io::Error::new(io::ErrorKind::Other, "file is a directory"));
        }

        let ext = path
            .extension()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "file requires an extension"))?;
        let ext = ext
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "file extension is not valid"))?;

        let format = match ext {
            "gif" => Format::Gif,
            "png" => Format::Png,
            "rxi" => Format::Archive,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("file extension is not supported: .{}", ext),
                ))
            }
        };

        Ok(Self { raw: path, format })
    }
}
