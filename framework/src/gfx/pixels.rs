use nonempty::NonEmpty;

use crate::gfx::Rect;

/// `icn` format tile width and height.
pub const ICN_TILE: usize = 8;

/// A view into a pixel buffer.
pub struct Pixels<'a, T> {
    width: usize,
    height: usize,
    pixels: &'a [T],
}

impl<'a, T: Copy> Pixels<'a, T> {
    pub fn new(pixels: &'a [T], width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&T> {
        if x < self.width && y < self.height {
            self.pixels.get(self.width * y + x)
        } else {
            None
        }
    }

    pub fn rect(&self, rect: Rect<usize>) -> Vec<T> {
        let mut pixels = Vec::with_capacity(rect.area());

        for y in rect.origin.y..rect.origin.y + rect.size.h {
            for x in rect.origin.x..rect.origin.x + rect.size.w {
                if let Some(pixel) = self.get(x, y) {
                    pixels.push(*pixel);
                }
            }
        }
        pixels
    }
}

/// A mutable view into a pixel buffer.
pub struct PixelsMut<'a, T> {
    pub width: usize,
    pub height: usize,

    pixels: &'a mut [T],
}

impl<'a, T: Copy> PixelsMut<'a, T> {
    pub fn new(pixels: &'a mut [T], width: usize, height: usize) -> Self {
        assert_eq!(pixels.len(), width * height);

        Self {
            width,
            height,
            pixels,
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&T> {
        if x < self.width && y < self.height {
            self.pixels.get(x + y * self.width)
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut T> {
        if x < self.width && y < self.height {
            self.pixels.get_mut(x + y * self.width)
        } else {
            None
        }
    }

    pub fn set(&mut self, x: usize, y: usize, value: T) {
        if x < self.width && y < self.height {
            self.pixels[self.width * y + x] = value;
        }
    }

    pub fn iter_mut(&'a mut self) -> impl Iterator<Item = (usize, usize, &'a mut T)> {
        let width = self.width;

        self.pixels
            .as_mut()
            .iter_mut()
            .enumerate()
            .map(move |(i, c)| (i % width, i / width, c))
    }

    pub fn icn(&mut self, tile: [u8; ICN_TILE], x: usize, y: usize, color: T) {
        for v in 0..ICN_TILE {
            for h in 0..ICN_TILE {
                let value = (tile[v] >> (7 - h)) & 0x1;
                if value == 1 {
                    self.set(x + h, y + v, color);
                };
            }
        }
    }
}

/// Scale an image using the nearest-neighbor algorithm, by a given factor.
pub fn scale<T: Default + Clone + Copy>(
    image: &[T],
    width: u32,
    height: u32,
    factor: u32,
) -> Vec<T> {
    assert_eq!(image.len(), (width * height) as usize);

    let input = Pixels::new(image, width as usize, height as usize);

    let width = (width * factor) as usize;
    let height = (height * factor) as usize;

    let mut output_buf = vec![T::default(); width * height];
    let mut output = PixelsMut::new(&mut output_buf, width, height);

    for (x, y, pixel) in output.iter_mut() {
        let x = x / factor as usize;
        let y = y / factor as usize;

        *pixel = *input.get(x, y).unwrap();
    }
    output_buf
}

/// Stitch frames together so that they form a single contiguous strip.
pub fn stitch_frames<T: Clone>(
    frames: NonEmpty<Vec<T>>,
    fw: usize,
    fh: usize,
    default: T,
) -> Vec<T> {
    let nframes = frames.len();
    let width = fw * nframes;

    if frames.tail.is_empty() {
        return frames.head;
    }

    let mut buffer: Vec<T> = vec![default; fw * fh * nframes];

    for (i, frame) in frames.iter().enumerate() {
        for y in 0..fh {
            let offset = i * fw + y * width;
            buffer.splice(
                offset..offset + fw,
                frame[fw * y..fw * y + fw].iter().cloned(),
            );
        }
    }
    buffer
}
