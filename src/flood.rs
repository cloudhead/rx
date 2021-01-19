use crate::view::layer::LayerCoords;
use crate::view::{View, ViewResource};
use rgx::color::Rgba8;
use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::ZDepth;
use rgx::math::Point2;
use rgx::rect::Rect;

struct Grid {
    pixels: Vec<Rgba8>,
    pub width: usize,
    pub height: usize,
}

impl Grid {
    pub fn new(pixels: Vec<Rgba8>, width: usize, height: usize) -> Grid {
        Grid {
            pixels,
            width,
            height,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Rgba8> {
        if x < self.width && y < self.height {
            self.pixels.get(x + y * self.width)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Rgba8> {
        if x < self.width && y < self.height {
            self.pixels.get_mut(x + y * self.width)
        } else {
            None
        }
    }
}

pub struct FloodFiller {
    grid: Grid,
    replacement_color: Rgba8,
    target_color: Rgba8,
    rects: Vec<(Rect<f32>, Rgba8)>,
    stack: Vec<Point2<usize>>,
}

impl FloodFiller {
    pub fn new(
        view: &View<ViewResource>,
        starting_point: LayerCoords<f32>,
        replacement_color: Rgba8,
    ) -> Option<FloodFiller> {
        let (snapshot, pixels) = view.current_snapshot(view.active_layer_id)?;
        let bounds = snapshot.extent.rect();
        let grid = Grid::new(
            pixels.clone().into_rgba8(),
            bounds.width() as usize,
            bounds.height() as usize,
        );

        let starting_point = Point2::new(
            starting_point.x as usize,
            grid.height - starting_point.y as usize - 1,
        );

        let target_color = grid.get(starting_point.x, starting_point.y)?.clone();
        Some(FloodFiller {
            grid,
            target_color,
            replacement_color,
            rects: Vec::new(),
            stack: vec![starting_point],
        })
    }

    fn push_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: Rgba8) {
        self.rects.push((
            Rect::new(
                x as f32,
                (self.grid.height - y - 1) as f32,
                (x + w) as f32,
                (self.grid.height - y - 1 + h) as f32,
            ),
            color,
        ));
    }

    fn try_set_at(&mut self, x: usize, y: usize) -> bool {
        match self.grid.get_mut(x, y) {
            Some(c) => {
                if *c != self.target_color {
                    false
                } else {
                    *c = self.replacement_color;
                    true
                }
            }
            None => false,
        }
    }

    fn push_on_change(&mut self, x: usize, y: usize, edge: &mut bool) {
        if let Some(c) = self.grid.get(x, y) {
            if *c == self.target_color {
                if *edge {
                    // We're at an edge, we'll come back to this point in the next loop to start a
                    // new horizontal span.
                    self.stack.push(Point2::new(x, y));
                    *edge = false;
                }
            } else {
                *edge = true;
            }
        }
    }

    fn look_above_below(&mut self, x: usize, y: usize, up: &mut bool, down: &mut bool) {
        if y > 0 {
            self.push_on_change(x, y - 1, up);
        }

        if y < self.grid.height - 1 {
            self.push_on_change(x, y + 1, down);
        }
    }

    pub fn run(mut self) -> Option<Vec<Shape>> {
        // This algorithm fills horizontally from the starting point, looking for edges above and
        // below. An "edge" is a place where a solid pixel changes to a fillable one. "Solid" means
        // not equal to self.target_color. When we see one of these transitions, we push the next
        // point onto the stack and, later, we come back and repeat the horizontal scan from that
        // point.
        if self.target_color == self.replacement_color {
            return None;
        }

        while let Some(p) = self.stack.pop() {
            let mut min_x = p.x;
            let mut max_x = p.x;

            // Keep track of whether the pixels above/below us are transitioning from solid to
            // fillable. These will be true as long as we're "in" (above/below) a solid region, and
            // will become false when we are past it.
            let mut up_edge = true;
            let mut down_edge = true;

            // scan right
            for x in p.x..=self.grid.width {
                max_x = x;
                if !self.try_set_at(x, p.y) {
                    break;
                }
                self.look_above_below(x, p.y, &mut up_edge, &mut down_edge);
            }

            up_edge = p.y > 0 && self.grid.get(p.x, p.y - 1) != Some(&self.target_color);
            down_edge = p.y < self.grid.height - 1
                && self.grid.get(p.x, p.y + 1) != Some(&self.target_color);

            // scan left
            for x in (0..p.x).rev() {
                min_x = x;
                if !self.try_set_at(x, p.y) {
                    min_x += 1;
                    break;
                }
                self.look_above_below(x, p.y, &mut up_edge, &mut down_edge);
            }

            self.push_rect(min_x, p.y, max_x - min_x, 1, self.replacement_color);
        }

        Some(to_shapes(self.rects))
    }
}

fn to_shapes(input: Vec<(Rect<f32>, Rgba8)>) -> Vec<Shape> {
    let mut rects = Vec::with_capacity(input.len());
    for (rect, color) in input {
        rects.push(Shape::Rectangle(
            rect,
            ZDepth::default(),
            Rotation::ZERO,
            Stroke::NONE,
            Fill::solid(color),
        ));
    }
    rects
}
