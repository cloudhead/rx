use crate::session::SessionCoords;

use rgx::kit::Rgba8;

pub struct Palette {
    // TODO: Make this an `ArrayVec<[Rgba8; 256]>`.
    pub colors: Vec<Rgba8>,
    pub hover: Option<Rgba8>,
    pub cellsize: f32,
    pub height: usize,
    pub x: f32,
    pub y: f32,
}

impl Palette {
    pub fn new(cellsize: f32, height: usize) -> Self {
        Self {
            colors: Vec::with_capacity(256),
            hover: None,
            cellsize,
            height,
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
        let height = self.height as i32;
        let columns = (self.size() as f32 / self.height as f32).ceil() as i32;

        let width = if size > height {
            cellsize * columns
        } else {
            cellsize
        };
        let height = i32::min(size, height) * cellsize;

        if x >= width || y >= height || x < 0 || y < 0 {
            self.hover = None;
            return;
        }

        x /= cellsize;
        y /= cellsize;

        let index = y + x * (height / cellsize);

        self.hover = if index < size {
            // We index from the back because the palette is reversed
            // before it is displayed, due to the Y axis pointing up,
            // where as the palette is created starting at the top
            // and going down.
            Some(self.colors[self.size() - index as usize - 1])
        } else {
            None
        };
    }
}
