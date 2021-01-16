use crate::view::layer::LayerCoords;
use crate::view::{Snapshot, View, ViewResource};
use rgx::color::{Rgba8};
use rgx::kit::shape2d::{Fill, Rotation, Shape, Stroke};
use rgx::kit::ZDepth;
use rgx::rect::Rect;

pub fn flood_fill(
    view: &View<ViewResource>,
    starting_point: LayerCoords<u32>,
    replacement_color: Rgba8,
) -> Option<Vec<Shape>> {
    let (snapshot, pixels) = view.current_snapshot(view.active_layer_id)?;
    let mut canvas = pixels.clone().into_rgba8();
    let bounds = snapshot.extent.rect();
    let target_color = get_color(snapshot, &mut canvas, starting_point)?.clone();

    if target_color == replacement_color {
        return None;
    }

    let mut points = Vec::new();
    let mut queue = vec![starting_point];

    while let Some(point) = queue.pop() {
        if let Some(c) = get_color(snapshot, &mut canvas, point) {
            if *c == target_color {
                *c = replacement_color;
                points.push((point, replacement_color));
                queue.extend(neighbors(point, bounds));
            }
        }
    }

    Some(to_shapes(points))
}

fn get_color<'a>(
    snapshot: &Snapshot,
    canvas: &'a mut Vec<Rgba8>,
    point: LayerCoords<u32>,
) -> Option<&'a mut Rgba8> {
    let idx = snapshot.layer_coord_to_index(point)?;
    let color = canvas.get_mut(idx);
    color
}

fn to_shapes(points: Vec<(LayerCoords<u32>, Rgba8)>) -> Vec<Shape> {
    let mut rects = Vec::with_capacity(points.len());
    for (it, color) in points {
        let x = it.x as f32;
        let y = it.y as f32;
        rects.push(Shape::Rectangle(
            Rect::new(x, y, x + 1.0, y + 1.0),
            ZDepth::default(),
            Rotation::ZERO,
            Stroke::NONE,
            Fill::Solid(color.into()),
        ));
    }
    rects
}

fn neighbors(p: LayerCoords<u32>, bounds: Rect<u32>) -> Vec<LayerCoords<u32>> {
    let mut v = Vec::with_capacity(4);
    if p.x > bounds.x1 {
        v.push(LayerCoords::new(p.x - 1, p.y))
    }
    if p.x < bounds.x2 - 1 {
        v.push(LayerCoords::new(p.x + 1, p.y))
    }
    if p.y > bounds.y1 {
        v.push(LayerCoords::new(p.x, p.y - 1))
    }
    if p.y < bounds.y2 - 1 {
        v.push(LayerCoords::new(p.x, p.y + 1))
    }
    v
}
