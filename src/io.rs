use crate::image;
use crate::view::ViewExtent;

use rgx::color::Rgba8;

use miniserde::{json, Deserialize, Serialize};

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub extent: ViewExtent,
}

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

pub fn load_archive<P: AsRef<Path>>(path: P) -> io::Result<Archive> {
    use std::io::Read;
    use zip::result::ZipError;

    let file = File::open(&path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let files: Vec<_> = archive.file_names().map(|f| PathBuf::from(f)).collect();

    // Get the root directory name of the archive.
    let root = files
        .first()
        .and_then(|f| f.iter().next())
        .map(Path::new)
        .ok_or(io::Error::new(io::ErrorKind::Other, "invalid archive"))?;

    // Decode the manifest file.
    let manifest: Manifest = {
        let mut buf = String::new();

        archive
            .by_name(&root.join("manifest.json").to_string_lossy())?
            .read_to_string(&mut buf)?;
        json::from_str(&buf).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?
    };

    let mut layers = Vec::new();

    // Discover the layers and frames in the archive.
    for layer in 0.. {
        let path = root.join("layers").join(layer.to_string()).join("frames");

        if !files.iter().any(|f| f.starts_with(&path)) {
            break;
        }

        let mut frames = Vec::new();
        for frame in 0.. {
            let path = path.join(frame.to_string()).with_extension("png");

            match archive.by_name(&path.to_string_lossy()) {
                Ok(mut file) => {
                    let mut img = Vec::new();
                    file.read_to_end(&mut img)?;

                    let (pixels, w, h) = image::read(img.as_slice())?;
                    debug_assert!(w == manifest.extent.fw && h == manifest.extent.fh);

                    frames.push(unsafe { std::mem::transmute(pixels) });
                }
                Err(ZipError::FileNotFound) => {
                    break;
                }
                Err(err) => return Err(err.into()),
            }
        }
        layers.push(frames);
    }

    Ok(Archive { layers, manifest })
}
