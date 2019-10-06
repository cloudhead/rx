use rx;
use rx::session;

use clap::{App, Arg};

use std::process;

fn main() {
    rx::ALLOCATOR.reset();

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

    if matches.is_present("replay") && matches.is_present("record") {
        fatal("error: '--replay' and '--record' can't both be specified");
    }

    let exec = if let Some(path) = matches.value_of("replay") {
        session::ExecutionMode::replaying(path)
    } else if let Some(path) = matches.value_of("record") {
        session::ExecutionMode::recording(path)
    } else {
        session::ExecutionMode::normal()
    };

    if let Err(e) = exec {
        fatal(format!("initialization error: {}", e));
    } else if let Ok(exec) = exec {
        if let Err(e) = rx::init(&paths, rx::Options { log, exec }) {
            fatal(format!("initialization error: {}", e));
        }
    }
}

fn fatal<S: AsRef<str>>(msg: S) -> ! {
    eprintln!("rx: {}", msg.as_ref());
    process::exit(1);
}
