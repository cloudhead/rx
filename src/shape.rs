use crate::brush::Brush;
use crate::view::layer::LayerCoords;
use crate::view::ViewExtent;
use rgx::color::Rgba8;
use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::ZDepth;
use rgx::rect::Rect;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum LineState {
    /// Not currently drawing.
    NotDrawing,
    /// Drawing has just started.
    DrawStarted(ViewExtent),
    /// Drawing.
    Drawing(ViewExtent),
    /// Drawing has just ended.
    DrawEnded(ViewExtent),
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct LineTool {
    pub state: LineState,
    pub start_point: LayerCoords<i32>,
    pub end_point: LayerCoords<i32>,
    pub color: Rgba8,
}

impl LineTool {
    pub fn default() -> Self {
        LineTool {
            state: LineState::NotDrawing,
            start_point: LayerCoords::new(0, 0),
            end_point: LayerCoords::new(0, 0),
            color: Rgba8::WHITE,
        }
    }

    pub fn start_drawing(&mut self, p: LayerCoords<i32>, color: Rgba8, extent: ViewExtent) {
        self.state = LineState::DrawStarted(extent);
        self.color = color;
        self.start_point = p;
        self.end_point = p;
        self.draw(p);
    }

    pub fn update(&mut self) {
        if let LineState::DrawEnded(_) = self.state {
            self.state = LineState::NotDrawing;
        }
    }

    pub fn draw(&mut self, p: LayerCoords<i32>) {
        self.end_point = p;
        match self.state {
            LineState::Drawing(_) => {}
            LineState::DrawStarted(extent) => {
                self.state = LineState::Drawing(extent);
            }
            _ => unreachable!(),
        }
    }

    pub fn stop_drawing(&mut self) {
        match self.state {
            LineState::DrawStarted(ex) | LineState::Drawing(ex) => {
                self.state = LineState::DrawEnded(ex);
            }
            _ => unreachable!(),
        }
    }

    pub fn output(&self, stroke: Stroke, fill: Fill) -> Vec<Shape> {
        match self.state {
            LineState::DrawStarted(extent)
            | LineState::Drawing(extent)
            | LineState::DrawEnded(extent) => {
                let mut pixels = vec![];
                Brush::line(
                    self.start_point.map(|x| x.into()),
                    self.end_point.map(|x| x.into()),
                    &mut pixels,
                );

                pixels
                    .iter()
                    .map(|p| {
                        Shape::Rectangle(
                            Rect::new(p.x as f32, p.y as f32, (p.x + 1) as f32, (p.y + 1) as f32),
                            ZDepth::ZERO,
                            Rotation::ZERO,
                            stroke,
                            fill,
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}
