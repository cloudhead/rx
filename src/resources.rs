use crate::image;
use crate::session::Rgb8;
use crate::view::{ViewExtent, ViewId};

use nonempty::NonEmpty;
use rgx::core::{Bgra8, Rgba8};
use rgx::rect::Rect;

use gif::{self, SetParameter};
use png;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time;

// TODO: It's probably better to represent this as a `Vec<u8>` with a "format" tag.
// It would avoid a bunch of pattern matching at least..
#[derive(Debug, Clone)]
pub enum Pixels {
    Bgra(Box<[Bgra8]>),
    Rgba(Box<[Rgba8]>),
}

impl Pixels {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Bgra(buf) => buf.len(),
            Self::Rgba(buf) => buf.len(),
        }
    }

    pub fn slice(&self, r: core::ops::Range<usize>) -> Vec<Rgba8> {
        match self {
            Self::Bgra(buf) => {
                let slice = &buf[r];
                slice.iter().map(|bgra| (*bgra).into()).collect()
            }
            Self::Rgba(buf) => buf[r].to_owned(),
        }
    }

    pub fn get(&self, idx: usize) -> Option<Rgba8> {
        match self {
            Self::Bgra(buf) => buf
                .get(idx)
                .map(|bgra| Rgba8::new(bgra.r, bgra.g, bgra.b, bgra.a)),
            Self::Rgba(buf) => buf.get(idx).cloned(),
        }
    }

    pub fn into_rgba8(self) -> Vec<Rgba8> {
        match self {
            Self::Bgra(buf) => {
                let mut pixels: Vec<Rgba8> = Vec::with_capacity(buf.len());
                for c in buf.into_iter() {
                    pixels.push((*c).into());
                }
                pixels
            }
            Self::Rgba(buf) => buf.into(),
        }
    }

    pub fn into_bgra8(self) -> Vec<Bgra8> {
        match self {
            Self::Rgba(buf) => {
                let mut pixels: Vec<Bgra8> = Vec::with_capacity(buf.len());
                for c in buf.into_iter() {
                    pixels.push((*c).into());
                }
                pixels
            }
            Self::Bgra(buf) => buf.into(),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Bgra(buf) => {
                let (head, body, tail) = unsafe { buf.align_to::<u8>() };
                assert!(head.is_empty() && tail.is_empty());
                body
            }
            Self::Rgba(buf) => {
                let (head, body, tail) = unsafe { buf.align_to::<u8>() };
                assert!(head.is_empty() && tail.is_empty());
                body
            }
        }
    }
}

pub struct ResourceManager {
    resources: Rc<RefCell<Resources>>,
}

pub struct Resources {
    data: BTreeMap<ViewId, ViewResources>,
}

impl Resources {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn get_snapshot(&self, id: ViewId) -> (&Snapshot, &Pixels) {
        self.data
            .get(&id)
            .map(|r| r.current_snapshot())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_snapshot_mut(&mut self, id: ViewId) -> (&mut Snapshot, &Pixels) {
        self.data
            .get_mut(&id)
            .map(|r| r.current_snapshot_mut())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_snapshot_rect(&self, id: ViewId, rect: &Rect<i32>) -> Vec<Rgba8> {
        let (snapshot, pixels) = self.get_snapshot(id);

        let w = rect.width() as usize;
        let h = rect.height() as usize;

        let total_w = snapshot.width() as usize;
        let total_h = snapshot.height() as usize;

        let mut buffer: Vec<Rgba8> = Vec::with_capacity(w * h);

        for y in (rect.y1 as usize..rect.y2 as usize).rev() {
            let y = total_h - y - 1;
            let offset = y * total_w + rect.x1 as usize;
            let row = &pixels.slice(offset..offset + w);

            buffer.extend_from_slice(row);
        }
        assert!(buffer.len() == w * h);

        buffer
    }

    pub fn get_view_mut(&mut self, id: ViewId) -> Option<&mut ViewResources> {
        self.data.get_mut(&id)
    }
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            resources: Rc::new(RefCell::new(Resources::new())),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
        }
    }

    pub fn lock(&self) -> Ref<Resources> {
        self.resources.borrow()
    }

    pub fn lock_mut(&self) -> RefMut<Resources> {
        self.resources.borrow_mut()
    }

    pub fn remove_view(&mut self, id: ViewId) {
        self.resources.borrow_mut().data.remove(&id);
    }

    pub fn add_blank_view(&mut self, id: ViewId, w: u32, h: u32) {
        let len = w as usize * h as usize;
        let pixels = vec![Rgba8::TRANSPARENT; len];

        self.add_view(id, w, h, Pixels::Rgba(pixels.into()));
    }

    pub fn load_image<P: AsRef<Path>>(path: P) -> io::Result<(u32, u32, Vec<Rgba8>)> {
        let (buffer, width, height) = image::load(path)?;
        let pixels = Rgba8::align(&buffer);

        // TODO: (perf) Avoid the copy?

        Ok((width, height, pixels.into()))
    }

