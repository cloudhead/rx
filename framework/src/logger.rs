//! Logging module.
use std::io::Write;
use std::time::SystemTime;

use log::{Level, Log, Metadata, Record, SetLoggerError};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

struct Logger {
    level: Level,
    start: SystemTime,
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            if record.level() == Level::Error {
                write(
                    record,
                    StandardStream::stderr(ColorChoice::Always),
                    self.start,
                );
            } else {
                write(
                    record,
                    StandardStream::stdout(ColorChoice::Always),
                    self.start,
                );
            }

            fn write(record: &log::Record<'_>, mut stream: StandardStream, start: SystemTime) {
                let now = SystemTime::now().duration_since(start).unwrap();
                let message = format!(
                    "{:5} {:012} {}",
                    record.level(),
                    now.as_millis(),
                    record.args()
                );
                match record.level() {
                    Level::Info => stream.set_color(ColorSpec::new().set_fg(Some(Color::Cyan))),
                    Level::Error => stream
                        .set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_intense(true)),
                    Level::Warn => stream.set_color(ColorSpec::new().set_fg(Some(Color::Yellow))),
                    Level::Debug => stream.set_color(ColorSpec::new().set_fg(None)),
                    Level::Trace => stream
                        .set_color(ColorSpec::new().set_fg(Some(Color::White)).set_dimmed(true)),
                }
                .ok();

                writeln!(stream, "{}", message).ok();
            }
        }
    }

    fn flush(&self) {}
}

/// Initialize a new logger.
pub fn init(level: Level) -> Result<(), SetLoggerError> {
    let logger = Logger {
        level,
        start: SystemTime::now(),
    };

    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(level.to_level_filter());

    Ok(())
}
