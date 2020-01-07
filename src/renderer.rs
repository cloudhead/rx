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
        scale_factor: f64,
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

    fn handle_present_mode_changed(&mut self, present_mode: PresentMode);
    fn handle_scale_factor_changed(&mut self, scale_factor: f64);
}
