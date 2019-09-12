//! Data included in the `rx` binary.

/// Initial (default) configuration for rx.
pub const CONFIG: &'static [u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/config/init.rx"));

/// Cursor sprites.
pub const CURSORS: &'static [u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/cursors.png"));

/// Glyphs used for font rendering.
pub const GLYPHS: &'static [u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/glyphs.png"));
