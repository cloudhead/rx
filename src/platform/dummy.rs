use raw_window_handle::RawWindowHandle;

use crate::platform::{ControlFlow, LogicalSize, WindowEvent};

use std::io;

///////////////////////////////////////////////////////////////////////////////

pub fn run<F>(mut _win: Window, _events: Events, _callback: F)
where
    F: 'static + FnMut(&mut Window, WindowEvent) -> ControlFlow,
{
    unimplemented!()
}

pub struct Events {
    handle: (),
}

pub struct Window {
    handle: (),
}

impl Window {
    pub fn request_redraw(&self) {
        unreachable!()
    }

    pub fn raw_handle(&self) -> RawWindowHandle {
        unreachable!()
    }

    pub fn set_cursor_visible(&mut self, _visible: bool) {
        unreachable!()
    }

    pub fn scale_factor(&self) -> f64 {
        unreachable!()
    }

    pub fn size(&self) -> io::Result<LogicalSize> {
        unreachable!()
    }
}

pub fn init(_title: &str) -> io::Result<(Window, Events)> {
    panic!("`dummy` platform initialized");
}
