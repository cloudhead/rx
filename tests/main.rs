use rx::execution::Execution;
use std::env;
use std::io;
use std::path::Path;

#[test]
fn visual_mode() {
    test("visual-mode");
}

#[test]
fn palette() {
    test("palette");
}

////////////////////////////////////////////////////////////////////////////////

fn test(name: &str) {
    let path = Path::new("tests/").join(name);
    env::set_current_dir(path).unwrap();

    if let Err(e) = run(name) {
        panic!("test '{}' failed with: {}", name, e);
    }
}

fn run(name: &str) -> io::Result<()> {
    let path = Path::new(name).with_extension("events");
    let exec = Execution::replaying(path, true)?;
    let options = rx::Options {
        exec,
        log: "rx=info",
        resizable: false,
        source: None,
        ..Default::default()
    };
    let paths: &[String] = &[];

    rx::init(paths, options)
}
