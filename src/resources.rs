use crate::image;
use crate::session::Rgb8;
use crate::view::layer::LayerId;
use crate::view::{ViewExtent, ViewId};

use nonempty::NonEmpty;
use rgx::color::{Bgra8, Rgba8};
use rgx::rect::Rect;

use gif::{self, SetParameter};

use serde_derive::{Deserialize, Serialize};

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time;

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub extent: ViewExtent,
}

#[derive(Debug)]
pub struct Archive {
    pub layers: Vec<Vec<Vec<Rgba8>>>,
    pub manifest: Manifest,
}

#[derive(Debug, Copy, Clone)]
enum PixelFormat {
    Rgba8,
    Bgra8,
}

#[derive(Debug, Clone)]
pub struct Pixels {
    format: PixelFormat,
    buf: Box<[u32]>,
}

impl Pixels {
    pub fn blank(w: usize, h: usize) -> Self {
        let buf = vec![Rgba8::TRANSPARENT; w * h];
        Pixels::from_rgba8(buf.into())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn from_rgba8(buf: Box<[Rgba8]>) -> Self {
        let buf = unsafe { std::mem::transmute(buf) };
        Self {
            format: PixelFormat::Rgba8,
            buf,
        }
    }

    pub fn from_bgra8(buf: Box<[Bgra8]>) -> Self {
        let buf = unsafe { std::mem::transmute(buf) };
        Self {
            format: PixelFormat::Bgra8,
            buf,
        }
    }

    pub fn slice(&self, r: core::ops::Range<usize>) -> Vec<Rgba8> {
        match self.format {
            PixelFormat::Bgra8 => {
                let slice = &self.buf[r];
                slice.iter().map(|u| Bgra8::from(*u).into()).collect()
            }
            PixelFormat::Rgba8 => Rgba8::align(&self.buf[r]).to_vec(),
        }
    }

    pub fn get(&self, idx: usize) -> Option<Rgba8> {
        match self.format {
            PixelFormat::Rgba8 => self.buf.get(idx).cloned().map(Rgba8::from),
            PixelFormat::Bgra8 => self.buf.get(idx).cloned().map(|u| Bgra8::from(u).into()),
        }
    }

    pub fn into_rgba8(self) -> Vec<Rgba8> {
        match self.format {
            PixelFormat::Rgba8 => Rgba8::align(&self.buf).to_vec(),
            PixelFormat::Bgra8 => self
                .buf
                .iter()
                .cloned()
                .map(|u| Bgra8::from(u).into())
                .collect(),
        }
    }

    pub fn into_bgra8(self) -> Vec<Bgra8> {
        match self.format {
            PixelFormat::Rgba8 => self
                .buf
                .iter()
                .cloned()
                .map(|u| Rgba8::from(u).into())
                .collect(),
            PixelFormat::Bgra8 => Bgra8::align(&self.buf).to_vec(),
        }
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item = Rgba8> + 'a {
        self.buf.iter().cloned().map(move |u| match self.format {
            PixelFormat::Rgba8 => Rgba8::from(u),
            PixelFormat::Bgra8 => Bgra8::from(u).into(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        let (head, body, tail) = unsafe { self.buf.align_to::<u8>() };
        assert!(head.is_empty() && tail.is_empty());
        body
    }

    pub fn as_rgba8(&self) -> Option<&[Rgba8]> {
        match self.format {
            PixelFormat::Rgba8 => Some(Rgba8::align(&self.buf)),
            PixelFormat::Bgra8 => None,
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

    pub fn get_snapshot_safe(&self, id: ViewId, layer_id: LayerId) -> Option<(&Snapshot, &Pixels)> {
        self.data
            .get(&id)
            .and_then(|v| v.current_snapshot(layer_id))
    }

    pub fn get_snapshot(&self, id: ViewId, layer_id: LayerId) -> (&Snapshot, &Pixels) {
        self.get_snapshot_safe(id, layer_id).expect(&format!(
            "layer #{} of view #{} must exist and have an associated snapshot",
            layer_id, id
        ))
    }

    pub fn get_snapshot_id(&self, id: ViewId, layer_id: LayerId) -> Option<SnapshotId> {
        self.data
            .get(&id)
            .and_then(|v| v.layers.get(&layer_id))
            .map(|l| SnapshotId(l.snapshot))
    }

    pub fn get_snapshot_mut(&mut self, id: ViewId, layer_id: LayerId) -> (&mut Snapshot, &Pixels) {
        self.data
            .get_mut(&id)
            .and_then(|v| v.layers.get_mut(&layer_id))
            .map(|l| l.current_snapshot_mut())
            .expect(&format!(
                "layer #{} of view #{} must exist and have an associated snapshot",
                layer_id, id
            ))
    }

    pub fn get_snapshot_rect(
        &self,
        id: ViewId,
        layer_id: LayerId,
        rect: &Rect<i32>,
    ) -> Option<(&Snapshot, Vec<Rgba8>)> {
        self.data
            .get(&id)
            .and_then(|v| v.layers.get(&layer_id))
            .expect(&format!(
                "view #{} with layer #{} must exist and have an associated snapshot",
                id, layer_id
            ))
            .get_snapshot_rect(rect)
    }

    pub fn get_view(&self, id: ViewId) -> Option<&ViewResources> {
        self.data.get(&id)
    }

    pub fn get_view_mut(&mut self, id: ViewId) -> Option<&mut ViewResources> {
        self.data.get_mut(&id)
    }

    pub fn current_edit(&self, id: ViewId) -> EditId {
        self.get_view(id)
            .expect(&format!("view #{} must exist", id))
            .cursor
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

    pub fn load_image<P: AsRef<Path>>(path: P) -> io::Result<(u32, u32, Vec<Rgba8>)> {
        let (buffer, width, height) = image::load(path)?;
        let pixels = Rgba8::align(&buffer);

        // TODO: (perf) Avoid the copy?

        Ok((width, height, pixels.into()))
    }

    pub fn load_archive<P: AsRef<Path>>(path: P) -> io::Result<Archive> {
        use std::io::Read;
        use zip::result::ZipError;

        let file = File::open(&path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let files: Vec<_> = archive.file_names().map(|f| PathBuf::from(f)).collect();

        // Get the root directory name of the archive.
        let root = files
            .first()
            .and_then(|f| f.iter().next())
            .map(Path::new)
            .ok_or(io::Error::new(io::ErrorKind::Other, "invalid archive"))?;

        // Decode the manifest file.
        let manifest: Manifest = {
            let mut buf = String::new();

            archive
                .by_name(&root.join("manifest.toml").to_string_lossy())?
                .read_to_string(&mut buf)?;
            toml::from_str(&buf)?
        };

        let mut layers = Vec::new();

        // Discover the layers and frames in the archive.
        for layer in 0.. {
            let path = root.join("layers").join(layer.to_string()).join("frames");

            if !files.iter().any(|f| f.starts_with(&path)) {
                break;
            }

            let mut frames = Vec::new();
            for frame in 0.. {
                let path = path.join(frame.to_string()).with_extension("png");

                match archive.by_name(&path.to_string_lossy()) {
                    Ok(mut file) => {
                        let mut img = Vec::new();
                        file.read_to_end(&mut img)?;

                        let (pixels, w, h) = image::read(img.as_slice())?;
                        debug_assert!(w == manifest.extent.fw && h == manifest.extent.fh);

                        frames.push(unsafe { std::mem::transmute(pixels) });
                    }
                    Err(ZipError::FileNotFound) => {
                        break;
                    }
                    Err(err) => return Err(err.into()),
                }
            }
            layers.push(frames);
        }

        Ok(Archive { layers, manifest })
    }

    pub fn save_layer<P: AsRef<Path>>(
        &self,
        id: ViewId,
        layer_id: LayerId,
        rect: Rect<u32>,
        path: P,
    ) -> io::Result<(EditId, usize)> {
        let resources = self.lock();
        let (_, pixels) = resources
            .get_snapshot_rect(id, layer_id, &rect.map(|n| n as i32))
            .expect("rect should be within view");
        let (w, h) = (rect.width(), rect.height());
        let edit_id = resources.current_edit(id);

        image::save(path, w, h, &pixels)?;

        Ok((edit_id, (w * h) as usize))
    }

    pub fn save_view_archive<P: AsRef<Path>>(&self, id: ViewId, path: P) -> io::Result<usize> {
        use std::io::Write;
        use zip::write::FileOptions;

        let f = File::create(path.as_ref())?;
        let out = &mut io::BufWriter::new(f);
        let mut zip = zip::ZipWriter::new(out);
        let opts = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        let resources = self.lock();
        let view = resources.get_view(id).expect("the view must exist");
        let extent = view.extent;
        let mut buffer = Vec::new();
        let mut written = 0;

        let name = path
            .as_ref()
            .file_stem()
            .expect("the file must have a stem");

        let manifest = toml::to_string(&Manifest { extent })
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        zip.start_file_from_path(
            &Path::new(name).join("manifest.toml"),
            FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o644),
        )?;
        zip.write_all(manifest.as_bytes())?;

        for (id, layer) in view.layers.iter() {
            let path = Path::new(name).join("layers").join(id.to_string());

            for i in 0..extent.nframes {
                let rect = &extent.frame(i);
                let (_, pixels) = layer
                    .get_snapshot_rect(&rect.map(|n| n as i32))
                    .expect("the rect is within the view");

                buffer.clear();
                image::write(&mut buffer, rect.width(), rect.height(), &pixels)?;

                let path = path
                    .join("frames")
                    .join(i.to_string())
                    .with_extension("png");

                zip.start_file_from_path(&path, opts)?;
                zip.write_all(&buffer)?;

                written += pixels.len() * std::mem::size_of::<Rgba8>();
            }
        }

        zip.finish()?;

        Ok(written)
    }

    pub fn save_view_svg<P: AsRef<Path>>(
        &self,
        id: ViewId,
        layer_id: LayerId,
        path: P,
    ) -> io::Result<usize> {
        use std::io::Write;

        let resources = self.lock();
        let (snapshot, pixels) = resources.get_snapshot(id, layer_id);
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
        layer_id: LayerId,
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
        let (snapshot, pixels) = resources.get_snapshot_mut(id, layer_id);
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

    pub fn add_view(&mut self, id: ViewId, extent: ViewExtent, pixels: Pixels) {
        self.resources
            .borrow_mut()
            .data
            .insert(id, ViewResources::new(pixels, extent));
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    LayerPainted(LayerId),
    LayerAdded(LayerId),
    ViewResized(Vec<LayerId>, ViewExtent, ViewExtent),
    ViewPainted(Vec<LayerId>),
    Initial,
}

pub type EditId = usize;

#[derive(Debug)]
pub struct ViewResources {
    layers: BTreeMap<LayerId, LayerResources>,

    pub history: NonEmpty<Edit>,
    pub cursor: usize,
    pub extent: ViewExtent,
}

impl ViewResources {
    fn new(pixels: Pixels, extent: ViewExtent) -> Self {
        use std::iter::FromIterator;

        Self {
            layers: BTreeMap::from_iter(
                vec![(LayerId::default(), LayerResources::new(pixels, extent))].drain(..),
            ),
            history: NonEmpty::new(Edit::Initial),
            cursor: 0,
            extent,
        }
    }

    pub fn layer(&self, layer: LayerId) -> &LayerResources {
        self.layers
            .get(&layer)
            .expect(&format!("layer #{} should exist", layer))
    }

    pub fn layer_mut(&mut self, layer: LayerId) -> &mut LayerResources {
        self.layers
            .get_mut(&layer)
            .expect(&format!("layer #{} should exist", layer))
    }

    pub fn layers(&self) -> impl Iterator<Item = (&LayerId, &LayerResources)> + '_ {
        self.layers.iter().filter(|(_, l)| !l.hidden)
    }

    pub fn add_layer(&mut self, layer_id: LayerId, extent: ViewExtent, pixels: Pixels) {
        self.layers
            .insert(layer_id, LayerResources::new(pixels, extent));
        self.history_record(Edit::LayerAdded(layer_id));
    }

    pub fn record_view_resized(&mut self, layers: Vec<(LayerId, Pixels)>, extent: ViewExtent) {
        self.history_record(Edit::ViewResized(
            layers.iter().map(|(l, _)| *l).collect(),
            self.extent,
            extent,
        ));
        self.extent = extent;

        for (id, pixels) in layers.into_iter() {
            self.layer_mut(id).push_snapshot(pixels, extent);
        }
    }

    pub fn record_view_painted(&mut self, layers: Vec<(LayerId, Pixels)>) {
        let extent = self.extent.clone();
        self.history_record(Edit::ViewPainted(layers.iter().map(|(l, _)| *l).collect()));

        for (id, pixels) in layers.into_iter() {
            self.layer_mut(id).push_snapshot(pixels, extent);
        }
    }

    pub fn record_layer_painted(&mut self, layer: LayerId, pixels: Pixels, extent: ViewExtent) {
        self.history_record(Edit::LayerPainted(layer));
        self.layer_mut(layer).push_snapshot(pixels, extent);
    }

    pub fn history_truncate(&mut self) {
        if self.cursor != self.history.len() - 1 {
            self.history.truncate(self.cursor + 1);
            self.cursor = self.history.len() - 1;
        }
    }

    pub fn history_record(&mut self, edit: Edit) {
        debug!("edit: {:?}", edit);

        // If we try to add an edit when we're not at the
        // latest, we have to clear the list forward.
        self.history_truncate();
        self.cursor += 1;

        self.history.push(edit);
    }

    pub fn current_snapshot(&self, layer: LayerId) -> Option<(&Snapshot, &Pixels)> {
        self.layers.get(&layer).map(|l| l.current_snapshot())
    }

    pub fn history_prev(&mut self) -> Option<(usize, Edit)> {
        if self.cursor == 0 {
            return None;
        }

        if let Some(edit) = self.history.get(self.cursor).cloned() {
            match edit {
                Edit::LayerPainted(id) => {
                    self.layer_mut(id).prev_snapshot();
                }
                Edit::LayerAdded(id) => {
                    self.layer_mut(id).hidden = true;
                }
                Edit::ViewResized(ref layers, from, _) => {
                    self.extent = from;

                    for id in layers.iter() {
                        self.layer_mut(*id).prev_snapshot();
                    }
                }
                Edit::ViewPainted(ref layers) => {
                    for id in layers.iter() {
                        self.layer_mut(*id).prev_snapshot();
                    }
                }
                _ => return None,
            }
            self.cursor -= 1;

            Some((self.cursor, edit))
        } else {
            None
        }
    }

    pub fn history_next(&mut self) -> Option<(usize, Edit)> {
        if let Some(edit) = self.history.get(self.cursor + 1).cloned() {
            self.cursor += 1;

            match edit {
                Edit::LayerPainted(id) => {
                    self.layer_mut(id).next_snapshot();
                }
                Edit::LayerAdded(id) => {
                    self.layer_mut(id).hidden = false;
                }
                Edit::ViewResized(ref layers, _, to) => {
                    self.extent = to;

                    for id in layers.iter() {
                        self.layer_mut(*id).next_snapshot();
                    }
                }
                Edit::ViewPainted(ref layers) => {
                    for id in layers.iter() {
                        self.layer_mut(*id).next_snapshot();
                    }
                }
                _ => return None,
            }
            Some((self.cursor, edit))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct LayerResources {
    /// Non empty list of view snapshots.
    snapshots: NonEmpty<Snapshot>,
    /// Current layer snapshot.
    snapshot: usize,
    /// Current layer pixels. We keep a separate decompressed
    /// cache of the view pixels for performance reasons.
    pixels: Pixels,
    /// Whether this layer should be hidden.
    hidden: bool,
}

impl LayerResources {
    fn new(pixels: Pixels, extent: ViewExtent) -> Self {
        Self {
            snapshots: NonEmpty::new(Snapshot::new(SnapshotId(0), pixels.clone(), extent)),
            snapshot: 0,
            pixels,
            hidden: false,
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

    pub fn get_snapshot_rect(&self, rect: &Rect<i32>) -> Option<(&Snapshot, Vec<Rgba8>)> {
        let (snapshot, pixels) = self.current_snapshot();
        let snapshot_rect = snapshot.extent.rect().map(|n| n as i32);

        // Fast path.
        if snapshot_rect == *rect {
            return Some((snapshot, pixels.clone().into_rgba8()));
        }

        let w = rect.width() as usize;
        let h = rect.height() as usize;

        let total_w = snapshot.width() as usize;
        let total_h = snapshot.height() as usize;

        if !(snapshot_rect.x1 <= rect.x1 && snapshot_rect.y1 <= rect.y1)
            || !(snapshot_rect.x2 >= rect.x2 && snapshot_rect.y2 >= rect.y2)
        {
            return None;
        }
        debug_assert!(w * h <= total_w * total_h);

        let mut buffer: Vec<Rgba8> = Vec::with_capacity(w * h);

        for y in (rect.y1 as usize..rect.y2 as usize).rev() {
            let y = total_h - y - 1;
            let offset = y * total_w + rect.x1 as usize;
            let row = &pixels.slice(offset..offset + w);

            buffer.extend_from_slice(row);
        }
        assert!(buffer.len() == w * h);

        Some((snapshot, buffer))
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
            self.pixels = snapshot.pixels();

            Some(snapshot)
        } else {
            None
        }
    }

    pub fn next_snapshot(&mut self) -> Option<&Snapshot> {
        if let Some(snapshot) = self.snapshots.get(self.snapshot + 1) {
            self.snapshot += 1;
            self.pixels = snapshot.pixels();

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

    format: PixelFormat,
}

impl Snapshot {
    pub fn new(id: SnapshotId, pixels: Pixels, extent: ViewExtent) -> Self {
        let size = pixels.len();
        let format = pixels.format;
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
            PixelFormat::Rgba8 => Pixels::from_rgba8(Rgba8::align(&bytes).into()),
            PixelFormat::Bgra8 => Pixels::from_bgra8(Bgra8::align(&bytes).into()),
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
