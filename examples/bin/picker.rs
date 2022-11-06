use std::str::FromStr;

use rx::app::{DEFAULT_CURSORS, DEFAULT_FONT};
use rx::framework::gfx::prelude::*;
use rx::framework::ui::text::{FontFormat, FontId};

use rx::framework::ui::{center, hstack, painter, zstack, CursorStyle};
use rx::framework::ui::{Interact, WidgetExt};

fn main() -> anyhow::Result<()> {
    let cursors = Image::try_from(DEFAULT_CURSORS).unwrap();
    let palette = [
        "#1a1c2c", "#5d275d", "#b13e53", "#ef7d57", "#ffcd75", "#a7f070", "#38b764", "#257179",
        "#29366f", "#3b5dc9", "#41a6f6", "#73eff7", "#f4f4f4", "#94b0c2", "#566c86", "#333c57",
    ];
    let swatches = palette
        .into_iter()
        .map(|s| Rgba8::from_str(s).unwrap())
        .map(|color| {
            color
                .sized((16., 16.))
                .on_click(move |_, state| *state = color)
                .cursor_style(CursorStyle::Pointer)
                .boxed()
        })
        .collect::<Vec<_>>();

    let ui = zstack((
        painter(|mut canvas, color| canvas.fill(canvas.bounds(), *color))
            .cursor_style(CursorStyle::Hand),
        center(hstack(swatches)),
    ));

    rx::framework::logger::init(log::Level::Debug)?;
    rx::framework::Application::new("picker")
        .fonts([(FontId::default(), DEFAULT_FONT, FontFormat::UF2)])?
        .cursors(cursors)
        .launch(ui, Rgba8::TRANSPARENT)
        .map_err(Into::into)
}
