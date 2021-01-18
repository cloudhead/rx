use crate::image;
use crate::session::Rgb8;
use crate::view::layer::{LayerCoords, LayerId};
use crate::view::ViewExtent;

use super::pixels::{PixelFormat, Pixels};

use nonempty::NonEmpty;
use rgx::color::{Bgra8, Rgba8};
use rgx::rect::Rect;

use gif::{self, SetParameter};

use miniserde::json;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time;

#[derive(Debug)]
pub struct ViewResource {
    pub layers: BTreeMap<LayerId, LayerResource>,
    pub history: NonEmpty<Edit>,
    pub cursor: usize,
    pub extent: ViewExtent,
}

impl ViewResource {
    pub fn new(pixels: Pixels, extent: ViewExtent) -> Self {
        use std::iter::FromIterator;

        Self {
            layers: BTreeMap::from_iter(
                vec![(Default::default(), LayerResource::new(pixels, extent))].drain(..),
            ),
            history: NonEmpty::new(Edit::Initial),
            cursor: 0,
            extent,
        }
    }

    pub fn layer(&self, layer: LayerId) -> &LayerResource {
        self.layers
            .get(&layer)
            .expect(&format!("layer #{} should exist", layer))
    }

    pub fn layer_mut(&mut self, layer: LayerId) -> &mut LayerResource {
        self.layers
            .get_mut(&layer)
            .expect(&format!("layer #{} should exist", layer))
    }

    pub fn layers(&self) -> impl Iterator<Item = (&LayerId, &LayerResource)> + '_ {
        self.layers.iter().filter(|(_, l)| !l.hidden)
    }

    pub fn add_layer(&mut self, layer_id: LayerId, extent: ViewExtent, pixels: Pixels) {
        self.layers
            .insert(layer_id, LayerResource::new(pixels, extent));
        self.history_record(Edit::LayerAdded(layer_id));
    }

    pub fn save_layer<P: AsRef<Path>>(
        &self,
        layer_id: LayerId,
        rect: Rect<u32>,
        path: P,
    ) -> io::Result<(EditId, usize)> {
        let (_, pixels) = self
            .layer(layer_id)
            .get_snapshot_rect(&rect.map(|n| n as i32))
            .expect("rect should be within view");
        let (w, h) = (rect.width(), rect.height());

        image::save_as(path, w, h, &pixels)?;

        Ok((self.cursor, (w * h) as usize))
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

    pub fn current_edit(&self) -> EditId {
        self.cursor
    }

    pub fn save_archive<P: AsRef<Path>>(&self, path: P) -> io::Result<usize> {
        use std::io::Write;
        use zip::write::FileOptions;

        let f = File::create(path.as_ref())?;
        let out = &mut io::BufWriter::new(f);
        let mut zip = zip::ZipWriter::new(out);
        let opts = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        let extent = self.extent;
        let mut buffer = Vec::new();
        let mut written = 0;

        let name = path
            .as_ref()
            .file_stem()
            .expect("the file must have a stem");

        let manifest = json::to_string(&crate::io::Manifest { extent });

        zip.start_file_from_path(
            &Path::new(name).join("manifest.json"),
            FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored)
                .unix_permissions(0o644),
        )?;
        zip.write_all(manifest.as_bytes())?;

        for (id, layer) in self.layers.iter() {
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

    pub fn save_svg<P: AsRef<Path>>(&self, layer_id: LayerId, path: P) -> io::Result<usize> {
        use std::io::Write;

        let (snapshot, pixels) = self
            .current_snapshot(layer_id)
            .ok_or(io::ErrorKind::InvalidInput)?;
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

    pub fn save_gif<P: AsRef<Path>>(
        &self,
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

        let (snapshot, pixels) = self
            .current_snapshot(layer_id)
            .ok_or(io::ErrorKind::InvalidInput)?;
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
}

#[derive(Debug)]
pub struct LayerResource {
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

impl LayerResource {
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

#[derive(Debug, Clone)]
pub enum Edit {
    LayerPainted(LayerId),
    LayerAdded(LayerId),
    ViewResized(Vec<LayerId>, ViewExtent, ViewExtent),
    ViewPainted(Vec<LayerId>),
    Initial,
}

pub type EditId = usize;

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
    pub fn layer_coord_to_index(&self, p: LayerCoords<u32>) -> Option<usize> {
        self.height()
            .checked_sub(p.y)
            .and_then(|x| x.checked_sub(1))
            .map(|y| (y * self.width() + p.x) as usize)
    }
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
