use crate::session::SessionCoords;

use rgx::kit::Rgba8;

pub struct Palette {
    // TODO: Make this an `ArrayVec<[Rgba8; 256]>`.
    pub colors: Vec<Rgba8>,
    pub hover: Option<Rgba8>,
    pub cellsize: f32,
    pub x: f32,
    pub y: f32,
}

impl Palette {
    pub fn new(cellsize: f32) -> Self {
        Self {
            colors: Vec::with_capacity(256),
            hover: None,
            cellsize,
            x: 0.,
            y: 0.,
        }
    }

    pub fn add(&mut self, color: Rgba8) {
        if !self.colors.contains(&color) {
            self.colors.push(color);
        }
    }

    pub fn clear(&mut self) {
        self.colors.clear();
    }

    pub fn size(&self) -> usize {
        self.colors.len()
    }

    pub fn handle_cursor_moved(&mut self, p: SessionCoords) {
        let (x, y) = (p.x, p.y);
        let mut x = x as i32 - self.x as i32;
        let mut y = y as i32 - self.y as i32;
        let cellsize = self.cellsize as i32;
        let size = self.size() as i32;

        let width = if size > 16 { cellsize * 2 } else { cellsize };
        let height = i32::min(size, 16) * cellsize;

        if x >= width || y >= height || x < 0 || y < 0 {
            self.hover = None;
            return;
        }

        x /= cellsize;
        y /= cellsize;

        let index = y + x * 16;

        self.hover = if index < size {
            Some(self.colors[index as usize])
        } else {
            None
        };
    }
}
