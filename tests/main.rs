use rx::execution::Execution;
use std::env;
use std::io;
use std::path::Path;
use std::sync::Mutex;

#[macro_use]
extern crate lazy_static;

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
    let exec = Execution::replaying(path.clone(), true)?;
    let options = rx::Options {
        exec,
        resizable: false,
        source: Some(path.join(name).with_extension("rx")),
        ..Default::default()
    };

    {
        let _guard = MUTEX.lock();
        rx::init::<&str>(&[], options)
    }
}
