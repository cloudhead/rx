use rx::app::{DEFAULT_CURSORS, DEFAULT_FONT};
use rx::framework::gfx::prelude::*;
use rx::framework::ui::text::{FontFormat, FontId};
use rx::framework::ui::widgets::{Align, Painter, SizedBox, ZStack};
use rx::framework::ui::CursorStyle;
use rx::framework::ui::Interact;

fn main() -> anyhow::Result<()> {
    let cursors = Image::try_from(DEFAULT_CURSORS).unwrap();
    let ui = ZStack::new()
        .push(Align::new(
            SizedBox::new(Painter::new(|mut c, _| {
                c.stroke(Rect::origin(c.size), 1., Rgba8::BLUE);
            }))
            .width(256.)
            .height(256.)
            .cursor_style(CursorStyle::Crosshair),
        ))
        .push(Align::new(
            SizedBox::new(Painter::new(|mut c, _| {
                c.stroke(Rect::origin(c.size), 1., Rgba8::RED);
            }))
            .width(128.)
            .height(128.)
            .cursor_style(CursorStyle::Erase),
        ));

    rx::framework::logger::init(log::Level::Debug)?;
    rx::framework::Application::new("button")
        .fonts([(FontId::default(), DEFAULT_FONT, FontFormat::UF2)])?
        .cursors(cursors)
        .launch(ui, ())
        .map_err(Into::into)
}
