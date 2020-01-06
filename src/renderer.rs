use crate::execution::Execution;
use crate::platform::{self, LogicalSize};
use crate::resources::ResourceManager;
use crate::session::{self, Effect, PresentMode, Session};
use crate::view::ViewManager;

use std::cell::RefCell;
use std::rc::Rc;
use std::time;

pub trait Renderer: std::marker::Sized {
    fn new<T>(
        win: &mut platform::backend::Window<T>,
        win_size: LogicalSize,
        hidpi_factor: f64,
        present_mode: PresentMode,
        resources: ResourceManager,
    ) -> std::io::Result<Self>;

    fn init(&mut self, effects: Vec<Effect>, views: &ViewManager);

    fn frame(
        &mut self,
        session: &Session,
        execution: Rc<RefCell<Execution>>,
        effects: Vec<session::Effect>,
        avg_frametime: &time::Duration,
    );

    fn update_present_mode(&mut self, present_mode: PresentMode);
}
