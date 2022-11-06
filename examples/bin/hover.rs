use std::ops::ControlFlow;

use rx::app::{DEFAULT_CURSORS, DEFAULT_FONT};
use rx::framework::gfx::prelude::*;
use rx::framework::ui::text::{FontFormat, FontId};

use rx::framework::ui::{center, hstack, Widget};
use rx::framework::ui::{WidgetEvent, WidgetExt};

struct Element(Rgba8, f32);

impl<T> Widget<T> for Element {
    fn paint(&mut self, mut canvas: rx::framework::ui::Canvas<'_>, _data: &T) {
        canvas.fill(canvas.bounds(), self.0);
        canvas.stroke(canvas.bounds(), self.1, Rgba8::WHITE);
    }

    fn display(&self) -> String {
        format!("Element({})", self.0)
    }

    fn event(
        &mut self,
        event: &rx::framework::ui::WidgetEvent,
        _ctx: &rx::framework::ui::Context<'_>,
        _data: &mut T,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::MouseEnter { .. } => {
                self.1 = 1.0;

                return ControlFlow::Break(());
            }
            WidgetEvent::MouseExit => {
                self.1 = 0.0;

                return ControlFlow::Break(());
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
struct State {
    clicks: u64,
    hot: bool,
}

fn main() -> anyhow::Result<()> {
    let cursors = Image::try_from(DEFAULT_CURSORS).unwrap();

    let items = vec![
        Element(Rgba8::RED, 0.).sized([32., 32.]).boxed(),
        Element(Rgba8::GREEN, 0.).sized([32., 32.]).boxed(),
        Element(Rgba8::BLUE, 0.).sized([32., 32.]).boxed(),
    ];
    let ui = center(hstack(items).spacing(16.));

    rx::framework::logger::init(log::Level::Debug)?;
    rx::framework::Application::new("hover")
        .fonts([(FontId::default(), DEFAULT_FONT, FontFormat::UF2)])?
        .cursors(cursors)
        .launch(ui, Rgba8::TRANSPARENT)
        .map_err(Into::into)
}
