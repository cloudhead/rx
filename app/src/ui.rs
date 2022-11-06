pub mod theme;
pub mod widgets;

use crate::app::ui::widgets::Root;
use crate::app::view::ViewId;
use crate::app::Session;
use crate::framework::ui;
use crate::framework::ui::text::{Text, TextAlign};
use crate::framework::ui::widgets::{Button, HStack, Image, Painter, SizedBox};
use crate::gfx::prelude::{Offset, Rectangle, Rgb8, Rgba8, Size};

use widgets::{CommandLine, Palette, View, Viewport};

/// Initial root widget state.
pub fn root(view: ViewId) -> Root {
    Root::default()
        .child(Viewport::default().view(View::new(view)))
        .child(ui::top(toolbar()))
        .child(ui::left(Palette::new(Size::from(12.))))
        .child(ui::Align::new(status_bar()).position(ui::Position::default().bottom(16.).left(0.)))
        .child(
            ui::Align::new(CommandLine::default())
                .position(ui::Position::default().bottom(0.).left(0.)),
        )
}

/// Session toolbar.
pub fn toolbar() -> impl ui::Widget<Session> {
    use crate::app::session::Tool;
    use crate::brush;

    HStack::default()
        .spacing(16.)
        .child(Button::new(
            Image::named("pencil"),
            |_, session: &mut Session| {
                session.tool(Tool::Brush);
                session.brush.mode(brush::Mode::Pencil);
            },
        ))
        .child(Button::new(Text::new("B"), |_, session: &mut Session| {
            session.tool(Tool::Brush);
            session.brush.mode(brush::Mode::Normal);
        }))
        .child(Button::new(Text::new("E"), |_, session: &mut Session| {
            session.tool(Tool::Brush);
            session.brush.mode(brush::Mode::Erase);
        }))
        .child(Button::new(Text::new("G"), |_, session: &mut Session| {
            session.tool(Tool::Bucket);
        }))
}

/// Session status bar. Displays general information about the session.
pub fn status_bar() -> impl ui::Widget<Session> {
    SizedBox::new(Painter::new(|mut canvas, session: &Session| {
        if let Some(view) = session.views.active() {
            let status = view.status();
            let zoom = format!("{:>5}%", (session.zoom * 100.) as u32);
            let cursor = session.views.cursor;
            let hover_color = session
                .colors
                .hover
                .map_or(String::new(), |c| Rgb8::from(c).to_string());

            canvas.paint(
                Text::new(format!(
                    "{:>5},{:<5} {}",
                    cursor.x.floor(),
                    cursor.y.floor(),
                    hover_color
                ))
                .font(session.settings["ui/font"].to_string().into())
                .offset(Offset::new(canvas.size.w / 2., 0.)),
            );

            // TODO: Use `Label` widget and "right-align".
            canvas.paint(Text::new(status).font(session.settings["ui/font"].to_string().into()));
            canvas.paint(
                Text::new(zoom)
                    .align(TextAlign::Right)
                    .font(session.settings["ui/font"].to_string().into())
                    .offset(Offset::new(canvas.size.w, 0.)),
            );
        }

        canvas.paint(
            Rectangle::new([96., 3.], [8., 8.])
                .stroke(1., Rgba8::WHITE)
                .fill(session.colors.fg),
        );
        canvas.paint(
            Rectangle::new([112., 3.], [8., 8.])
                .stroke(1., Rgba8::WHITE)
                .fill(session.colors.bg),
        );
    }))
    .height(16.)
    .width(f32::INFINITY)
}
