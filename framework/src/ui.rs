pub mod canvas;
pub mod constraints;
pub mod context;
pub mod cursor;
pub mod env;
pub mod event;
#[cfg(test)]
pub mod tests;
pub mod text;
pub mod widgets;

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::ops::{ControlFlow, Deref, DerefMut};
use std::sync::atomic;
use std::time;

use crate::gfx::prelude::*;
use crate::renderer;

pub use canvas::*;
pub use context::*;
pub use cursor::{Cursor, CursorStyle};
pub use env::Env;
pub use event::{InputState, WidgetEvent, WindowEvent};
pub use renderer::{Blending, Paint, TextureId};
pub use widgets::align::Align;
pub use widgets::align::{align, bottom, center, left, right, top};
pub use widgets::click::Click;
pub use widgets::controller::Control;
pub use widgets::hover::Hover;
pub use widgets::hstack::hstack;
pub use widgets::painter::painter;
pub use widgets::zstack::zstack;
pub use widgets::Pod;
pub use widgets::Widget;

/// A widget lifecycle event.
#[derive(Debug, Copy, Clone)]
pub enum WidgetLifecycle<'a> {
    Initialized(&'a HashMap<TextureId, TextureInfo>),
}

pub struct Interactive<T> {
    cursor: CursorStyle,
    widget: Box<dyn Widget<T>>,
}

impl<T> Widget<T> for Interactive<T> {
    fn layout(&mut self, parent: Size, ctx: &LayoutCtx<'_>, data: &T, env: &Env) -> Size {
        self.widget.layout(parent, ctx, data, env)
    }

    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        self.widget.paint(canvas, data);
    }

    fn update(&mut self, delta: time::Duration, ctx: &Context<'_>, data: &T) {
        self.widget.update(delta, ctx, data);
    }

    fn event(&mut self, event: &WidgetEvent, ctx: &Context<'_>, data: &mut T) -> ControlFlow<()> {
        self.widget.event(event, ctx, data)
    }

    fn cursor(&self) -> Option<CursorStyle> {
        Some(self.cursor)
    }

    fn contains(&self, point: Point) -> bool {
        self.widget.contains(point)
    }

    fn display(&self) -> String {
        format!("Interactive({})", self.widget.display())
    }
}

pub trait WidgetExt<T>: Sized + Widget<T> + 'static {
    fn boxed(self) -> Box<dyn Widget<T> + 'static>;
    fn sized<S: Into<Size>>(self, size: S) -> widgets::SizedBox<T>;
}

impl<T, W: 'static> WidgetExt<T> for W
where
    W: Widget<T>,
{
    fn boxed(self) -> Box<dyn Widget<T> + 'static> {
        Box::new(self)
    }

    fn sized<S: Into<Size>>(self, size: S) -> widgets::SizedBox<T> {
        let size = size.into();
        widgets::SizedBox::new(self).width(size.w).height(size.h)
    }
}

pub trait Interact<T>: Sized + Widget<T> + 'static {
    fn cursor_style(self, cursor: CursorStyle) -> Interactive<T> {
        Interactive {
            widget: Box::new(self),
            cursor,
        }
    }

    fn on_click(self, action: impl Fn(&Context<'_>, &mut T) + 'static) -> Control<Self, Click<T>> {
        Control::new(self, Click::new(action))
    }

    fn on_hover(
        self,
        action: impl Fn(bool, &Context<'_>, &mut T) + 'static,
    ) -> Control<Self, Hover<T>> {
        Control::new(self, Hover::new(action))
    }
}

impl<T, W> Interact<T> for W where W: Widget<T> + 'static {}

pub type Surfaces = HashMap<TextureId, Image>;

#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub enum ExitReason {
    #[default]
    Normal,
    Error(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Id(u64);

impl Id {
    pub fn next() -> Self {
        static NEXT: atomic::AtomicU64 = atomic::AtomicU64::new(1);

        Self(NEXT.fetch_add(1, atomic::Ordering::SeqCst))
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Session state.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    /// The session is initializing.
    Initializing,
    /// The session is running normally.
    Running,
    /// The session is paused. Inputs are not processed.
    Paused,
    /// The session is being shut down.
    Closing(ExitReason),
}

#[derive(Debug, Clone)]
pub struct Shadow {
    pub offset: Vector,
    pub size: f32,
    pub fill: Fill,
}

pub enum Drag {
    Started { anchor: Point, origin: Point },
    Stopped,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct WidgetId(Id);

impl WidgetId {
    pub fn root() -> Self {
        WidgetId(Id(0))
    }

    pub fn next() -> Self {
        Self(Id::next())
    }
}

impl fmt::Display for WidgetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

#[derive(Default, Debug, Clone)]
pub struct Padding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Padding {
    pub fn all(padding: f32) -> Self {
        Self {
            top: padding,
            bottom: padding,
            right: padding,
            left: padding,
        }
    }

    pub fn bottom(mut self, bottom: f32) -> Self {
        self.bottom = bottom;
        self
    }
}

impl From<[f32; 4]> for Padding {
    fn from([top, right, bottom, left]: [f32; 4]) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

impl From<[f32; 2]> for Padding {
    fn from([vertical, horizontal]: [f32; 2]) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct Position {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl Position {
    pub fn top(mut self, top: f32) -> Self {
        self.top = Some(top);
        self
    }

    pub fn right(mut self, right: f32) -> Self {
        self.right = Some(right);
        self
    }

    pub fn bottom(mut self, bottom: f32) -> Self {
        self.bottom = Some(bottom);
        self
    }

    pub fn left(mut self, left: f32) -> Self {
        self.left = Some(left);
        self
    }
}
