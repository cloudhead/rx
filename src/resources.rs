use crate::view::ViewId;

use rgx::nonempty::NonEmpty;

use image::png;
use image::ImageDecoder;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

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

    pub fn get_snapshot(&self, id: &ViewId) -> Option<&Snapshot> {
        self.data.get(id).map(|r| r.current())
    }

    pub fn get_snapshot_mut(&mut self, id: &ViewId) -> Option<&mut Snapshot> {
        self.data.get_mut(id).map(|r| r.current_mut())
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

    pub fn add_blank_view(&mut self, id: &ViewId, w: u32, h: u32) {
        let len = w as usize * h as usize * 4;
        let mut pixels = Vec::with_capacity(len);
        pixels.resize(len, 0);

        self.add_view(id, w, h, pixels);
    }

    pub fn load_view<P: AsRef<Path>>(
        &mut self,
        id: &ViewId,
        path: P,
    ) -> io::Result<(u32, u32)> {
        let f = File::open(path)?;
        let decoder = image::png::PNGDecoder::new(f)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let (width, height) = decoder.dimensions();
        let (width, height) = (width as u32, height as u32);

        let buffer: Vec<u8> = decoder
            .read_image()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Convert pixels to BGRA, since they are going to be loaded into
        // the view framebuffer, which is BGRA.
        let mut pixels: Vec<u8> = Vec::with_capacity(buffer.len());
        for rgba in buffer.chunks(4) {
            match rgba {
                &[r, g, b, a] => pixels.extend_from_slice(&[b, g, r, a]),
                _ => panic!("fatal: invalid pixel buffer size"),
            }
        }
        self.add_view(id, width, height, pixels);

        Ok((width, height))
    }

    pub fn save_view<P: AsRef<Path>>(
        &self,
        id: &ViewId,
        path: P,
    ) -> io::Result<(SnapshotId, usize)> {
        let mut resources = self.resources.write().unwrap();
        let snapshot = resources.get_snapshot_mut(id).ok_or(io::Error::new(
            io::ErrorKind::Other,
            "error: unknown view",
        ))?;

        let f = File::create(path.as_ref())?;
        let encoder = png::PNGEncoder::new(f);

        // Convert pixels from BGRA to RGBA, for writing to disk.
        let mut pixels: Vec<u8> = Vec::with_capacity(snapshot.pixels.len());
        for rgba in snapshot.pixels.chunks(4) {
            match rgba {
                &[b, g, r, a] => pixels.extend_from_slice(&[r, g, b, a]),
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "error: invalid pixel buffer size",
                    ))
                }
            }
        }

        let (w, h) = (snapshot.width(), snapshot.height());

        encoder
            .encode(&pixels, w, h, image::ColorType::RGBA(8))
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok((snapshot.id, (w * h) as usize))
    }

    ///////////////////////////////////////////////////////////////////////////

    fn add_view(&mut self, id: &ViewId, fw: u32, fh: u32, pixels: Vec<u8>) {
        self.resources
            .write()
            .unwrap()
            .data
            .insert(*id, SnapshotList::new(pixels, fw, fh));
    }
}

#[derive(Debug)]
pub struct SnapshotList {
    list: NonEmpty<Snapshot>,
    current: usize,
}

impl SnapshotList {
    fn new(pixels: Vec<u8>, fw: u32, fh: u32) -> Self {
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
        self.list.get(self.current).unwrap()
    }

    pub fn current_mut(&mut self) -> &mut Snapshot {
        self.list.get_mut(self.current).unwrap()
    }

    pub fn push(&mut self, pixels: Vec<u8>, fw: u32, fh: u32, nframes: usize) {
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
    pub pixels: Vec<u8>,
    pub fw: u32,
    pub fh: u32,
    pub nframes: usize,
}

impl Snapshot {
    pub fn new(
        id: SnapshotId,
        pixels: Vec<u8>,
        fw: u32,
        fh: u32,
        nframes: usize,
    ) -> Self {
        Self {
            id,
            fw,
            fh,
            nframes,
            pixels,
        }
    }

    pub fn width(&self) -> u32 {
        self.fw * self.nframes as u32
    }

    pub fn height(&self) -> u32 {
        self.fh
    }
}
