use crate::image;
use crate::view::ViewId;

use rgx::core::Rgba8;
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
    pub resources: Arc<RwLock<Resources>>,
}

pub struct Resources {
    pub data: BTreeMap<ViewId, SnapshotList>,
}

impl Resources {
    fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    pub fn get_snapshot(&self, id: &ViewId) -> &Snapshot {
        self.data.get(id).map(|r| r.current()).expect(&format!(
            "view #{} must exist and have an associated snapshot",
            id
        ))
    }

    pub fn get_snapshot_mut(&mut self, id: &ViewId) -> &mut Snapshot {
        self.data
            .get_mut(id)
            .map(|r| r.current_mut())
            .expect(&format!(
                "view #{} must exist and have an associated snapshot",
                id
            ))
    }

    pub fn get_snapshots_mut(
        &mut self,
        id: &ViewId,
    ) -> Option<&mut SnapshotList> {
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

        // Convert pixels from BGRA to RGBA, for writing to disk.
        let mut pixels: Vec<u8> = Vec::with_capacity(snapshot.len());
        for rgba in snapshot.pixels().chunks(mem::size_of::<Rgba8>()) {
            match rgba {
                &[b, g, r, a] => pixels.extend_from_slice(&[r, g, b, a]),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid pixel buffer size",
                    ))
                }
            }
        }

        // Bitmap image
        if let Some(ext) = path.as_ref().extension() {
            if ext == "bmp" {
                image::save_bmp(path.as_ref(), pixels, w, h).map_err(|_e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("Unable to save the following file: `{}`", path.as_ref().display()),
                    )
                })?;
                return Ok((snapshot.id, (w * h) as usize));
            }
        }

        // PNG image (default)
        let f = File::create(path.as_ref())?;
        let ref mut out = io::BufWriter::new(f);
        let mut encoder = png::Encoder::new(out, w, h);

        encoder.set_color(png::ColorType::RGBA);
        encoder.set_depth(png::BitDepth::Eight);

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
        let mut pixels: Vec<u8> = Vec::with_capacity(snapshot.len());
        for rgba in snapshot.pixels().chunks(mem::size_of::<Rgba8>()) {
            match rgba {
                &[b, g, r, a] => pixels.extend_from_slice(&[r, g, b, a]),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid pixel buffer size",
                    ))
                }
            }
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
            .insert(id, SnapshotList::new(pixels, fw, fh));
    }
}

#[derive(Debug)]
pub struct SnapshotList {
    list: NonEmpty<Snapshot>,
    current: usize,
}

impl SnapshotList {
    fn new(pixels: &[u8], fw: u32, fh: u32) -> Self {
        Self {
            list: NonEmpty::new(Snapshot::new(
                SnapshotId(0),
                pixels,
                fw,
                fh,
                1,
            )),
            current: 0,
        }
    }

    pub fn current(&self) -> &Snapshot {
        self.list
            .get(self.current)
            .expect("there must always be a current snapshot")
    }

    pub fn current_mut(&mut self) -> &mut Snapshot {
        self.list
            .get_mut(self.current)
            .expect("there must always be a current snapshot")
    }

    pub fn push(&mut self, pixels: &[u8], fw: u32, fh: u32, nframes: usize) {
        // FIXME: If pixels match current snapshot exactly, don't add the snapshot.

        // If we try to add a snapshot when we're not at the
        // latest, we have to clear the list forward.
        if self.current != self.list.len() - 1 {
            self.list.truncate(self.current + 1);
            self.current = self.list.len() - 1;
        }
        self.current += 1;

        self.list.push(Snapshot::new(
            SnapshotId(self.current),
            pixels,
            fw,
            fh,
            nframes,
        ));
    }

    pub fn prev(&mut self) -> Option<&Snapshot> {
        if self.current == 0 {
            return None;
        }
        if let Some(snapshot) = self.list.get(self.current - 1) {
            self.current -= 1;
            Some(snapshot)
        } else {
            None
        }
    }

    pub fn next(&mut self) -> Option<&Snapshot> {
        if let Some(snapshot) = self.list.get(self.current + 1) {
            self.current += 1;
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
}

impl Snapshot {
    pub fn new(
        id: SnapshotId,
        pixels: &[u8],
        fw: u32,
        fh: u32,
        nframes: usize,
    ) -> Self {
        let pixels = Compressed::from(pixels)
            .expect("compressing snapshot shouldn't result in an error");

        Self {
            id,
            fw,
            fh,
            nframes,
            pixels,
        }
    }

    pub fn pixels(&self) -> Vec<u8> {
        self.pixels
            .decompress()
            .expect("decompressing snapshot shouldn't result in an error")
    }

    pub fn len(&self) -> usize {
        self.width() as usize * self.height() as usize
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
