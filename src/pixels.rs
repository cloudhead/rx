#![allow(dead_code)]
use rgx::color::Rgba8;

/// A view into a pixel buffer.
pub struct Pixels<'a> {
    width: usize,
    height: usize,
    pixels: &'a [Rgba8],
}

impl<'a> Pixels<'a> {
    pub fn new<T: AsRef<[Rgba8]> + ?Sized>(pixels: &'a T, width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: pixels.as_ref(),
        }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<&Rgba8> {
        self.pixels.as_ref().get(self.width * y + x)
    }
}

/// A mutable view into a pixel buffer.
pub struct PixelsMut<'a> {
    width: usize,
    height: usize,
    pixels: &'a mut [Rgba8],
}

impl<'a> PixelsMut<'a> {
    pub fn new<T: AsMut<[Rgba8]> + ?Sized>(pixels: &'a mut T, width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: pixels.as_mut(),
        }
    }

    pub fn get_mut(&mut self, x: usize, y: usize) -> Option<&mut Rgba8> {
        self.pixels.as_mut().get_mut(self.width * y + x)
    }

    pub fn set(&mut self, x: usize, y: usize, pixel: Rgba8) {
        if let Some(p) = self.pixels.as_mut().get_mut(x * y) {
            *p = pixel;
        }
    }

    pub fn iter_mut(&'a mut self) -> impl Iterator<Item = (usize, usize, &'a mut Rgba8)> {
        let width = self.width;

        self.pixels
            .as_mut()
            .iter_mut()
            .enumerate()
            .map(move |(i, c)| (i % width, i / width, c))
    }
}

/// Scale an image using the nearest-neighbor algorithm, by a given factor.
pub fn scale(image: &[Rgba8], width: u32, height: u32, factor: u32) -> Vec<Rgba8> {
    let input = Pixels::new(image, width as usize, height as usize);

    let width = (width * factor) as usize;
    let height = (height * factor) as usize;

    let mut output_buf = vec![Rgba8::TRANSPARENT; width * height];
    let mut output = PixelsMut::new(&mut output_buf, width, height);

    for (x, y, pixel) in output.iter_mut() {
        let x = x / factor as usize;
        let y = y / factor as usize;

        *pixel = *input.get(x, y).unwrap();
    }
    output_buf
}
