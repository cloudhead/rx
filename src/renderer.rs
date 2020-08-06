use crate::data::Assets;
use crate::execution::Execution;
use crate::platform::{self, LogicalSize};
use crate::session::{self, Effect, PresentMode, Session};

use std::time;

pub trait Renderer<'a>: std::marker::Sized {
    type Error;

    fn new(
        win: &mut platform::backend::Window,
        win_size: LogicalSize,
        scale_factor: f64,
        present_mode: PresentMode,
        assets: Assets<'a>,
    ) -> std::io::Result<Self>;

    fn init(&mut self, effects: Vec<Effect>, session: &Session);

    fn frame(
        &mut self,
        session: &mut Session,
        execution: &mut Execution,
        effects: Vec<session::Effect>,
        avg_frametime: &time::Duration,
    ) -> Result<(), Self::Error>;

    fn handle_present_mode_changed(&mut self, present_mode: PresentMode);
    fn handle_scale_factor_changed(&mut self, scale_factor: f64);
}
