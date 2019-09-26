use clap::{App, Arg};
use log;
use rx;

fn main() {
    rx::ALLOCATOR.reset();

    let matches = App::new("rx")
        .version(&*format!("v{}", rx::VERSION))
        .author("Alexis Sellier <self@cloudhead.io>")
        .about("An Extensible Pixel Editor")
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the verbosity level"),
        )
        .arg(Arg::with_name("path").multiple(true))
        .get_matches();

    let paths = matches
        .values_of("path")
        .map_or(Vec::new(), |m| m.collect::<Vec<_>>());

    let log = match matches.occurrences_of("v") {
        0 => "rx=warn",
        1 => "rx=info,error",
        2 => "rx=debug,error",
        3 => "rx=debug,error",
        4 => "rx=debug,info",
        _ => "debug",
    };

    if let Err(e) = rx::init(&paths, rx::Options { log }) {
        log::error!("Error initializing rx: {}", e);
    }
}
