use std::collections::HashMap;

use std::ops::ControlFlow;
use std::time;

use crate::app::ui::View;
use crate::app::{Session, Tool};
use crate::framework::platform::{InputState, MouseButton};
use crate::framework::ui::canvas::Canvas;
use crate::framework::ui::*;
use crate::gfx::prelude::*;

use crate::view::ViewId;

/// Session viewport. Contains views.
pub struct Viewport {
    views: HashMap<ViewId, Pod<Session, Align<Session>>>,
    added: Vec<ViewId>,
    zoom: f32,
    region: Rect<f32>,
    cursor: Point2D<f32>,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            views: HashMap::new(),
            added: Vec::new(),
            zoom: 1.,
            region: Rect::ZERO,
            cursor: Point::ORIGIN,
        }
    }
}

impl Viewport {
    pub fn offset(&self) -> Offset {
        self.region.origin.into()
    }

    pub fn transform(&self) -> Transform {
        Transform::scale(self.zoom) * Transform::translate(self.offset())
    }

    /// Pan the viewport by a relative amount.
    fn pan(&mut self, offset: Offset) {
        self.region.origin = self.region.origin + offset;
    }

    /// Set the viewport zoom. Takes a center to zoom to.
    fn zoom(&mut self, z: f32, center: Point) {
        let zdiff = z / self.zoom;
        let offset = center - self.region.origin;
        let origin = center - offset * zdiff;

        self.region.origin = origin;
        self.zoom = z;
    }

    ///////////////////////////////////////////////////////////////////////////
    /// Event handlers
    ///////////////////////////////////////////////////////////////////////////

    fn handle_cursor_moved(&mut self, cursor: Point, session: &mut Session) {
        if self.cursor == cursor {
            return;
        }
        let offset = cursor - self.cursor;

        match session.tool {
            Tool::Pan { panning: true } => {
                self.pan(offset);
            }
            _ => {}
        }
        self.cursor = cursor;
    }

    fn handle_resize(&mut self, _size: Size) {}

    fn handle_mouse_input(
        &mut self,
        button: MouseButton,
        state: InputState,
        session: &mut Session,
    ) {
        if button != MouseButton::Left {
            return;
        }

        match &mut session.tool {
            Tool::Pan { ref mut panning } if !*panning && state.is_pressed() => {
                *panning = true;
            }
            Tool::Pan { ref mut panning } if *panning && state.is_released() => {
                *panning = false;
            }
            _ => {}
        }
    }
}

impl Widget<Session> for Viewport {
    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, session: &Session) {
        // Zoom changed.
        if session.zoom != self.zoom {
            self.zoom(session.zoom, ctx.cursor);
        }
        let ctx = ctx.transform(self.transform());

        // Check session views for views to be added.
        for view in session.views.iter() {
            if !self.views.keys().any(|id| *id == view.id) {
                self.added.push(view.id);
                self.views
                    .insert(view.id, Pod::new(Align::new(View::new(view.id))));
            }
        }
        // Check our views for views to be removed that aren't in the session.
        self.views
            .retain(|view_id, _view| session.views.ids().any(|id| id == view_id));

        for (_, widget) in &mut self.views {
            widget.update(delta, &ctx, session);
        }
    }

    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, session: &Session, env: &Env) -> Size {
        self.region.size = parent;

        for (_, widget) in &mut self.views {
            widget.layout(parent, ctx, session, env);
        }
        parent
    }

    fn paint(&mut self, mut canvas: Canvas<'_>, session: &Session) {
        let transform = self.transform();

        for (_, widget) in &mut self.views {
            widget.paint(canvas.transform(transform), session);
        }
    }

    fn event(
        &mut self,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        session: &mut Session,
    ) -> ControlFlow<()> {
        let transform = self.transform();
        let mut inner = ctx.transform(transform);

        match event {
            WidgetEvent::Resized(size) => {
                self.handle_resize(*size);
            }
            WidgetEvent::MouseDown(input) => {
                self.handle_mouse_input(*input, InputState::Pressed, session);
            }
            WidgetEvent::MouseUp(input) => {
                self.handle_mouse_input(*input, InputState::Released, session);
            }
            WidgetEvent::MouseMove(point) => {
                self.handle_cursor_moved(*point, session);
            }
            _ => {}
        }

        for (_, view) in &mut self.views {
            let flow = match event {
                WidgetEvent::MouseMove(point) => {
                    let cursor = point.untransform(transform);
                    view.event(&WidgetEvent::MouseMove(cursor), &mut inner, session)
                }
                _ => view.event(event, &mut inner, session),
            };
            // ctx.active |= inner.active;

            if let ControlFlow::Break(_) = flow {
                return flow;
            }
        }
        ControlFlow::Continue(())
    }

    fn contains(&self, _: Point) -> bool {
        // The viewport is always hot, since it is the
        // background layer of the session.
        true
    }

    fn lifecycle(
        &mut self,
        lifecycle: &WidgetLifecycle<'_>,
        ctx: &Context<'_>,
        session: &Session,
        env: &Env,
    ) {
        for (_, view) in &mut self.views {
            view.lifecycle(lifecycle, ctx, session, env);
        }
    }

    fn frame(&mut self, surfaces: &Surfaces, session: &mut Session) {
        for (_, view) in &mut self.views {
            view.frame(surfaces, session);
        }
    }
}

impl Viewport {
    pub fn view(mut self, view: View) -> Self {
        self.views.insert(view.id, Pod::new(Align::new(view)));
        self
    }
}
