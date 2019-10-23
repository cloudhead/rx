use crate::image;
use crate::view::ViewId;

use rgx::core::{Bgra8, Rgba8};
use rgx::nonempty::NonEmpty;

use gif::{self, SetParameter};
use png;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem;
use std::path::Path;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time;

/// Speed at which to encode gifs. This mainly affects quantization.
const GIF_ENCODING_SPEED: i32 = 10;

pub struct ResourceManager {
    resources: Arc<RwLock<Resources>>,
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

    pub fn get_snapshot(&self, id: &ViewId) -> &Snapshot {
        self.data
            .get(id)
            .map(|r| r.current_snapshot())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_snapshot_mut(&mut self, id: &ViewId) -> &mut Snapshot {
        self.data
            .get_mut(id)
            .map(|r| r.current_snapshot_mut())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_view_mut(&mut self, id: &ViewId) -> Option<&mut ViewResources> {
        self.data.get_mut(id)
    }
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            resources: Arc::new(RwLock::new(Resources::new())),
        }
    }

    pub fn clone(&self) -> Self {
        Self {
            resources: self.resources.clone(),
        }
    }

    pub fn lock(&self) -> RwLockReadGuard<Resources> {
        self.resources.read().unwrap()
    }

    pub fn lock_mut(&self) -> RwLockWriteGuard<Resources> {
        self.resources.write().unwrap()
    }

    pub fn remove_view(&mut self, id: &ViewId) {
        self.resources.write().unwrap().data.remove(id);
    }

    pub fn add_blank_view(&mut self, id: ViewId, w: u32, h: u32) {
        let len = w as usize * h as usize * 4;
        let pixels = vec![0; len];

        self.add_view(id, w, h, &pixels);
    }

