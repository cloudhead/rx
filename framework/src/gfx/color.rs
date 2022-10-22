use std::fmt;
use std::io;
use std::mem;
use std::mem::ManuallyDrop;
use std::str::FromStr;
use std::sync::Arc;

use super::{Point2D, Size, Zero};

/// File extension for RGBA images.
pub const FILE_EXTENSION: &str = "rgba";

///////////////////////////////////////////////////////////////////////////
// Image
///////////////////////////////////////////////////////////////////////////

/// An RGBA image. Can be created from an `.rgba` file buffer using the `TryFrom`
/// instance.
///
/// .---------.
/// |  MAGIC  | "RGBA" (4 bytes)
/// +---------+
/// |  WIDTH  |        (4 bytes) (Big Endian)
/// +---------+
/// |  HEIGHT |        (4 bytes) (Big Endian)
/// +---------+
/// |  DATA   |        (WIDTH * HEIGHT * 4 bytes)
/// |  ....   |        (RGBA format)
/// |  ....   |
/// |  ....   |
/// '---------'
///
#[derive(Debug, Clone)]
pub struct Image {
    pub size: Size<u32>,
    pub pixels: Arc<[Rgba8]>,
}

impl Eq for Image {}

impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        (Arc::ptr_eq(&self.pixels, &other.pixels) && self.size == other.size)
            || self.pixels == other.pixels
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ImageError {
    #[error("invalid RGBA image encoding")]
    Decoding,
}

impl Image {
    pub fn new(pixels: impl Into<Arc<[Rgba8]>>, size: impl Into<Size<u32>>) -> Self {
        let size = size.into();
        let pixels = pixels.into();

        assert_eq!(size.area() as usize, pixels.len());

        Self { size, pixels }
    }

    /// An empty image.
    pub fn empty() -> Self {
        Self {
            size: Size::ZERO,
            pixels: Arc::new([]),
        }
    }

    /// Create a blank image of the given size.
    pub fn blank(size: impl Into<Size<u32>>) -> Self {
        let size = size.into();

        Self::new(vec![Rgba8::TRANSPARENT; size.area() as usize], size)
    }

    /// Write the image to the writer in `.rgba` format.
    pub fn write(&self, mut w: impl io::Write) -> Result<usize, io::Error> {
        let mut n = 0;
        let (head, texels, tail) = unsafe { self.pixels.align_to::<u8>() };

        assert!(head.is_empty() && tail.is_empty());

        n += w.write(&[b'R', b'G', b'B', b'A'])?;
        n += w.write(&u32::to_be_bytes(self.size.w))?;
        n += w.write(&u32::to_be_bytes(self.size.h))?;
        n += w.write(texels)?;

        w.flush()?;

        Ok(n)
    }

    /// Get the texel color at the given position in the image.
    pub fn sample(&self, point: Point2D<u32>) -> Option<&Rgba8> {
        let offset = self.size.w * point.y + point.x;
        self.pixels.get(offset as usize)
    }
}

impl TryFrom<&[u8]> for Image {
    type Error = ImageError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let tail = if let &[b'R', b'G', b'B', b'A', ref tail @ ..] = bytes {
            tail
        } else {
            return Err(ImageError::Decoding);
        };

        let (tail, width) = if let &[a, b, c, d, ref tail @ ..] = tail {
            (tail, u32::from_be_bytes([a, b, c, d]))
        } else {
            return Err(ImageError::Decoding);
        };

        let (tail, height) = if let &[a, b, c, d, ref tail @ ..] = tail {
            (tail, u32::from_be_bytes([a, b, c, d]))
        } else {
            return Err(ImageError::Decoding);
        };

        let (head, texels, tail) = unsafe { tail.align_to::<Rgba8>() };

        if !head.is_empty() {
            return Err(ImageError::Decoding);
        }
        if !tail.is_empty() {
            return Err(ImageError::Decoding);
        }
        assert_eq!(texels.len(), (width * height) as usize);

        Ok(Self::new(texels.to_vec(), Size::new(width, height)))
    }
}

