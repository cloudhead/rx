use crate::ui::widgets::Controller;
use crate::ui::*;

pub struct Hover<T> {
    /// A closure that will be invoked when the child widget is hovered.
    action: Box<dyn Fn(bool, &Context<'_>, &mut T)>,
}

impl<T> Hover<T> {
    /// Create a new hoverable [`Controller`] widget.
    pub fn new(action: impl Fn(bool, &Context<'_>, &mut T) + 'static) -> Self {
        Hover {
            action: Box::new(action),
        }
    }
}

impl<T, W: Widget<T>> Controller<T, W> for Hover<T> {
    fn event(
        &mut self,
        child: &mut W,
        event: &WidgetEvent,
        ctx: &Context<'_>,
        data: &mut T,
    ) -> ControlFlow<()> {
        match event {
            WidgetEvent::MouseEnter { .. } => {
                (self.action)(true, ctx, data);

                // Continue because we want events to be triggered for other widgets
                // in case the mouse has exited another widget.
                return ControlFlow::Continue(());
            }
            WidgetEvent::MouseExit { .. } => {
                (self.action)(false, ctx, data);

                // Continue because we want events to be triggered for other widgets
                // in case the mouse has entered another widget.
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        child.event(event, ctx, data)
    }
}
