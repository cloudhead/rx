use crate::platform::MouseButton;
use crate::ui::widgets::Controller;
use crate::ui::*;

pub struct Click<T> {
    /// A closure that will be invoked when the child widget is clicked.
    action: Box<dyn Fn(&Context<'_>, &mut T)>,
}

impl<T> Click<T> {
    /// Create a new clickable [`Controller`] widget.
    pub fn new(action: impl Fn(&Context<'_>, &mut T) + 'static) -> Self {
        Click {
            action: Box::new(action),
        }
    }
}

impl<T, W: Widget<T>> Controller<T, W> for Click<T> {
    fn event(
        &mut self,
        child: &mut W,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        data: &mut T,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::MouseDown(MouseButton::Left) => {
                return ControlFlow::Break(());
            }
            WidgetEvent::MouseUp(MouseButton::Left) => {
                if ctx.active && ctx.hot {
                    (self.action)(ctx, data);
                }
                return ControlFlow::Break(());
            }
            _ => {}
        }
        child.event(event, ctx, data)
    }
}