///////////////////////////////////////////////////////////////////////////
// Rgba8
///////////////////////////////////////////////////////////////////////////

/// RGBA color with 8-bit channels.
#[repr(C)]
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Debug, Default)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    pub const WHITE: Self = Self {
        r: 0xff,
        g: 0xff,
        b: 0xff,
        a: 0xff,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0xff,
    };
    pub const RED: Self = Self {
        r: 0xff,
        g: 0,
        b: 0,
        a: 0xff,
    };
    pub const GREEN: Self = Self {
        r: 0,
        g: 0xff,
        b: 0,
        a: 0xff,
    };
    pub const BLUE: Self = Self {
        r: 0,
        g: 0,
        b: 0xff,
        a: 0xff,
    };
    pub const GREY: Self = Self {
        r: 0x7f,
        g: 0x7f,
        b: 0x7f,
        a: 0xff,
    };

    /// Create a new [`Rgba8`] color from individual channels.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Invert the color.
    pub fn invert(self) -> Self {
        Self::new(0xff - self.r, 0xff - self.g, 0xff - self.b, self.a)
    }

    /// Return the color with a changed alpha.
    ///
    /// ```
    /// use rx_framework::gfx::color::Rgba8;
    ///
    /// let c = Rgba8::WHITE;
    /// assert_eq!(c.alpha(0x88), Rgba8::new(c.r, c.g, c.b, 0x88))
    /// ```
    pub fn alpha(self, a: u8) -> Self {
        Self::new(self.r, self.g, self.b, a)
    }

    /// Given a byte slice, returns a slice of [`Rgba8`] values.
    pub fn align<'a, S: 'a, T: AsRef<[S]> + ?Sized>(bytes: &'a T) -> &'a [Rgba8] {
        let bytes = bytes.as_ref();
        let (head, body, tail) = unsafe { bytes.align_to::<Rgba8>() };

        if !(head.is_empty() && tail.is_empty()) {
            panic!("Rgba8::align: input is not a valid `Rgba8` buffer");
        }
        body
    }

    /// Given a color slice, returns a slice of `u8` values.
    pub fn bytes(bytes: &[Rgba8]) -> &[u8] {
        let (head, body, tail) = unsafe { bytes.align_to::<u8>() };

        assert!(head.is_empty());
        assert!(tail.is_empty());

        body
    }

    /// Given a byte vector, returns a vector of [`Rgba8`] values.
    pub fn into_vec(bytes: Vec<u8>) -> Vec<Self> {
        if bytes.len() % mem::size_of::<Rgba8>() != 0 {
            panic!("Rgba8::into_vec: input is not a valid `Rgba8` buffer");
        }
        assert_eq!(bytes.capacity() % 4, 0);

        let bytes = ManuallyDrop::new(bytes);
        let ptr = bytes.as_ptr();
        let length = bytes.len() / 4;
        let capacity = bytes.capacity() / 4;

        unsafe { Vec::from_raw_parts(ptr as *mut Rgba8, length, capacity) }
    }
}

impl Zero for Rgba8 {
    const ZERO: Self = Rgba8::TRANSPARENT;

    fn is_zero(&self) -> bool {
        self == &Self::ZERO
    }
}

/// ```
/// use rx_framework::gfx::color::Rgba8;
///
/// assert_eq!(format!("{}", Rgba8::new(0xff, 0x0, 0xa, 0xff)), "#ff000a");
/// ```
impl fmt::Display for Rgba8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 0xff {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

/// ```
/// use rx_framework::gfx::color::{Rgba8, Rgba};
///
/// assert_eq!(Rgba8::from(Rgba::RED), Rgba8::RED);
/// ```
impl From<Rgba> for Rgba8 {
    fn from(rgba: Rgba) -> Self {
        Self {
            r: (rgba.r * 255.0).round() as u8,
            g: (rgba.g * 255.0).round() as u8,
            b: (rgba.b * 255.0).round() as u8,
            a: (rgba.a * 255.0).round() as u8,
        }
    }
}

impl From<u32> for Rgba8 {
    fn from(rgba: u32) -> Self {
        unsafe { std::mem::transmute(rgba) }
    }
}

impl FromStr for Rgba8 {
    type Err = std::num::ParseIntError;

