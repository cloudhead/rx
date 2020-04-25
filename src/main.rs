use rx::execution::{DigestMode, ExecutionMode, GifMode};

use std::io;
use std::path::PathBuf;
use std::process;

const HEADER: &str = r#"
Alexis Sellier <self@cloudhead.io>
A Modern & Extensible Pixel Editor
"#;

const HELP: &str = r#"
USAGE
    rx [OPTIONS] [<path>..]

OPTIONS
    -h, --help           Prints help
    -V, --version        Prints version

    -v                   Verbose mode
    -u <script>          Use the commands in <script> for initialization

    --record <dir>       Record user input to a directory
    --replay <dir>       Replay user input from a directory
    --width <width>      Set the window width
    --height <height>    Set the window height
    --debug              Set debug mode
"#;

fn main() {
    if let Err(e) = self::execute(pico_args::Arguments::from_env()) {
        eprintln!("rx: {}", e);
        process::exit(1);
    }
}

fn execute(mut args: pico_args::Arguments) -> Result<(), Box<dyn std::error::Error>> {
    rx::ALLOCATOR.reset();

    let default = rx::Options::default();

    if args.contains(["-h", "--help"]) {
        println!("rx v{}{}{}", rx::VERSION, HEADER, HELP);
        return Ok(());
    }

    if args.contains(["-V", "--version"]) {
        println!("rx v{}", rx::VERSION);
        return Ok(());
    }

    let verbose = args.contains("-v");
    let debug = args.contains("--debug");
    let width = args.opt_value_from_str("--width")?;
    let height = args.opt_value_from_str("--height")?;
    let record_digests = args.contains("--record-digests");
    let record_gif = args.contains("--record-gif");
    let verify_digests = args.contains("--verify-digests");
    let headless = args.contains("--headless");
    let source = args.opt_value_from_str::<_, PathBuf>("-u")?;
    let replay = args.opt_value_from_str::<_, PathBuf>("--replay")?;
    let record = args.opt_value_from_str::<_, PathBuf>("--record")?;
    let resizable = width.is_none() && height.is_none() && replay.is_none() && record.is_none();

    if replay.is_some() && record.is_some() {
        return Err("'--replay' and '--record' can't both be specified".into());
    }

    let digest_mode = if record_digests && !verify_digests {
        DigestMode::Record
    } else if verify_digests && !record_digests {
        DigestMode::Verify
    } else if !verify_digests && !record_digests {
        DigestMode::Ignore
    } else {
        return Err("'--record-digests' and '--verify-digests' can't both be specified".into());
    };

    let gif_mode = if record_gif {
        GifMode::Record
    } else {
        GifMode::Ignore
    };

    if record_gif && record.is_none() && replay.is_none() {
        return Err("'--record-gif' has no effect without '--record' or '--replay'".into());
    }
    if record_digests && record.is_none() && replay.is_none() {
        return Err("'--record-digests' has no effect without '--record' or '--replay'".into());
    }

    let log_lvl = if verbose {
        log::Level::Debug
    } else {
        log::Level::Info
    };
    simple_logger::init_with_level(log_lvl)?;

    let width = width.unwrap_or(default.width);
    let height = height.unwrap_or(default.height);

    let exec = if let Some(path) = replay {
        ExecutionMode::Replay(path, digest_mode)
    } else if let Some(path) = record {
        ExecutionMode::Record(path, digest_mode, gif_mode)
    } else {
        ExecutionMode::Normal
    };

    let glyphs = rx::data::GLYPHS;

    let options = rx::Options {
        width,
        height,
        headless,
        resizable,
        source,
        exec,
        glyphs,
        debug,
    };

    match args.free() {
        Ok(paths) => rx::init(&paths, options).map_err(|e| e.into()),
        Err(e) => {
            Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{}\n{}", e, HELP)).into())
        }
    }
}
