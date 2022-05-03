use crate::gfx::color::{Rgb8, Rgba8};
use crate::gfx::rect::Rect;
use crate::image;
use crate::pixels;
use crate::util;
use crate::view::{ViewCoords, ViewExtent};

use nonempty::NonEmpty;

use gif::{self, SetParameter};

use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time;

#[derive(Debug)]
pub struct ViewResource {
    pub layer: LayerResource,
    pub history: NonEmpty<Edit>,
    pub cursor: usize,
    pub extent: ViewExtent,
}

impl ViewResource {
    pub fn new(pixels: Vec<Rgba8>, extent: ViewExtent) -> Self {
        Self {
            layer: LayerResource::new(pixels, extent),
            history: NonEmpty::new(Edit::Initial),
            cursor: 0,
            extent,
        }
    }

    pub fn save<P: AsRef<Path>>(&self, rect: Rect<u32>, path: P) -> io::Result<(EditId, usize)> {
        let (_, pixels) = self
            .layer
            .get_snapshot_rect(&rect.map(|n| n as i32))
            .expect("rect should be within view");
        let (w, h) = (rect.width(), rect.height());

        image::save_as(path, w, h, 1, &pixels)?;

        Ok((self.cursor, (w * h) as usize))
    }

    pub fn record_view_resized(&mut self, pixels: Vec<Rgba8>, extent: ViewExtent) {
        self.history_record(Edit::ViewResized(self.extent, extent));
        self.extent = extent;
        self.layer.push_snapshot(pixels, extent);
    }

