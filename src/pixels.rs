/// A view into a pixel buffer.
pub struct Pixels<'a, T> {
    width: usize,
    #[allow(dead_code)]
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
        self.pixels.get(self.width * y + x)
    }
}

/// A mutable view into a pixel buffer.
pub struct PixelsMut<'a, T> {
    width: usize,
    #[allow(dead_code)]
    height: usize,
    pixels: &'a mut [T],
}

impl<'a, T> PixelsMut<'a, T> {
    pub fn new(pixels: &'a mut [T], width: usize, height: usize) -> Self {
        assert_eq!(pixels.len(), width * height);

        Self {
            width,
            height,
            pixels,
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