    /// Parse a color code of the form `#ffffff` into an
    /// instance of `Rgba8`. The alpha is always `0xff`.
    fn from_str(hex_code: &str) -> Result<Self, Self::Err> {
        let r: u8 = u8::from_str_radix(&hex_code[1..3], 16)?;
        let g: u8 = u8::from_str_radix(&hex_code[3..5], 16)?;
        let b: u8 = u8::from_str_radix(&hex_code[5..7], 16)?;
        let a: u8 = 0xff;

        Ok(Rgba8 { r, g, b, a })
    }
}

//////////////////////////////////////////////////////////////////////////////
// Rgb8
//////////////////////////////////////////////////////////////////////////////

/// An RGB 8-bit color. Used when the alpha value isn't used.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<Rgba8> for Rgb8 {
    fn from(rgba: Rgba8) -> Self {
        Self {
            r: rgba.r,
            g: rgba.g,
            b: rgba.b,
        }
    }
}

impl fmt::Display for Rgb8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

//////////////////////////////////////////////////////////////////////////////
// Rgba
//////////////////////////////////////////////////////////////////////////////

/// A normalized RGBA color.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const RED: Self = Rgba::new(1.0, 0.0, 0.0, 1.0);
    pub const GREEN: Self = Rgba::new(0.0, 1.0, 0.0, 1.0);
    pub const BLUE: Self = Rgba::new(0.0, 0.0, 1.0, 1.0);
    pub const WHITE: Self = Rgba::new(1.0, 1.0, 1.0, 1.0);
    pub const BLACK: Self = Rgba::new(0.0, 0.0, 0.0, 1.0);
    pub const TRANSPARENT: Self = Rgba::new(0.0, 0.0, 0.0, 0.0);

    /// Create a new `Rgba` color.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Invert the color.
    pub fn invert(self) -> Self {
        Self::new(1.0 - self.r, 1.0 - self.g, 1.0 - self.b, self.a)
    }
}

impl From<Rgba8> for Rgba {
    fn from(rgba8: Rgba8) -> Self {
        Self {
            r: (rgba8.r as f32 / 255.0),
            g: (rgba8.g as f32 / 255.0),
            b: (rgba8.b as f32 / 255.0),
            a: (rgba8.a as f32 / 255.0),
        }
    }
}

impl From<Rgba> for [f32; 4] {
    fn from(rgba: Rgba) -> Self {
        [rgba.r, rgba.g, rgba.b, rgba.a]
    }
}

/// Color types.
pub trait Color {
    const WHITE: Self;
    const BLACK: Self;
    const TRANSPARENT: Self;
    const GREY: Self;
    const DARK_GREY: Self;
    const LIGHT_GREY: Self;
    const RED: Self;
    const YELLOW: Self;
    const LIGHT_GREEN: Self;
    const GREEN: Self;
    const BLUE: Self;
}

impl Color for Rgba8 {
    const WHITE: Self = Self::new(0xff, 0xff, 0xff, 0xff);
    const BLACK: Self = Self::new(0x00, 0x00, 0x00, 0xff);
    const TRANSPARENT: Self = Self::new(0x00, 0x00, 0x00, 0x00);
    const GREY: Self = Self::new(0x88, 0x88, 0x88, 0xff);
    const DARK_GREY: Self = Self::new(0x55, 0x55, 0x55, 0xff);
    const LIGHT_GREY: Self = Self::new(0xaa, 0xaa, 0xaa, 0xff);
    const RED: Self = Self::new(0xff, 0x33, 0x66, 0xff);
    const YELLOW: Self = Self::new(0xff, 0xff, 0x66, 0xff);
    const LIGHT_GREEN: Self = Self::new(0xbb, 0xff, 0xee, 0xff);
    const GREEN: Self = Self::new(0x38, 0xb7, 0x55, 0xff);
    const BLUE: Self = Self::new(0x29, 0x36, 0x6f, 0xff);
}
