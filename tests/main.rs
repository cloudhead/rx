use rx::execution::Execution;
use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use serde_derive::Deserialize;
use toml;

#[macro_use]
extern crate lazy_static;

#[derive(Deserialize)]
struct Config {
    window: WindowConfig,
}

#[derive(Deserialize)]
struct WindowConfig {
    width: u32,
    height: u32,
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

////////////////////////////////////////////////////////////////////////////////

fn test(name: &str) {
    if let Err(e) = run(name) {
        panic!("test '{}' failed with: {}", name, e);
    }
}

fn run(name: &str) -> io::Result<()> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(name);
    let cfg: Config = {
        let path = path.join(name).with_extension("toml");
        let cfg = fs::read_to_string(&path).map_err(|e| {
            io::Error::new(e.kind(), format!("{}: {}", path.display(), e))
        })?;
        toml::from_str(&cfg)?
    };
    let exec = Execution::replaying(path.clone(), true)?;
    let options = rx::Options {
        exec,
        resizable: false,
        headless: true,
        source: Some(path.join(name).with_extension("rx")),
        width: cfg.window.width,
        height: cfg.window.height,
    };

    {
        let _guard = MUTEX.lock();
        rx::init::<&str>(&[], options)
    }
}
