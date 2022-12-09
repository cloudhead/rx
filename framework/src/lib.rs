#![allow(clippy::needless_range_loop)]
#![allow(clippy::useless_format)]
#![allow(clippy::single_match)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::should_implement_trait)]
#![warn(rust_2018_idioms)]
use std::alloc::System;

#[macro_use]
extern crate log;

pub mod alloc;
pub mod application;
pub mod gfx;
pub mod logger;
pub mod platform;
pub mod renderer;
pub mod timer;
pub mod ui;

pub use application::Application;
pub use ui::Widget;

/// Program version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[global_allocator]
pub static ALLOCATOR: alloc::Allocator = alloc::Allocator::new(System);
