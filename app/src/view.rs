use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic;
use std::sync::Arc;

use nonempty::NonEmpty;

use crate::gfx::color::{Image, ImageError};
use crate::gfx::prelude::*;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid view extent")]
    ViewExtent,
    #[error("'{path}': {err}")]
    Io { path: PathBuf, err: io::Error },
    #[error("'{path}': {err}")]
    Image { path: PathBuf, err: ImageError },
}

/// View identifier.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug, Default)]
pub struct ViewId(u64);

impl ViewId {
    pub fn next() -> Self {
        static NEXT: atomic::AtomicU64 = atomic::AtomicU64::new(1);

        Self(NEXT.fetch_add(1, atomic::Ordering::SeqCst))
    }
}

impl fmt::Display for ViewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub struct View {
    /// Identifier.
    pub id: ViewId,
    /// View size and frame count.
    pub extent: ViewExtent,
    /// Animation state of the sprite displayed by this view.
    pub animation: Animation<Rect<f32>>,
    /// View image at the current snapshot. We keep a separate decompressed
    /// cache of the view pixels for performance reasons.
    pub image: Image,
    /// Path of the view, if saved.
    pub path: Option<PathBuf>,
    /// Whether the view was modified since the last save.
    pub modified: bool,
    /// Current view snapshot as an index in the snapshot list.
    pub snapshot: usize,
    /// List of view snapshots, used for undo/redo.
    pub snapshots: NonEmpty<Snapshot>,
}

impl View {
    pub fn new(id: ViewId, extent: ViewExtent, image: Image) -> Result<Self, Error> {
        // FIXME: We should move extent to image.
        assert_eq!(extent.size(), image.size);

        let frames = extent.frames()?;

        Ok(Self {
            id,
            extent,
            animation: Animation::new(frames),
            image: image.clone(),
            path: None,
            modified: false,
            snapshot: 0,
            snapshots: NonEmpty::new(Snapshot::new(image.pixels, extent)),
        })
    }

    pub fn path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// View width. Basically frame-width times number of frames.
    pub fn width(&self) -> u32 {
        self.extent.fw * self.animation.len() as u32
    }

    /// View height.
    pub fn height(&self) -> u32 {
        self.extent.fh
    }

    /// View size.
    pub fn size(&self) -> Size<u32> {
        Size::new(self.width(), self.height())
    }

    /// Resize view frame.
    pub fn resize(&mut self, size: Size<u32>) -> Result<(), Error> {
        self.reset(ViewExtent::new(size.w, size.h, self.animation.len()))?;
        self.modified = true;

        Ok(())
    }

    /// View status as a string.
    pub fn status(&self) -> String {
        let path = self
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        if self.modified {
            format!("{} *", path)
        } else {
            format!("{}", path)
        }
    }

    /// Get the color at the given view coordinate.
    pub fn sample(&self, point: Point2D<u32>) -> Option<Rgba8> {
        self.image().sample(point).copied()
    }

    /// Get the current view image.
    pub fn image(&self) -> Image {
        self.image.clone()
    }

    /// Get the current view pixels.
    pub fn pixels(&self) -> Arc<[Rgba8]> {
        self.image.pixels.clone()
    }

