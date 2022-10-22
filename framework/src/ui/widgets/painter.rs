use crate::gfx::Rgba8;
use crate::ui::{Canvas, Widget};

pub struct Painter<T> {
    paint: Box<dyn FnMut(Canvas<'_>, &T)>,
}

impl<T> Painter<T> {
    /// Create a new [`Painter`] with the provided paint function.
    pub fn new(paint: impl FnMut(Canvas<'_>, &T) + 'static) -> Self {
        Painter {
            paint: Box::new(paint),
        }
    }
}

impl<T> Widget<T> for Painter<T> {
    fn paint(&mut self, canvas: Canvas<'_>, data: &T) {
        (self.paint)(canvas, data)
    }

    fn display(&self) -> String {
        String::from("Painter")
    }
}

impl<T> From<Rgba8> for Painter<T> {
    fn from(color: Rgba8) -> Self {
        Self::new(move |mut canvas, _| {
            canvas.fill(canvas.bounds(), color);
        })
    }
}

pub fn painter<T>(paint: impl FnMut(Canvas<'_>, &T) + 'static) -> Painter<T> {
    Painter::new(paint)
}