    pub fn save_view<P: AsRef<Path>>(
        &self,
        id: ViewId,
        path: P,
    ) -> io::Result<(SnapshotId, usize)> {
        let mut resources = self.lock_mut();
        let (snapshot, pixels) = resources.get_snapshot_mut(id);
        let (w, h) = (snapshot.width(), snapshot.height());

        let f = File::create(path.as_ref())?;
        let out = &mut io::BufWriter::new(f);
        let mut encoder = png::Encoder::new(out, w, h);

        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);

        let pixels = pixels.clone().into_rgba8();

        // Convert pixels from BGRA to RGBA, for writing to disk.
        // TODO: Use `align_to`
        // TODO: (perf) Can this be made faster?
        let mut image: Vec<u8> = Vec::with_capacity(snapshot.size);
        for rgba in pixels.iter().cloned() {
            image.extend_from_slice(&[rgba.r, rgba.g, rgba.b, rgba.a]);
        }

        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image)?;

        Ok((snapshot.id, (w * h) as usize))
    }

    pub fn save_view_svg<P: AsRef<Path>>(&self, id: ViewId, path: P) -> io::Result<usize> {
        use std::io::Write;

        let resources = self.lock();
        let (snapshot, pixels) = resources.get_snapshot(id);
        let (w, h) = (snapshot.width() as usize, snapshot.height() as usize);

        let f = File::create(path.as_ref())?;
        let out = &mut io::BufWriter::new(f);

        writeln!(
            out,
            r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" fill="none" xmlns="http://www.w3.org/2000/svg">"#,
            w, h, w, h,
        )?;

        for (i, rgba) in pixels
            .clone()
            .into_rgba8()
            .iter()
            .cloned()
            .enumerate()
            .filter(|(_, c)| c.a > 0)
        {
            let rgb: Rgb8 = rgba.into();

            let x = i % w;
            let y = i / h;

            writeln!(
                out,
                r#"<rect x="{}" y="{}" width="1" height="1" fill="{}"/>"#,
                x, y, rgb
            )?;
        }

        writeln!(out, "</svg>")?;

        Ok(w * h)
    }

    pub fn save_view_gif<P: AsRef<Path>>(
        &self,
        id: ViewId,
        path: P,
        frame_delay: time::Duration,
        palette: &[Rgba8],
    ) -> io::Result<usize> {
        // The gif encoder expects the frame delay in units of 10ms.
        let frame_delay = frame_delay.as_millis() / 10;
        // If the passed in delay is larger than a `u16` can hold,
        // we ensure it doesn't overflow.
        let frame_delay = u128::min(frame_delay, u16::max_value() as u128) as u16;

        let mut resources = self.lock_mut();
        let (snapshot, pixels) = resources.get_snapshot_mut(id);
        let extent = snapshot.extent;
        let nframes = extent.nframes;

        // Create a color palette for the gif, where the zero index is used
        // for transparency.
        let transparent: u8 = 0;
        let mut palette = palette.to_vec();
        palette.push(Rgba8::TRANSPARENT);
        palette.sort();

        assert!(palette[transparent as usize] == Rgba8::TRANSPARENT);
        assert!(palette.len() <= 256);

        // Convert BGRA pixels into indexed pixels.
        let mut image: Vec<u8> = Vec::with_capacity(snapshot.size);
        for rgba in pixels.clone().into_rgba8().iter().cloned() {
            if let Ok(index) = palette.binary_search(&rgba) {
                image.push(index as u8);
            } else {
                image.push(transparent);
            }
        }

        let (fw, fh) = (extent.fw as usize, extent.fh as usize);
        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(nframes);
        frames.resize(nframes, Vec::with_capacity(fw * fh));

        {
            // Convert animation strip into discrete frames for gif encoder.
            let nrows = fh as usize * nframes;
            let row_nbytes = fw as usize;

            for i in 0..nrows {
                let offset = i * row_nbytes;
                let row = &image[offset..offset + row_nbytes];

                frames[i % nframes].extend_from_slice(row);
            }
        }

        // Discard alpha channel and convert to a `&[u8]`.
        let palette: Vec<Rgb8> = palette.into_iter().map(Rgb8::from).collect();
        let (head, palette, tail) = unsafe { palette.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());

        let mut f = File::create(path.as_ref())?;
        let mut encoder = gif::Encoder::new(&mut f, fw as u16, fh as u16, palette)?;
        encoder.set(gif::Repeat::Infinite)?;

        for frame in frames.iter_mut() {
            let mut frame =
                gif::Frame::from_indexed_pixels(fw as u16, fh as u16, &frame, Some(transparent));
            frame.delay = frame_delay;
            frame.dispose = gif::DisposalMethod::Background;

            encoder.write_frame(&frame)?;
        }

        Ok(fw * fh * nframes)
    }

    pub fn add_view(&mut self, id: ViewId, fw: u32, fh: u32, pixels: Pixels) {
        self.resources
            .borrow_mut()
            .data
            .insert(id, ViewResources::new(pixels, fw, fh));
    }
}

