#![allow(clippy::for_kv_map)]
#![allow(clippy::useless_format)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::single_match)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::should_implement_trait)]
#![warn(rust_2018_idioms)]
#[macro_use]
extern crate log;

#[macro_use]
pub mod util;
pub mod autocomplete;
pub mod brush;
pub mod bucket;
pub mod command;
pub mod command_line;
pub mod history;
pub mod keyboard;
pub mod palette;
#[cfg(feature = "png")]
pub mod png;
pub mod script;
pub mod session;
pub mod settings;
pub mod ui;
pub mod view;

pub mod app {
    /// Initial (default) configuration for rx.
    pub const DEFAULT_CONFIG: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/init.rx"));
    pub const DEFAULT_FONT: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/gohu14.uf2"));
    pub const DEFAULT_CURSORS: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/cursors.rgba"));

    pub mod images {
        pub const PENCIL: &[u8] =
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pencil.rgba"));
    }

    pub use super::autocomplete;
    pub use super::brush;
    pub use super::bucket;
    pub use super::command;
    pub use super::command_line;
    pub use super::history;
    pub use super::keyboard;
    pub use super::palette;
    pub use super::script;
    pub use super::session;
    pub use super::settings;
    pub use super::ui;
    pub use super::util;
    pub use super::view;

    pub use command::Command;
    pub use command_line::CommandLine;
    pub use palette::Palette;
    pub use session::{Direction, Mode, Session, Tool, VisualState};
    pub use settings::Settings;
}

pub use rx_framework as framework;
pub use rx_framework::gfx;
