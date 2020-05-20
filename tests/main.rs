use rx::execution::{DigestMode, ExecutionMode};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_derive::Deserialize;
use toml;

#[macro_use]
extern crate lazy_static;

#[derive(Deserialize)]
struct Config {
    window: WindowConfig,
    assets: AssetConfig,
}

#[derive(Deserialize)]
struct WindowConfig {
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct AssetConfig {
    glyphs: PathBuf,
}

lazy_static! {
    /// This mutex is here to prevent certain tests from running
    /// in parallel. This is due to the fact that we spawn windows
    /// and graphics contexts which are not thread-safe.
    pub static ref MUTEX: Mutex<()> = Mutex::new(());
}

#[test]
fn simple() {
    test("simple");
}

#[test]
fn resize() {
    test("resize");
}

#[test]
fn visual() {
    test("visual");
}

#[test]
fn palette() {
    test("palette");
}

#[test]
fn snapshots() {
    test("snapshots");
}

#[test]
fn saving() {
    test("saving");
}

#[test]
fn views() {
    test("views");
}

#[test]
fn yank_paste() {
    test("yank-paste");
}

#[test]
fn brush_basic() {
    test("brush-basic");
}

#[test]
fn brush_advanced() {
    test("brush-advanced");
}

#[test]
fn frames() {
    test("frames");
}

#[test]
fn ui() {
    test("ui");
}

#[test]
fn grid() {
    test("grid");
}

#[test]
fn source() {
    test("source");
}

#[test]
fn mouse() {
    test("mouse");
}

#[test]
fn visual_mouse() {
    test("visual-mouse");
}

#[test]
fn layers() {
    test("layers");
}

#[test]
fn layers_snapshots() {
    test("layers-snapshots");
}

#[test]
fn archive() {
    test("archive");
}

#[test]
fn organize_views() {
    test("organize-views");
}

////////////////////////////////////////////////////////////////////////////////

fn test(name: &str) {
    if let Err(e) = run(name) {
        panic!("test '{}' failed with: {}", name, e);
    }
}

fn run(name: &str) -> io::Result<()> {
    // We allow tests to create these temporary files,
    // so make sure it's not there when a test is run.
    fs::remove_file("/tmp/rx.png").ok();
    fs::remove_file("/tmp/archive.rxa").ok();

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    let cfg: Config = {
        let path = path.join(name).with_extension("toml");
        let cfg = fs::read_to_string(&path)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;
        toml::from_str(&cfg)?
    };
    let glyphs = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(&cfg.assets.glyphs))
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))?;

    let glyphs = glyphs.as_slice();

    let options = rx::Options {
        resizable: false,
        headless: true,
        source: Some(path.join(name).with_extension("rx")),
        width: cfg.window.width,
        height: cfg.window.height,
        exec: ExecutionMode::Replay(path.clone(), DigestMode::Verify),
        glyphs,
        debug: false,
    };

    {
        let _guard = MUTEX.lock();
        rx::init::<&str>(&[], options)
    }
}
