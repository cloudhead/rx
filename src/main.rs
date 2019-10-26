use rx;
use rx::session;

use clap::{App, Arg};

use std::process;

fn main() {
    rx::ALLOCATOR.reset();

    let mut options = rx::Options::default();
    let matches = App::new("rx")
        .version(&*format!("v{}", rx::VERSION))
        .author("Alexis Sellier <self@cloudhead.io>")
        .about("A Modern & Extensible Pixel Editor")
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the verbosity level"),
        )
        .arg(
            Arg::with_name("width")
                .long("width")
                .takes_value(true)
                .help("Sets the window width"),
        )
        .arg(
            Arg::with_name("height")
                .long("height")
                .takes_value(true)
                .help("Sets the window height"),
        )
        .arg(
            Arg::with_name("replay")
                .long("replay")
                .value_name("FILE")
                .help("Replay input from a file"),
        )
        .arg(
            Arg::with_name("record")
                .long("record")
                .value_name("FILE")
                .help("Record input to a file"),
        )
        .arg(Arg::with_name("path").multiple(true))
        .get_matches_safe()
        .unwrap_or_else(|e| match e.kind {
            clap::ErrorKind::HelpDisplayed
            | clap::ErrorKind::VersionDisplayed => {
                println!("{}", e.message);
                process::exit(0);
            }
            _ => fatal(e.message),
        });

    if matches.is_present("replay") && matches.is_present("record") {
        fatal("error: '--replay' and '--record' can't both be specified");
    }

    let paths = matches
        .values_of("path")
        .map_or(Vec::new(), |m| m.collect::<Vec<_>>());

    options.log = match matches.occurrences_of("v") {
        0 => "rx=warn",
        1 => "rx=info,error",
        2 => "rx=debug,error",
        3 => "rx=debug,error",
        4 => "rx=debug,info",
        _ => "debug",
    };

    if let Some(w) = matches.value_of("width") {
        match w.parse::<u32>() {
            Ok(w) => {
                options.width = w;
            }
            Err(_) => fatal("error: couldn't parse `--width` value specified"),
        }
    }
    if let Some(h) = matches.value_of("height") {
        match h.parse::<u32>() {
            Ok(h) => {
                options.height = h;
            }
            Err(_) => fatal("error: couldn't parse `--height` value specified"),
        }
    }

    if let Some(path) = matches.value_of("replay") {
        match session::ExecutionMode::replaying(path) {
            Err(e) => {
                fatal(format!("initialization error: {}", e));
            }
            Ok(exec) => {
                options.exec = exec;
            }
        }
    } else if let Some(path) = matches.value_of("record") {
        match session::ExecutionMode::recording(path) {
            Err(e) => {
                fatal(format!("initialization error: {}", e));
            }
            Ok(exec) => {
                options.exec = exec;
            }
        }
    };

    if let Err(e) = rx::init(&paths, options) {
        fatal(format!("error: initializing: {}", e));
    }
}

fn fatal<S: AsRef<str>>(msg: S) -> ! {
    eprintln!("rx: {}", msg.as_ref());
    process::exit(1);
}
