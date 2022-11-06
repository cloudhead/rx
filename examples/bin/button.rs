use rx::app::{DEFAULT_CURSORS, DEFAULT_FONT};
use rx::framework::gfx::prelude::*;
use rx::framework::ui::text::{FontFormat, FontId, Text};
use rx::framework::ui::widgets::{Align, Painter, SizedBox, ZStack};
use rx::framework::ui::Interact;

#[derive(Default)]
struct Data {
    red: (u64, bool),
    blue: (u64, bool),
    green: (u64, bool),
}

fn main() -> anyhow::Result<()> {
    let cursors = Image::try_from(DEFAULT_CURSORS).unwrap();
    let ui = ZStack::new()
        .push(Align::new(
            SizedBox::new(Painter::new(|mut c, data: &Data| {
                c.fill(Rect::origin(c.size), Rgba8::BLUE.alpha(0x55));
                c.paint(Text::new(format!("{} ({})", data.blue.0, data.blue.1)));
            }))
            .width(256.)
            .height(256.)
            .on_click(|_, data| {
                data.blue.0 += 1;
            })
            .on_hover(|hovered, _, data| {
                data.blue.1 = hovered;
            }),
        ))
        .push(Align::new(
            SizedBox::new(Painter::new(|mut c, data: &Data| {
                c.fill(Rect::origin(c.size), Rgba8::RED.alpha(0x55));
                c.paint(Text::new(format!("{} ({})", data.red.0, data.red.1)));
            }))
            .width(192.)
            .height(192.)
            .on_click(|_, data| {
                data.red.0 += 1;
            })
            .on_hover(|hovered, _, data| {
                data.red.1 = hovered;
            }),
        ))
        .push(Align::new(
            SizedBox::new(Painter::new(|mut c, data: &Data| {
                c.fill(Rect::origin(c.size), Rgba8::GREEN.alpha(0x55));
                c.paint(Text::new(format!("{} ({})", data.green.0, data.green.1)));
            }))
            .width(128.)
            .height(128.)
            .on_click(|_, data| {
                data.green.0 += 1;
            })
            .on_hover(|hovered, _, data| {
                data.green.1 = hovered;
            }),
        ));

    rx::framework::logger::init(log::Level::Debug)?;
    rx::framework::Application::new("button")
        .fonts([(FontId::default(), DEFAULT_FONT, FontFormat::UF2)])?
        .cursors(cursors)
        .launch(ui, Data::default())
        .map_err(Into::into)
}