    /// Save view as.
    pub fn save_as(&mut self, path: &Path) -> io::Result<usize> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        if let Some(view_path) = &self.path {
            // Don't let user overwrite other files.
            if path.exists() && path.canonicalize().ok() != view_path.canonicalize().ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{:?} already exists", path.display()),
                ));
            }
        }

        let f = fs::File::options()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)?;
        let image = self.image();
        let len = image.pixels.len();

        image.write(f)?;

        // Mark the view as saved.
        self.modified = false;
        // Store the save path.
        self.path = Some(path.to_path_buf());

        Ok(len)
    }

    /// Reset the view by providing frame size and number of frames.
    fn reset(&mut self, extent: ViewExtent) -> Result<(), Error> {
        let frames = extent.frames()?;

        self.animation = Animation::new(frames);
        self.extent = extent;

        Ok(())
    }

    /// When the view has been edited, we call this function which
    /// makes sure to clear the snapshot history forwards if we aren'the
    /// at the latest state.
    pub fn edited(&mut self) {
        if self.snapshot != self.snapshots.len() - 1 {
            self.snapshots.truncate(self.snapshot + 1);
            self.snapshot = self.snapshots.len() - 1;
        }
    }

    /// Record a new snapshot.
    pub fn snapshot(&mut self, pixels: Arc<[Rgba8]>, extent: ViewExtent) -> usize {
        // This is guaranteed to be true as long as `edited` is called
        // whenever the view changes.
        assert!(self.snapshot == self.snapshots.len() - 1);

        self.snapshot += 1;
        self.image = Image::new(pixels.clone(), extent.size());
        self.snapshots.push(Snapshot::new(pixels, extent));
        self.snapshot
    }

    pub fn undo(&mut self) -> Option<&Snapshot> {
        let length = self.snapshots.len();

        if let Some(snapshot) = self
            .snapshot
            .checked_sub(1)
            .and_then(|n| self.snapshots.get_mut(n))
        {
            self.snapshot -= 1;
            self.image = snapshot.image();

            log::debug!("undo: {}/{}", self.snapshot, length - 1);

            Some(snapshot)
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&Snapshot> {
        let length = self.snapshots.len();

        if let Some(snapshot) = self.snapshots.get_mut(self.snapshot + 1) {
            self.snapshot += 1;
            self.image = snapshot.image();

            log::debug!("redo: {}/{}", self.snapshot, length - 1);

            Some(snapshot)
        } else {
            None
        }
    }
}

/// View extent information.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewExtent {
    /// Frame width.
    pub fw: u32,
    /// Frame height.
    pub fh: u32,
    /// Number of frames.
    pub nframes: usize,
}

impl ViewExtent {
    pub fn new(fw: u32, fh: u32, nframes: usize) -> Self {
        ViewExtent { fw, fh, nframes }
    }

    /// Extent total width.
    pub fn width(&self) -> u32 {
        self.fw * self.nframes as u32
    }

    /// Extent total height.
    pub fn height(&self) -> u32 {
        self.fh
    }

    /// Extent size.
    pub fn size(&self) -> Size<u32> {
        Size::new(self.width(), self.height())
    }

    /// Rect containing the whole extent.
    pub fn rect(&self) -> Rect<f32> {
        Rect::origin([self.width() as f32, self.height() as f32])
    }

    /// Rect containing a single frame.
    pub fn frame(&self, n: usize) -> Rect<f32> {
        let n = n as f32;
        Rect::new([self.fw as f32 * n, 0.], [self.fw as f32, self.fh as f32])
    }

    /// Return the frames of this view extent.
    pub fn frames(&self) -> Result<NonEmpty<Rect<f32>>, Error> {
        let origin = Rect::origin([self.fw as f32, self.fh as f32]);
        let frames: Vec<_> = (0..self.nframes)
            .map(|i| origin + Vector::new(i as f32 * self.fw as f32, 0.))
            .collect();
        let frames = NonEmpty::from_vec(frames).ok_or(Error::ViewExtent)?;

        Ok(frames)
    }

    /// Compute the frame index, given a point.
    /// Warning: can underflow.
    pub fn to_frame(self, p: Point) -> usize {
        (p.x / self.fw as f32) as usize
    }
}

/// View animation.
#[derive(Debug)]
pub struct Animation<T> {
    pub index: usize,
    pub frames: NonEmpty<T>,
}

impl<T> Animation<T> {
    pub fn new(frames: NonEmpty<T>) -> Self {
        Self { index: 0, frames }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn step(&mut self) {
        self.index = (self.index + 1) % self.len();
    }

    pub fn val(&self) -> &T {
        &self.frames[self.index % self.len()]
    }
}

#[derive(Default)]
pub struct Manager {
    pub active: ViewId,
    pub views: HashMap<ViewId, View>,
    pub cursor: Option<Point2D>,
}

impl Manager {
    pub fn activate(&mut self, id: ViewId) {
        self.active = id;
    }

