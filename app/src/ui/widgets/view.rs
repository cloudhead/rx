use std::ops::ControlFlow;
use std::time;

use rx_framework::platform::MouseButton;

use crate::app::brush;
use crate::app::bucket;
use crate::app::view::ViewId;
use crate::app::{Mode, Session, Tool};

use crate::framework::ui::text::Text;
use crate::framework::ui::*;
use crate::gfx::color::Image;
use crate::gfx::prelude::*;
use crate::gfx::shape2d;

pub struct View {
    pub id: ViewId,

    snapshot: usize,
    size: Size<u32>,
    zoom: f32,
    cursor: Point,

    draft: Vec<shape2d::Vertex>,
    paint: Vec<shape2d::Vertex>,
    blending: Blending,

    paint_texture: TextureId,
    draft_texture: TextureId,
}

impl View {
    pub fn new(id: ViewId) -> Self {
        Self {
            id,
            snapshot: 0,
            zoom: 1.,
            size: Size::default(),
            cursor: Point::default(),
            draft: Vec::default(),
            paint: Vec::default(),
            blending: Blending::default(),
            paint_texture: TextureId::next(),
            draft_texture: TextureId::next(),
        }
    }
}

impl Widget<Session> for View {
    fn layout(
        &mut self,
        _parent: Size,
        _ctx: &LayoutCtx<'_>,
        _session: &Session,
        _env: &Env,
    ) -> Size {
        self.size.into()
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, session: &Session) {
        // Setup view textures.
        let paint = canvas.offscreen(self.paint_texture, self.size, || {
            session.views.get(&self.id).map(|v| v.image()).unwrap()
        });
        let draft = canvas.offscreen(self.draft_texture, self.size, || Image::blank(self.size));

        if let Some(view) = session.views.get(&self.id) {
            if self.snapshot != view.snapshot {
                // Undo or redo was issued.
                canvas.on(paint).upload(view.pixels());

                self.snapshot = view.snapshot;
            }
        }

        // Render brush strokes on to view textures.
        canvas.on(draft).clear(Rgba8::TRANSPARENT);
        canvas
            .on(paint)
            .blending(self.blending)
            .paint(Paint::from(self.paint.drain(..)));
        canvas.on(draft).paint(Paint::from(self.draft.clone()));

        // Render view textures to the screen.
        canvas.paint(Paint::sprite(&self.paint_texture, &canvas));
        canvas.paint(Paint::sprite(&self.draft_texture, &canvas));

        // Parent transform without the scaling. This is to draw UI decorations
        // that don't scale when the viewport is zoomed.
        let unscaled = Transform::translate(canvas.transform.translation());
        let size: Size<f32> = self.size.into();

        canvas
            .with(unscaled)
            .stroke(Rect::origin(size) * session.zoom, 1., Rgba8::WHITE);
        canvas.with(unscaled).paint(
            Text::new(format!("{}x{}", size.w, size.h))
                .color(Rgba8::GREY)
                .font(session.settings["ui/font"].to_string().into())
                .offset(Vector::new(0., size.h * session.zoom + 1.)),
        );
    }

    fn update(&mut self, _delta: time::Duration, _ctx: &Context<'_>, session: &Session) {
        self.zoom = session.zoom;
        self.size = session
            .views
            .get(&self.id)
            .map(|v| v.size())
            .unwrap_or_default();
    }

