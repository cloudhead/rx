pub mod theme;
pub mod widgets;

use crate::app::ui::widgets::Root;
use crate::app::view::ViewId;
use crate::app::Session;
use crate::framework::ui;
use crate::framework::ui::text::{Text, TextAlign};
use crate::framework::ui::widgets::{Button, HStack, Image, Painter, SizedBox};
use crate::framework::ui::Paint;
use crate::gfx::prelude::{rectangle, Offset, Rgb8, Rgba8, Size};
use crate::gfx::sprite2d::Sprite;

use widgets::{CommandLine, Palette, View, Viewport};

/// Initial root widget state.
pub fn root(view: ViewId) -> Root {
    Root::default()
        .child(Viewport::default().view(View::new(view)))
        .child(ui::top(toolbar()))
        .child(ui::left(Palette::new(Size::from(12.))))
        .child(ui::align(status_bar()).bottom(16.).left(0.))
        .child(ui::align(CommandLine::default()).bottom(0.).left(0.))
}

pub struct Icon {
    image: Image,
    hot: bool,
}

impl Icon {
    pub fn new(name: &'static str) -> Self {
        Self {
            image: Image::named(name),
            hot: false,
        }
    }
}

impl<T> crate::framework::Widget<T> for Icon {
    fn paint(&mut self, mut canvas: ui::Canvas<'_>, _data: &T) {
        use crate::gfx::Rect;
        if let Image::ById(id, info) = self.image {
            if self.hot {
                canvas.paint(Paint::sprite(
                    &id,
                    Sprite::new(Rect::origin(info.size), Rect::origin(info.size))
                        .color(Rgba8::TRANSPARENT),
                    &canvas,
                ));
            } else {
                canvas.paint(Paint::sprite(
                    &id,
                    Sprite::new(Rect::origin(info.size), Rect::origin(info.size))
                        .color(Rgba8::GREY),
                    &canvas,
                ));
            }
        }
    }

    fn lifecycle(
        &mut self,
        lifecycle: &ui::WidgetLifecycle<'_>,
        ctx: &ui::Context<'_>,
        data: &T,
        env: &ui::Env,
    ) {
        self.image.lifecycle(lifecycle, ctx, data, env)
    }

    fn event(
        &mut self,
        event: &ui::WidgetEvent,
        ctx: &ui::Context<'_>,
        data: &mut T,
    ) -> std::ops::ControlFlow<()> {
        self.hot = ctx.hot;
        self.image.event(event, ctx, data)
    }

    fn layout(&mut self, parent: Size, ctx: &ui::LayoutCtx<'_>, data: &T, env: &ui::Env) -> Size {
        self.image.layout(parent, ctx, data, env)
    }
}

/// Session toolbar.
pub fn toolbar() -> impl ui::Widget<Session> {
    use crate::app::session::Tool;
    use crate::brush;

    HStack::default()
        .spacing(16.)
        .child(Button::new(
            Icon::new("pencil"),
            |_, session: &mut Session| {
                session.tool(Tool::Brush);
                session.brush.mode(brush::Mode::Pencil);
            },
        ))
        .child(Button::new(
            Icon::new("brush"),
            |_, session: &mut Session| {
                session.tool(Tool::Brush);
                session.brush.mode(brush::Mode::Normal);
            },
        ))
        .child(Button::new(
            Icon::new("eraser"),
            |_, session: &mut Session| {
                session.tool(Tool::Brush);
                session.brush.mode(brush::Mode::Erase);
            },
        ))
        .child(Button::new(
            Icon::new("bucket"),
            |_, session: &mut Session| {
                session.tool(Tool::Bucket);
            },
        ))
}

/// Session status bar. Displays general information about the session.
pub fn status_bar() -> impl ui::Widget<Session> {
    SizedBox::new(Painter::new(|mut canvas, session: &Session| {
        if let Some(view) = session.views.active() {
            let status = view.status();
            let zoom = format!("{:>5}%", (session.zoom * 100.) as u32);

            if let Some(cursor) = session.views.cursor {
                canvas.paint(
                    Text::new(format!("{:>5},{:<5}", cursor.x.floor(), cursor.y.floor(),))
                        .font(session.settings.font())
                        .offset(Offset::new(canvas.size.w / 2., 0.)),
                );
            }

            // TODO: Use `Label` widget and "right-align".
            canvas.paint(Text::new(status).font(session.settings.font()));
            canvas.paint(
                Text::new(zoom)
                    .align(TextAlign::Right)
                    .font(session.settings.font())
                    .offset(Offset::new(canvas.size.w, 0.)),
            );
        }

        if let Some(color) = session.colors.hover {
            let hex = Rgb8::from(color).to_string();

            canvas.paint(
                Text::new(format!("{}", hex))
                    .font(session.settings.font())
                    .offset(Offset::new(canvas.size.w / 2. + 128., 0.)),
            );
        }

        canvas.paint(
            rectangle([96., 3.], [8., 8.])
                .stroke(1., Rgba8::WHITE)
                .fill(session.colors.fg),
        );
        canvas.paint(
            rectangle([112., 3.], [8., 8.])
                .stroke(1., Rgba8::WHITE)
                .fill(session.colors.bg),
        );
    }))
    .height(16.)
    .width(f32::INFINITY)
}
