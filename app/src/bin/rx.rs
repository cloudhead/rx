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
}

impl Options {
    fn parse() -> anyhow::Result<Self> {
        use lexopt::prelude::*;

        let mut parser = lexopt::Parser::from_env();
        let mut paths = Vec::new();

        while let Some(arg) = parser.next()? {
            match arg {
                Value(val) => {
                    let path = PathBuf::try_from(val)?;
                    paths.push(path);
                }
                _ => return Err(anyhow!(arg.unexpected())),
            }
        }
        Ok(Self { paths })
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

    rx::framework::Application::new("rx")
        .font(FontId::default(), DEFAULT_FONT, FontFormat::UF2)?
        .cursors(cursors)
        .image("pencil", Image::try_from(images::PENCIL).unwrap())
        .launch(ui, session)
        .map_err(Into::into)
}