    fn event(
        &mut self,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        let view = match session.views.active_mut() {
            Some(view) if view.id == self.id => view,
            _ => return ControlFlow::Continue(()),
        };

        match event {
            WidgetEvent::MouseMove(cursor) => {
                if ctx.hot {
                    session.colors.hover = view.sample(cursor.map(|n| n as u32));
                }
                let cursor = cursor.map(|c| c.floor());

                if self.cursor == cursor {
                    return ControlFlow::Continue(());
                }
                session.views.cursor = cursor;

                if session.mode == Mode::Normal {
                    if session.tool == Tool::Brush {
                        if let brush::State::Drawing { .. } = session.brush.state {
                            let brush = &mut session.brush;
                            let mut cursor: Point2D<i32> = cursor.into();

                            if brush.is_set(brush::Modifier::Multi) {
                                cursor.clamp(Rect::points(
                                    [(brush.size / 2) as i32, (brush.size / 2) as i32],
                                    [
                                        self.size.w as i32 - (brush.size / 2) as i32 - 1,
                                        self.size.h as i32 - (brush.size / 2) as i32 - 1,
                                    ],
                                ));
                            }
                            let output = brush
                                .extend_stroke(cursor)
                                .iter()
                                .flat_map(|r| r.vertices())
                                .collect();

                            if brush.mode == brush::Mode::Erase {
                                self.paint = output;
                            } else {
                                self.draft = output;
                            }
                        }
                    }
                }
                self.cursor = cursor;
            }
            // Click on this view.
            WidgetEvent::MouseDown(button) => {
                // ctx.active = true;

                // Clicking on a view is one way to get out of command mode.
                if session.mode == Mode::Command {
                    session.prev_mode();
                } else if session.views.is_active(&self.id) {
                    if let Some(view) = session.views.get_mut(&self.id) {
                        match session.mode {
                            Mode::Normal => match session.tool {
                                Tool::Brush => {
                                    let erase = session.brush.mode == brush::Mode::Erase;
                                    let color = if erase {
                                        Rgba8::TRANSPARENT
                                    } else if *button == MouseButton::Left {
                                        session.colors.fg
                                    } else if *button == MouseButton::Right {
                                        session.colors.bg
                                    } else {
                                        return ControlFlow::Continue(());
                                    };

                                    let output = session.brush.begin_stroke(
                                        ctx.cursor.into(),
                                        color,
                                        view.extent,
                                    );

                                    if erase {
                                        self.blending = Blending::Constant;
                                        self.paint =
                                            output.iter().flat_map(|r| r.vertices()).collect();
                                    } else {
                                        self.blending = Blending::default();
                                        self.draft =
                                            output.iter().flat_map(|r| r.vertices()).collect();
                                    }
                                }
                                Tool::Sampler => {}
                                Tool::Pan { .. } => {}
                                Tool::Bucket => {
                                    let output = bucket::fill(
                                        view,
                                        ctx.cursor.map(|n| n as usize),
                                        session.colors.fg,
                                    );
                                    self.paint.extend(output.iter().flat_map(|r| r.vertices()));

                                    view.edited();
                                }
                            },
                            Mode::Command => {}
                            Mode::Visual { .. } => {}
                            Mode::Help => {}
                        }
                    }
                } else {
                    session.views.activate(self.id);
                }
                return ControlFlow::Break(());
            }
            WidgetEvent::MouseUp(_button) => match session.mode {
                Mode::Normal => {
                    if let Tool::Brush = session.tool {
                        match session.brush.state {
                            brush::State::Drawing { .. } => {
                                let output = session.brush.end_stroke();

                                view.edited();

                                self.paint.extend(output.iter().flat_map(|r| r.vertices()));
                                self.draft.clear();
                            }
                            _ => {}
                        }
                    }
                    return ControlFlow::Break(());
                }
                _ => {}
            },
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn contains(&self, point: Point) -> bool {
        Rect::<f32>::origin(self.size.map(|n| n as f32) * self.zoom).contains(point)
    }

    fn frame(&mut self, surfaces: &Surfaces, session: &mut Session) {
        if let Some(view) = session.views.get_mut(&self.id) {
            if view.snapshot != view.snapshots.len() - 1 {
                return;
            }

            if let Some(image) = surfaces.get(&self.paint_texture) {
                if &view.image() != image {
                    self.snapshot = view.snapshot(image.pixels.clone(), view.extent);
                }
            }
        }
    }
}