    pub fn load_image<P: AsRef<Path>>(
        path: P,
    ) -> io::Result<(u32, u32, Vec<u8>)> {
        let (buffer, width, height) = image::load(path)?;

        // Convert pixels to BGRA, since they are going to be loaded into
        // the view framebuffer, which is BGRA.
        let mut pixels: Vec<u8> = Vec::with_capacity(buffer.len());
        for rgba in buffer.chunks(4) {
            match rgba {
                &[r, g, b, a] => pixels.extend_from_slice(&[b, g, r, a]),
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
        id: &ViewId,
        path: P,
    ) -> io::Result<(SnapshotId, usize)> {
        let mut resources = self.lock_mut();
        let snapshot = resources.get_snapshot_mut(id);
        let (w, h) = (snapshot.width(), snapshot.height());

        let f = File::create(path.as_ref())?;
        let ref mut out = io::BufWriter::new(f);
        let mut encoder = png::Encoder::new(out, w, h);

        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);

        // Convert pixels from BGRA to RGBA, for writing to disk.
        // TODO: (perf) Can this be made faster?
        let mut pixels: Vec<u8> = Vec::with_capacity(snapshot.size);
        for bgra in snapshot.pixels() {
            let rgba: Rgba8 = bgra.into();
            pixels.extend_from_slice(&[rgba.r, rgba.g, rgba.b, rgba.a]);
        }

        let mut writer = encoder.write_header()?;
        writer.write_image_data(&pixels)?;

        Ok((snapshot.id, (w * h) as usize))
    }

    pub fn save_view_gif<P: AsRef<Path>>(
        &self,
        id: &ViewId,
        path: P,
        frame_delay: time::Duration,
    ) -> io::Result<usize> {
        // The gif encoder expects the frame delay in units of 10ms.
        let frame_delay = frame_delay.as_millis() / 10;
        // If the passed in delay is larger than a `u16` can hold,
        // we ensure it doesn't overflow.
        let frame_delay =
            u128::min(frame_delay, u16::max_value() as u128) as u16;

        let mut resources = self.lock_mut();
        let snapshot = resources.get_snapshot_mut(id);
        let nframes = snapshot.nframes;

        // Convert pixels from BGRA to RGBA, for writing to disk.
        let mut pixels: Vec<u8> = Vec::with_capacity(snapshot.size);
        for bgra in snapshot.pixels() {
            let rgba: Rgba8 = bgra.into();
            pixels.extend_from_slice(&[rgba.r, rgba.g, rgba.b, rgba.a]);
        }

        let (fw, fh) = (snapshot.fw as usize, snapshot.fh as usize);
        let frame_nbytes = fw * fh as usize * mem::size_of::<Rgba8>();

        let mut frames: Vec<Vec<u8>> = Vec::with_capacity(nframes);
        frames.resize(nframes, Vec::with_capacity(frame_nbytes));

        {
            // Convert animation strip into discrete frames for gif encoder.
            let nrows = fh as usize * nframes;
            let row_nbytes = fw as usize * mem::size_of::<Rgba8>();

            for i in 0..nrows {
                let offset = i * row_nbytes;
                let row = &pixels[offset..offset + row_nbytes];

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
            .write()
            .unwrap()
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
}

impl ViewResources {
    fn new(pixels: &[u8], fw: u32, fh: u32) -> Self {
        Self {
            snapshots: NonEmpty::new(Snapshot::new(
                SnapshotId(0),
                pixels,
                fw,
                fh,
                1,
            )),
            snapshot: 0,
        }
    }

    pub fn current_snapshot(&self) -> &Snapshot {
        self.snapshots
            .get(self.snapshot)
            .expect("there must always be a current snapshot")
    }

    pub fn current_snapshot_mut(&mut self) -> &mut Snapshot {
        self.snapshots
            .get_mut(self.snapshot)
            .expect("there must always be a current snapshot")
    }

    pub fn push_snapshot(
        &mut self,
        pixels: &[u8],
        fw: u32,
        fh: u32,
        nframes: usize,
    ) {
        // FIXME: If pixels match current snapshot exactly, don't add the snapshot.

        // If we try to add a snapshot when we're not at the
        // latest, we have to clear the list forward.
        if self.snapshot != self.snapshots.len() - 1 {
            self.snapshots.truncate(self.snapshot + 1);
            self.snapshot = self.snapshots.len() - 1;
        }
        self.snapshot += 1;

        self.snapshots.push(Snapshot::new(
            SnapshotId(self.snapshot),
            pixels,
            fw,
            fh,
            nframes,
        ));
    }

    pub fn prev_snapshot(&mut self) -> Option<&Snapshot> {
        if self.snapshot == 0 {
            return None;
        }
        if let Some(snapshot) = self.snapshots.get(self.snapshot - 1) {
            self.snapshot -= 1;
            Some(snapshot)
        } else {
            None
        }
    }

    pub fn next_snapshot(&mut self) -> Option<&Snapshot> {
        if let Some(snapshot) = self.snapshots.get(self.snapshot + 1) {
            self.snapshot += 1;
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
    pub pixels: Compressed<Box<[u8]>>,
    pub fw: u32,
    pub fh: u32,
    pub nframes: usize,
    pub size: usize,
}

impl Snapshot {
    pub fn new(
        id: SnapshotId,
        pixels: &[u8],
        fw: u32,
        fh: u32,
        nframes: usize,
    ) -> Self {
        let size = pixels.len();
        let pixels = Compressed::from(pixels)
            .expect("compressing snapshot shouldn't result in an error");

        debug_assert!(
            (fw * fh) as usize * nframes * mem::size_of::<Rgba8>() == size,
            "the pixel buffer has the expected size"
        );

        Self {
            id,
            fw,
            fh,
            nframes,
            size,
            pixels,
        }
    }

    pub fn pixels(&self) -> Vec<Bgra8> {
        // TODO: (perf) Any way not to clone here?
        Bgra8::align(
            &self
                .pixels
                .decompress()
                .expect("decompressing snapshot shouldn't result in an error"),
        )
        .to_owned()
    }

    pub fn width(&self) -> u32 {
        self.fw * self.nframes as u32
    }

    pub fn height(&self) -> u32 {
        self.fh
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
