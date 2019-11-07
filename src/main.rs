use rx;
use rx::execution::Execution;

use pico_args;

use std::path::PathBuf;
use std::process;

const HELP: &'static str = r#"
A Modern & Extensible Pixel Editor
Alexis Sellier <self@cloudhead.io>

USAGE
    rx [OPTIONS] [<path>..]

OPTIONS
    -h, --help           Prints help
    -V, --version        Prints version

    -v                   Verbose mode (verbosity=2)
    -u <script>          Use the commands in <script> for initialization

    --verbosity <level>  Set verbosity level (0-5)
    --record <file>      Record user input to a file
    --replay <file>      Replay user input from a file
    --width <width>      Set the window width
    --height <height>    Set the window height
"#;

fn main() {
    if let Err(_) = self::execute(pico_args::Arguments::from_env()) {
        process::exit(1);
    }
}

fn execute(
    mut args: pico_args::Arguments,
) -> Result<(), Box<dyn std::error::Error>> {
    rx::ALLOCATOR.reset();

    let default = rx::Options::default();

    if args.contains(["-h", "--help"]) {
        println!("rx v{}{}", rx::VERSION, HELP);
        return Ok(());
    }

    if args.contains(["-V", "--version"]) {
        println!("rx v{}", rx::VERSION);
        return Ok(());
    }

    let verbose = args.contains("-v");
    let width = args.opt_value_from_str("--width")?;
    let height = args.opt_value_from_str("--height")?;
    let digest = args.contains("--digest");
    let source = args.opt_value_from_str::<_, PathBuf>("-u")?;
    let replay = args.opt_value_from_str::<_, PathBuf>("--replay")?;
    let record = args.opt_value_from_str::<_, PathBuf>("--record")?;
    let resizable = width.is_none() && height.is_none() && !digest;

    if replay.is_some() && record.is_some() {
        return Err("'--replay' and '--record' can't both be specified".into());
    }

    let log = match args
        .opt_value_from_str("--verbosity")?
        .unwrap_or(if verbose { 2 } else { 0 })
    {
        0 => "rx=info",
        1 => "rx=info,error",
        2 => "rx=debug,error",
        3 => "rx=debug,info",
        _ => "debug",
    };

    let exec = if let Some(path) = replay {
        Execution::replaying(path.with_extension("log"), digest)?
    } else if let Some(path) = record {
        Execution::recording(path.with_extension("log"), digest)?
    } else {
        default.exec
    };

    let options = rx::Options {
        exec,
        log,
        width: width.unwrap_or(default.width),
        height: height.unwrap_or(default.height),
        resizable,
        source,
    };

    let paths = args.free()?;
    rx::init(&paths, options).map_err(|e| e.into())
}
