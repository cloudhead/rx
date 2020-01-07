use crate::platform::{ControlFlow, GraphicsContext, LogicalSize, WindowEvent, WindowHint};

use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::io;

pub fn run<F, T>(mut _win: Window<T>, _events: Events, _callback: F) -> T
where
    F: 'static + FnMut(&mut Window<T>, WindowEvent) -> ControlFlow<T>,
{
    unimplemented!()
}

pub struct DummyWindow(());

unsafe impl HasRawWindowHandle for DummyWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        unreachable!()
    }
}

pub struct Events {
    handle: (),
}

pub struct Window<T> {
    handle: DummyWindow,
    phantom: std::marker::PhantomData<T>,
}

impl<T> Window<T> {
    pub fn request_redraw(&self) {
        unreachable!()
    }

    pub fn handle(&self) -> &DummyWindow {
        &self.handle
    }

    pub fn set_cursor_visible(&mut self, _visible: bool) {
        unreachable!()
    }

    pub fn scale_factor(&self) -> f64 {
        unreachable!()
    }

    pub fn size(&self) -> LogicalSize {
        unreachable!()
    }
}

pub fn init<T>(
    _title: &str,
    _w: u32,
    _h: u32,
    _hints: &[WindowHint],
    _context: GraphicsContext,
) -> io::Result<(Window<T>, Events)> {
    panic!("`dummy` platform initialized");
}
