use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context as _};
use directories as dirs;

use rx::app::images;
use rx::app::view::{View, ViewExtent, ViewId};
use rx::app::{DEFAULT_CURSORS, DEFAULT_FONT};
use rx::framework::ui::text::{FontFormat, FontId};
use rx::gfx::Image;

struct Options {
    paths: Vec<PathBuf>,
    fonts: Option<PathBuf>,
}

impl Options {
    fn parse() -> anyhow::Result<Self> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut paths = Vec::new();
        let mut fonts = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Long("fonts") => {
                    let folder = parser.value()?;
                    fonts = Some(PathBuf::from(folder));
                }
                Value(val) => {
                    let path = PathBuf::try_from(val)?;
                    paths.push(path);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        Ok(Self { paths, fonts })
    }
}

fn main() -> anyhow::Result<()> {
    rx::framework::logger::init(log::Level::Debug)?;

    let options = Options::parse()?;
    let proj_dirs =
        dirs::ProjectDirs::from("io", "cloudhead", "rx").context("config directory not found")?;
    let base_dirs = dirs::BaseDirs::new().context("home directory not found")?;
    let cursors = Image::try_from(DEFAULT_CURSORS).unwrap();

    // App data.
    let mut session = rx::app::Session::new(Path::new("."), proj_dirs, base_dirs);
    let id = ViewId::next();
    session.views.insert(
        id,
        View::new(id, ViewExtent::new(128, 128, 1), Image::blank([128, 128]))?,
    );
    session.views.activate(id);
    session.init()?;
    session.edit(options.paths.iter())?;

    // App UI.
    let ui = rx::app::ui::root(id);
    let fonts = if let Some(fonts) = options.fonts {
        fs::read_dir(fonts)?
            .map(|entry| {
                let entry = entry?;
                let font = fs::read(entry.path())?;
                let name = Path::new(&entry.file_name())
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();

                Ok::<_, anyhow::Error>((FontId::from(name), font, FontFormat::UF2))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![(FontId::default(), DEFAULT_FONT.to_owned(), FontFormat::UF2)]
    };

    if let Some((font, _, _)) = fonts.first() {
        session.settings.set("ui/font", font.to_string())?;
    } else {
        anyhow::bail!("No fonts found");
    }

    // let fonts = fonts
    //     .into_iter()
    //     .map(|(id, data, format)| (id, data.as_slice(), format));

    // } else {
    //     vec![(FontId::default(), DEFAULT_FONT, FontFormat::UF2)]
    // };

    rx::framework::Application::new("rx")
        .fonts(fonts)?
        .cursors(cursors)
        .image("pencil", Image::try_from(images::PENCIL).unwrap())
        .launch(ui, session)
        .map_err(Into::into)
}