#[derive(Debug)]
pub struct ViewResources {
    /// Non empty list of view snapshots.
    snapshots: NonEmpty<Snapshot>,
    /// Current view snapshot.
    snapshot: usize,
    /// Current view pixels. We keep a separate decompressed
    /// cache of the view pixels for performance reasons.
    pixels: Pixels,
}

impl ViewResources {
    fn new(pixels: Pixels, fw: u32, fh: u32) -> Self {
        Self {
            snapshots: NonEmpty::new(Snapshot::new(
                SnapshotId(0),
                pixels.clone(),
                ViewExtent::new(fw, fh, 1),
            )),
            snapshot: 0,
            pixels,
        }
    }

    pub fn current_snapshot(&self) -> (&Snapshot, &Pixels) {
        (
            self.snapshots
                .get(self.snapshot)
                .expect("there must always be a current snapshot"),
            &self.pixels,
        )
    }

    pub fn current_snapshot_mut(&mut self) -> (&mut Snapshot, &Pixels) {
        (
            self.snapshots
                .get_mut(self.snapshot)
                .expect("there must always be a current snapshot"),
            &self.pixels,
        )
    }

    pub fn push_snapshot(&mut self, pixels: Pixels, extent: ViewExtent) {
        // FIXME: If pixels match current snapshot exactly, don't add the snapshot.

        // If we try to add a snapshot when we're not at the
        // latest, we have to clear the list forward.
        if self.snapshot != self.snapshots.len() - 1 {
            self.snapshots.truncate(self.snapshot + 1);
            self.snapshot = self.snapshots.len() - 1;
        }
        self.snapshot += 1;
        self.pixels = pixels.clone();

        self.snapshots
            .push(Snapshot::new(SnapshotId(self.snapshot), pixels, extent));
    }

    pub fn prev_snapshot(&mut self) -> Option<&Snapshot> {
        if self.snapshot == 0 {
            return None;
        }
        if let Some(snapshot) = self.snapshots.get(self.snapshot - 1) {
            self.snapshot -= 1;
            self.pixels = snapshot.pixels().into();

            Some(snapshot)
        } else {
            None
        }
    }

    pub fn next_snapshot(&mut self) -> Option<&Snapshot> {
        if let Some(snapshot) = self.snapshots.get(self.snapshot + 1) {
            self.snapshot += 1;
            self.pixels = snapshot.pixels().into();

            Some(snapshot)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct SnapshotId(usize);

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        SnapshotId(0)
    }
}

#[derive(Debug, Copy, Clone)]
enum SnapshotFormat {
    Rgba8,
    Bgra8,
}

#[derive(Debug)]
pub struct Snapshot {
    pub id: SnapshotId,
    pub extent: ViewExtent,

    size: usize,
    pixels: Compressed<Box<[u8]>>,

    format: SnapshotFormat,
}

impl Snapshot {
    pub fn new(id: SnapshotId, pixels: Pixels, extent: ViewExtent) -> Self {
        let format = match pixels {
            Pixels::Rgba(_) => SnapshotFormat::Rgba8,
            Pixels::Bgra(_) => SnapshotFormat::Bgra8,
        };

        let size = pixels.len();
        let pixels =
            Compressed::from(pixels).expect("compressing snapshot shouldn't result in an error");

        debug_assert!(
            (extent.fw * extent.fh) as usize * extent.nframes == size,
            "the pixel buffer has the expected size"
        );

        Self {
            id,
            extent,
            size,
            pixels,
            format,
        }
    }

    pub fn width(&self) -> u32 {
        self.extent.fw * self.extent.nframes as u32
    }

    pub fn height(&self) -> u32 {
        self.extent.fh
    }

    ////////////////////////////////////////////////////////////////////////////

    fn pixels(&self) -> Pixels {
        let bytes = self
            .pixels
            .decompress()
            .expect("decompressing snapshot shouldn't result in an error");
        match self.format {
            SnapshotFormat::Rgba8 => Pixels::Rgba(Rgba8::align(&bytes).into()),
            SnapshotFormat::Bgra8 => Pixels::Bgra(Bgra8::align(&bytes).into()),
        }
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct Compressed<T>(T);

impl Compressed<Box<[u8]>> {
    fn from(input: Pixels) -> snap::Result<Self> {
        let mut enc = snap::Encoder::new();
        let bytes = input.as_bytes();
        enc.compress_vec(bytes).map(|v| Self(v.into_boxed_slice()))
    }

    fn decompress(&self) -> snap::Result<Vec<u8>> {
        let mut dec = snap::Decoder::new();
        dec.decompress_vec(&self.0)
    }
}