    pub fn record_view_painted(&mut self, pixels: Vec<Rgba8>) {
        let extent = self.extent;
        self.history_record(Edit::ViewPainted);
        self.layer.push_snapshot(pixels, extent);
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

    pub fn history_prev(&mut self) -> Option<(usize, Edit)> {
        if self.cursor == 0 {
            return None;
        }

        if let Some(edit) = self.history.get(self.cursor).cloned() {
            match edit {
                Edit::ViewResized(from, _) => {
                    self.extent = from;
                    self.layer.prev_snapshot();
                }
                Edit::ViewPainted => {
                    self.layer.prev_snapshot();
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
                Edit::ViewResized(_, to) => {
                    self.extent = to;
                    self.layer.next_snapshot();
                }
                Edit::ViewPainted => {
                    self.layer.next_snapshot();
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

    pub fn save_png<P: AsRef<Path>>(&self, path: P, scale: u32) -> io::Result<usize> {
        let (snapshot, pixels) = self.layer.current_snapshot();
        let (w, h) = (snapshot.width(), snapshot.height());

        image::save_as(path, w, h, scale, pixels)?;

        Ok((w * h * scale) as usize)
    }

    pub fn save_svg<P: AsRef<Path>>(&self, path: P, scale: u32) -> io::Result<usize> {
        use std::io::Write;

        let (snapshot, pixels) = self.layer.current_snapshot();
        let (w, h) = (snapshot.width(), snapshot.height());

        let f = File::create(path.as_ref())?;
        let out = &mut io::BufWriter::new(f);

        writeln!(
            out,
            r#"<svg width="{}" height="{}" viewBox="0 0 {} {}" fill="none" xmlns="http://www.w3.org/2000/svg">"#,
            w * scale,
            h * scale,
            w * scale,
            h * scale,
        )?;

        for (i, rgba) in pixels.iter().cloned().enumerate().filter(|(_, c)| c.a > 0) {
            let rgb: Rgb8 = rgba.into();

            let x = (i as u32 % w) * scale;
            let y = (i as u32 / h) * scale;

            writeln!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                x, y, scale, scale, rgb
            )?;
        }

        writeln!(out, "</svg>")?;

        Ok((w * h * scale) as usize)
    }

    pub fn save_gif<P: AsRef<Path>>(
        &self,
        path: P,
        frame_delay: time::Duration,
        palette: &[Rgba8],
        scale: u32,
    ) -> io::Result<usize> {
        assert!(scale >= 1);

        // The gif encoder expects the frame delay in units of 10ms.
        let frame_delay = frame_delay.as_millis() / 10;
        // If the passed in delay is larger than a `u16` can hold,
        // we ensure it doesn't overflow.
        let frame_delay = u128::min(frame_delay, u16::max_value() as u128) as u16;

        let (snapshot, pixels) = self.layer.current_snapshot();
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

        // Convert RGBA pixels into indexed pixels.
        let mut image: Vec<u8> = Vec::with_capacity(snapshot.size);
        for rgba in pixels {
            if let Ok(index) = palette.binary_search(rgba) {
                image.push(index as u8);
            } else {
                image.push(transparent);
            }
        }
        if scale > 1 {
            image = pixels::scale(&image, extent.width(), extent.height(), scale);
        }

        let (fw, fh) = ((extent.fw * scale) as usize, (extent.fh * scale) as usize);
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
        let palette = util::align_u8(&palette);

        let mut f = File::create(path.as_ref())?;
        let mut encoder = gif::Encoder::new(&mut f, fw as u16, fh as u16, palette)?;
        encoder.set(gif::Repeat::Infinite)?;

        for frame in frames.iter_mut() {
            let mut frame =
                gif::Frame::from_indexed_pixels(fw as u16, fh as u16, frame, Some(transparent));
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
    pixels: Vec<Rgba8>,
}

impl LayerResource {
    fn new(pixels: Vec<Rgba8>, extent: ViewExtent) -> Self {
        Self {
            snapshots: NonEmpty::new(Snapshot::new(SnapshotId(0), &pixels, extent)),
            snapshot: 0,
            pixels,
        }
    }

    pub fn current_snapshot(&self) -> (&Snapshot, &[Rgba8]) {
        (
            self.snapshots
                .get(self.snapshot)
                .expect("there must always be a current snapshot"),
            &self.pixels,
        )
    }

    pub fn current_snapshot_mut(&mut self) -> (&mut Snapshot, &[Rgba8]) {
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
            return Some((snapshot, pixels.into()));
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
            let row = &pixels[offset..offset + w];

            buffer.extend_from_slice(row);
        }
        assert!(buffer.len() == w * h);

        Some((snapshot, buffer))
    }

    pub fn push_snapshot(&mut self, pixels: Vec<Rgba8>, extent: ViewExtent) {
        // FIXME: If pixels match current snapshot exactly, don't add the snapshot.

        // If we try to add a snapshot when we're not at the
        // latest, we have to clear the list forward.
        if self.snapshot != self.snapshots.len() - 1 {
            self.snapshots.truncate(self.snapshot + 1);
            self.snapshot = self.snapshots.len() - 1;
        }
        self.snapshot += 1;
        self.snapshots
            .push(Snapshot::new(SnapshotId(self.snapshot), &pixels, extent));
        self.pixels = pixels;
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
    ViewResized(ViewExtent, ViewExtent),
    ViewPainted,
    Initial,
}

pub type EditId = usize;

#[derive(Debug, PartialEq, Eq, Copy, Clone, Default)]
pub struct SnapshotId(usize);

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
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
    pub fn new(id: SnapshotId, pixels: &[Rgba8], extent: ViewExtent) -> Self {
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
        }
    }

    pub fn coord_to_index(&self, p: ViewCoords<u32>) -> Option<usize> {
        self.height()
            .checked_sub(p.y)
            .and_then(|x| x.checked_sub(1))
            .map(|y| (y * self.width() + p.x) as usize)
    }

    pub fn width(&self) -> u32 {
        self.extent.fw * self.extent.nframes as u32
    }

    pub fn height(&self) -> u32 {
        self.extent.fh
    }

    ////////////////////////////////////////////////////////////////////////////

    fn pixels(&self) -> Vec<Rgba8> {
        let bytes = self
            .pixels
            .decompress()
            .expect("decompressing snapshot shouldn't result in an error");
        Rgba8::align(&bytes).into()
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct Compressed<T>(T);

impl Compressed<Box<[u8]>> {
    fn from(input: &[Rgba8]) -> snap::Result<Self> {
        let mut enc = snap::Encoder::new();
        let bytes = util::align_u8(input);
        enc.compress_vec(bytes).map(|v| Self(v.into_boxed_slice()))
    }

    fn decompress(&self) -> snap::Result<Vec<u8>> {
        let mut dec = snap::Decoder::new();
        dec.decompress_vec(&self.0)
    }
}
