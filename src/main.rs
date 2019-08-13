use clap::{App, Arg};
use vx;

fn main() {
    let matches = App::new("rx")
        .version("0.1.0")
        .author("Alexis Sellier <self@cloudhead.io>")
        .about("An Extensible Pixel Editor")
        .arg(Arg::with_name("path").multiple(true))
        .get_matches();

    let paths = matches
        .values_of("path")
        .map_or(Vec::new(), |m| m.collect::<Vec<_>>());

    vx::init(&paths);
}
