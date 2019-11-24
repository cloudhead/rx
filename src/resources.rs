use crate::image;
use crate::view::{ViewExtent, ViewId};

use nonempty::NonEmpty;
use rgx::core::{Bgra8, Rgba8};

use gif::{self, SetParameter};
use png;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem;
use std::path::Path;
use std::rc::Rc;
use std::time;

/// Speed at which to encode gifs. This mainly affects quantization.
const GIF_ENCODING_SPEED: i32 = 10;

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

    pub fn get_snapshot(&self, id: ViewId) -> (&Snapshot, &[Bgra8]) {
        self.data
            .get(&id)
            .map(|r| r.current_snapshot())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_snapshot_mut(&mut self, id: ViewId) -> (&mut Snapshot, &[Bgra8]) {
        self.data
            .get_mut(&id)
            .map(|r| r.current_snapshot_mut())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
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
        let len = w as usize * h as usize * 4;
        let pixels = vec![0; len];

        self.add_view(id, w, h, &pixels);
    }

    pub fn load_image<P: AsRef<Path>>(path: P) -> io::Result<(u32, u32, Vec<u8>)> {
        let (buffer, width, height) = image::load(path)?;

        // Convert pixels to BGRA, since they are going to be loaded into
        // the view framebuffer, which is BGRA.
        let mut pixels: Vec<u8> = Vec::with_capacity(buffer.len());
        for rgba in buffer.chunks(4) {
            match rgba {
                [r, g, b, a] => pixels.extend_from_slice(&[*b, *g, *r, *a]),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid pixel buffer size",
                    ))
                }
            }
        }

        Ok((width, height, pixels))
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

        // Convert pixels from BGRA to RGBA, for writing to disk.
        // TODO: (perf) Can this be made faster?
        let mut image: Vec<u8> = Vec::with_capacity(snapshot.size);
        for bgra in pixels.iter().cloned() {
            let rgba: Rgba8 = bgra.into();
            image.extend_from_slice(&[rgba.r, rgba.g, rgba.b, rgba.a]);
        }

        let mut writer = encoder.write_header()?;
        writer.write_image_data(&image)?;

        Ok((snapshot.id, (w * h) as usize))
    }

    pub fn save_view_gif<P: AsRef<Path>>(
        &self,
        id: ViewId,
        path: P,
        frame_delay: time::Duration,
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

        // Convert pixels from BGRA to RGBA, for writing to disk.
        let mut image: Vec<u8> = Vec::with_capacity(snapshot.size);
        for bgra in pixels.iter().cloned() {
            let rgba: Rgba8 = bgra.into();
            image.extend_from_slice(&[rgba.r, rgba.g, rgba.b, rgba.a]);
        }

        let (fw, fh) = (extent.fw as usize, extent.fh as usize);
        let frame_nbytes = fw * fh as usize * mem::size_of::<Rgba8>();

        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(nframes);
        frames.resize(nframes, Vec::with_capacity(frame_nbytes));

        {
            // Convert animation strip into discrete frames for gif encoder.
            let nrows = fh as usize * nframes;
            let row_nbytes = fw as usize * mem::size_of::<Rgba8>();

            for i in 0..nrows {
                let offset = i * row_nbytes;
                let row = &image[offset..offset + row_nbytes];

                frames[i % nframes].extend_from_slice(row);
            }
        }

        let mut f = File::create(path.as_ref())?;
        let mut encoder = gif::Encoder::new(&mut f, fw as u16, fh as u16, &[])?;
        encoder.set(gif::Repeat::Infinite)?;

        for mut frame in frames.iter_mut() {
            let mut frame = gif::Frame::from_rgba_speed(
                fw as u16,
                fh as u16,
                &mut frame,
                self::GIF_ENCODING_SPEED,
            );
            frame.delay = frame_delay;
            frame.dispose = gif::DisposalMethod::Background;

            encoder.write_frame(&frame)?;
        }

        Ok(frame_nbytes * nframes)
    }

    pub fn add_view(&mut self, id: ViewId, fw: u32, fh: u32, pixels: &[u8]) {
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
    pixels: Box<[Bgra8]>,
}

impl ViewResources {
    fn new(pixels: &[u8], fw: u32, fh: u32) -> Self {
        let pxs = Bgra8::align(&pixels);
        Self {
            snapshots: NonEmpty::new(Snapshot::new(
                SnapshotId(0),
                pixels,
                ViewExtent::new(fw, fh, 1),
            )),
            snapshot: 0,
            pixels: pxs.into(),
        }
    }

    pub fn current_snapshot(&self) -> (&Snapshot, &[Bgra8]) {
        (
            self.snapshots
                .get(self.snapshot)
                .expect("there must always be a current snapshot"),
            &self.pixels,
        )
    }

    pub fn current_snapshot_mut(&mut self) -> (&mut Snapshot, &[Bgra8]) {
        (
            self.snapshots
                .get_mut(self.snapshot)
                .expect("there must always be a current snapshot"),
            &self.pixels,
        )
    }

    pub fn push_snapshot(&mut self, pixels: &[u8], extent: ViewExtent) {
        // FIXME: If pixels match current snapshot exactly, don't add the snapshot.

        // If we try to add a snapshot when we're not at the
        // latest, we have to clear the list forward.
        if self.snapshot != self.snapshots.len() - 1 {
            self.snapshots.truncate(self.snapshot + 1);
            self.snapshot = self.snapshots.len() - 1;
        }
        self.snapshot += 1;
        self.pixels = Bgra8::align(&pixels).into();

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

#[derive(Debug)]
pub struct Snapshot {
    pub id: SnapshotId,
    pub extent: ViewExtent,

    size: usize,
    pixels: Compressed<Box<[u8]>>,
}

impl Snapshot {
    pub fn new(id: SnapshotId, pixels: &[u8], extent: ViewExtent) -> Self {
        let size = pixels.len();
        let pixels =
            Compressed::from(pixels).expect("compressing snapshot shouldn't result in an error");

        debug_assert!(
            (extent.fw * extent.fh) as usize * extent.nframes * mem::size_of::<Rgba8>() == size,
            "the pixel buffer has the expected size"
        );

        Self {
            id,
            extent,
            size,
            pixels,
        }
    }

    pub fn width(&self) -> u32 {
        self.extent.fw * self.extent.nframes as u32
    }

    pub fn height(&self) -> u32 {
        self.extent.fh
    }

    ////////////////////////////////////////////////////////////////////////////

    fn pixels(&self) -> Vec<Bgra8> {
        // TODO: (perf) Any way not to clone here?
        Bgra8::align(
            &self
                .pixels
                .decompress()
                .expect("decompressing snapshot shouldn't result in an error"),
        )
        .to_owned()
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct Compressed<T>(T);

impl Compressed<Box<[u8]>> {
    fn from(input: &[u8]) -> snap::Result<Self> {
        let mut enc = snap::Encoder::new();
        enc.compress_vec(input).map(|v| Self(v.into_boxed_slice()))
    }

    fn decompress(&self) -> snap::Result<Vec<u8>> {
        let mut dec = snap::Decoder::new();
        dec.decompress_vec(&self.0)
    }
}
