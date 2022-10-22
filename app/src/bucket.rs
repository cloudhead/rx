//! This algorithm fills horizontally from the starting point, looking for edges above and
//! below. An "edge" is a place where a solid pixel changes to a fillable one. "Solid" means
//! not equal to the old color. When we see one of these transitions, we push the next
//! point onto the stack and, later, we come back and repeat the horizontal scan from that
//! point.
use crate::gfx::pixels::PixelsMut;
use crate::gfx::prelude::*;
use crate::view::View;

struct Bucket<'a> {
    old_color: Rgba8,
    new_color: Rgba8,
    stack: Vec<Point2D<usize>>,
    pixels: PixelsMut<'a, Rgba8>,
}

impl<'a> Bucket<'a> {
    fn try_set_color(&mut self, x: usize, y: usize) -> bool {
        if let Some(c) = self.pixels.get_mut(x, y) {
            if *c == self.old_color {
                *c = self.new_color;
                return true;
            }
        }
        false
    }

    fn push_on_change(&mut self, x: usize, y: usize, edge: &mut bool) {
        if let Some(c) = self.pixels.get(x, y) {
            if *c == self.old_color {
                if *edge {
                    // We're at an edge, we'll come back to this point in the next loop to start a
                    // new horizontal span.
                    self.stack.push(Point2D::new(x, y));
                    *edge = false;

                    return;
                }
            }
        }
        *edge = true;
    }

    fn look_around(&mut self, x: usize, y: usize, up: &mut bool, down: &mut bool) {
        if y > 0 {
            self.push_on_change(x, y - 1, up);
        }
        if y < self.pixels.height - 1 {
            self.push_on_change(x, y + 1, down);
        }
    }
}

pub fn fill(view: &View, origin: Point2D<usize>, color: Rgba8) -> Vec<Rectangle> {
    let mut pixels = view.image.pixels.to_vec();
    let bounds = view.extent.rect();
    let pixels = PixelsMut::new(
        &mut pixels,
        bounds.width() as usize,
        bounds.height() as usize,
    );
    let old_color = if let Some(c) = pixels.get(origin.x, origin.y) {
        *c
    } else {
        return vec![];
    };

    if old_color == color {
        return vec![];
    }

    let mut bucket = Bucket {
        old_color,
        new_color: color,
        stack: vec![origin],
        pixels,
    };
    let mut rects = Vec::new();

    while let Some(Point2D { x, y }) = bucket.stack.pop() {
        let mut left = x;
        let mut right = x;

        // Keep track of whether the pixels above/below us are transitioning from solid to
        // fillable. These will be true as long as we're "in" (above/below) a solid region, and
        // will become false when we are past it.
        let mut up_edge = true;
        let mut down_edge = true;

        // Scan right.
        for x in x..=bucket.pixels.width {
            right = x;

            if bucket.try_set_color(x, y) {
                bucket.look_around(x, y, &mut up_edge, &mut down_edge);
            } else {
                break;
            }
        }

        // Scan left.
        for x in (0..x).rev() {
            left = x;

            if bucket.try_set_color(x, y) {
                bucket.look_around(x, y, &mut up_edge, &mut down_edge);
            } else {
                left += 1;
                break;
            }
        }

        rects.push(
            Rectangle::from(Rect::new(
                [left as f32, y as f32],
                [(right - left) as f32, 1 as f32],
            ))
            .fill(bucket.new_color),
        );
    }

    rects
}