    pub fn active(&self) -> Option<&View> {
        self.views.get(&self.active)
    }

    pub fn active_mut(&mut self) -> Option<&mut View> {
        self.views.get_mut(&self.active)
    }

    pub fn get(&self, id: &ViewId) -> Option<&View> {
        self.views.get(id)
    }

    pub fn get_mut(&mut self, id: &ViewId) -> Option<&mut View> {
        self.views.get_mut(id)
    }

    pub fn is_active(&self, id: &ViewId) -> bool {
        self.active == *id && self.views.contains_key(id)
    }

    pub fn insert(&mut self, id: ViewId, view: View) {
        self.views.insert(id, view);
    }

    pub fn remove(&mut self, id: &ViewId) -> Option<View> {
        self.views.remove(id)
    }

    pub fn len(&self) -> usize {
        self.views.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &View> {
        self.views.values()
    }

    pub fn ids(&self) -> impl Iterator<Item = &ViewId> {
        self.views.keys()
    }

    /// Open a view from a path.
    pub fn open<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, Error> {
        let path = path.as_ref();

        debug!("open: {:?}", path);

        let bytes = fs::read(path).map_err(|err| Error::Io {
            path: path.to_path_buf(),
            err,
        })?;
        let img = Image::try_from(bytes.as_slice()).map_err(|err| Error::Image {
            path: path.to_path_buf(),
            err,
        })?;

        // Remove default view if it hasn't been modified.
        if let Some(active) = self.views.get(&self.active) {
            if !active.modified && active.path.is_none() {
                self.views.remove(&self.active);
            }
        }

        let extent = ViewExtent {
            fw: img.size.w,
            fh: img.size.h,
            nframes: 1,
        };
        self.load(path, img, extent)?;

        Ok(extent.size().area() as usize)
    }

    pub fn load(&mut self, path: &Path, image: Image, extent: ViewExtent) -> Result<ViewId, Error> {
        // Check if view is already loaded.
        if let Some(id) = self
            .views
            .values()
            .find(|v| v.path.as_ref().map_or(false, |p| p.as_path() == path))
            .map(|v| v.id)
        {
            self.activate(id);

            return Ok(id);
        }
        assert_eq!(image.size, extent.size());

        let id = ViewId::next();
        let view = View::new(id, extent, image)?.path(path.to_path_buf());

        self.views.insert(id, view);
        self.activate(id);

        Ok(id)
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub struct SnapshotId(usize);

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub struct Snapshot {
    pub extent: ViewExtent,

    pixels: Compressed<Rgba8>,
}

impl Snapshot {
    pub fn new(pixels: Arc<[Rgba8]>, extent: ViewExtent) -> Self {
        let size = pixels.len();
        let pixels = Compressed::from(&pixels);

        debug_assert!(
            (extent.fw * extent.fh) as usize * extent.nframes == size,
            "the pixel buffer has the expected size"
        );

        Self { extent, pixels }
    }

    pub fn width(&self) -> u32 {
        self.extent.width()
    }

    pub fn height(&self) -> u32 {
        self.extent.height()
    }

    ////////////////////////////////////////////////////////////////////////////

    fn image(&self) -> Image {
        let bytes = self
            .pixels
            .decompress()
            .expect("decompressing a snapshot should succeed");

        Image::new(Rgba8::align(&bytes), self.extent.size())
    }
}

#[derive(Debug)]
pub struct Compressed<T> {
    data: Box<[u8]>,
    witness: PhantomData<T>,
}

impl Compressed<Rgba8> {
    fn from(input: &[Rgba8]) -> Self {
        let bytes = Rgba8::bytes(input);
        let compressed = lz4_flex::compress_prepend_size(bytes);

        Self {
            data: compressed.into_boxed_slice(),
            witness: PhantomData,
        }
    }

    fn decompress(&self) -> Result<Vec<u8>, lz4_flex::block::DecompressError> {
        lz4_flex::decompress_size_prepended(&self.data)
    }
}
