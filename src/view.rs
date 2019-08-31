use crate::resources::SnapshotId;

use cgmath::prelude::*;
use cgmath::{Point2, Vector2};

use rgx::core::Rect;
use rgx::kit::Animation;

use std::fmt;
use std::path::PathBuf;
use std::time;

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct ViewId(pub u16);

impl fmt::Display for ViewId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[allow(dead_code)]
pub enum Error {
    FileError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewState {
    /// The view is okay. It doesn't need to be redrawn or saved.
    Okay,
    /// The view has been touched, the changes need to be stored in a snapshot.
    Dirty,
    /// The view is damaged, it needs to be redrawn from a snapshot.
    /// This happens when undo/redo is used.
    Damaged,
}

#[derive(Debug, Clone, Copy)]
pub enum ViewOp {
    Blit(Rect<f32>, Rect<f32>),
}

#[derive(Debug)]
pub struct View {
    pub fw: u32,
    pub fh: u32,
    pub offset: Vector2<f32>,
    pub id: ViewId,
    pub zoom: f32,
    pub ops: Vec<ViewOp>,

    pub flip_x: bool,
    pub flip_y: bool,
    pub hover: bool,

    pub file_status: FileStatus,
    pub state: ViewState,

    pub animation: Animation<Rect<f32>>,

    saved_snapshot: Option<SnapshotId>,
}

impl View {
    const DEFAULT_ANIMATION_DELAY: u64 = 160;

    pub fn new(id: ViewId, fs: FileStatus, fw: u32, fh: u32) -> Self {
        let saved_snapshot = if let FileStatus::Saved(_) = &fs {
            Some(SnapshotId::default())
        } else {
            None
        };
        Self {
            id,
            fw,
            fh,
            offset: Vector2::zero(),
            zoom: 1.,
            ops: Vec::new(),
            flip_x: false,
            flip_y: false,
            hover: false,
            file_status: fs,
            animation: Animation::new(
                &[Rect::origin(fw as f32, fh as f32)],
                time::Duration::from_millis(Self::DEFAULT_ANIMATION_DELAY),
            ),
            state: ViewState::Okay,
            saved_snapshot,
        }
    }

    pub fn width(&self) -> u32 {
        self.fw * self.animation.len() as u32
    }

    pub fn height(&self) -> u32 {
        self.fh
    }

    pub fn file_name(&self) -> Option<&PathBuf> {
        match self.file_status {
            FileStatus::New(ref f) => Some(f),
            FileStatus::Modified(ref f) => Some(f),
            FileStatus::Saved(ref f) => Some(f),
            FileStatus::NoFile => None,
        }
    }

    pub fn save_as(&mut self, id: SnapshotId, path: PathBuf) {
        match self.file_status {
            FileStatus::Modified(ref curr_path)
            | FileStatus::New(ref curr_path) => {
                if curr_path == &path {
                    self.saved(id, path);
                }
            }
            FileStatus::NoFile => {
                self.saved(id, path);
            }
            FileStatus::Saved(_) => {}
        }
    }

    pub fn extend(&mut self) {
        let w = self.width() as f32;
        let fw = self.fw as f32;
        let fh = self.fh as f32;

        self.animation.push_frame(Rect::new(w, 0., w + fw, fh));

        self.touch();
    }

    pub fn extend_clone(&mut self, index: i32) {
        let width = self.width() as f32;
        let (fw, fh) = (self.fw as f32, self.fh as f32);

        let index = if index == -1 {
            self.animation.len() - 1
        } else {
            index as usize
        };

        self.ops.push(ViewOp::Blit(
            Rect::new(fw * index as f32, 0., fw * (index + 1) as f32, fh),
            Rect::new(width, 0., width + fw, fh),
        ));
        self.extend();
    }

    pub fn resize_frame(&mut self, fw: u32, fh: u32) {
        self.resize(fw, fh, self.animation.len());
    }

    pub fn resize(&mut self, fw: u32, fh: u32, nframes: usize) {
        self.fw = fw;
        self.fh = fh;

        let mut frames = Vec::new();
        let origin = Rect::origin(self.fw as f32, self.fh as f32);

        for i in 0..nframes {
            frames.push(origin + Vector2::new(i as f32 * self.fw as f32, 0.));
        }
        self.animation = Animation::new(&frames, self.animation.delay);
    }

    pub fn slice(&mut self, nframes: usize) -> bool {
        if self.width() % nframes as u32 == 0 {
            let fw = self.width() / nframes as u32;
            self.resize(fw, self.fh, nframes);
            return true;
        }
        false
    }

    pub fn shrink(&mut self) {
        // Don't allow the view to have zero frames.
        if self.animation.len() > 1 {
            self.animation.pop_frame();
            self.touch();
        }
    }

    #[allow(dead_code)]
    pub fn play_animation(&mut self) {
        self.animation.play();
    }

    #[allow(dead_code)]
    pub fn pause_animation(&mut self) {
        self.animation.pause();
    }

    #[allow(dead_code)]
    pub fn stop_animation(&mut self) {
        self.animation.stop();
    }

    pub fn set_animation_delay(&mut self, ms: u64) {
        self.animation.delay = time::Duration::from_millis(ms);
    }

    pub fn okay(&mut self) {
        self.state = ViewState::Okay;
        self.ops.clear();
    }

    pub fn frame(&mut self, delta: time::Duration) {
        self.animation.step(delta);
    }

    pub fn rect(&self) -> Rect<f32> {
        Rect::new(
            self.offset.x,
            self.offset.y,
            self.offset.x + self.width() as f32 * self.zoom,
            self.offset.y + self.height() as f32 * self.zoom,
        )
    }

    pub fn contains(&self, p: Point2<f32>) -> bool {
        self.rect().contains(p)
    }

    /// View has been modified. Called when using the brush on the view,
    /// or resizing the view.
    pub fn touch(&mut self) {
        if let FileStatus::Saved(ref f) = self.file_status {
            self.file_status = FileStatus::Modified(f.clone());
        }
        self.state = ViewState::Dirty;
    }

    /// View should be considered damaged and needs to be restored from snapshot.
    /// Used when undoing or redoing changes.
    pub fn damaged(&mut self) {
        self.state = ViewState::Damaged;
    }

    pub fn is_damaged(&self) -> bool {
        self.state == ViewState::Damaged
    }

    pub fn is_dirty(&self) -> bool {
        self.state == ViewState::Dirty
    }

    pub fn is_okay(&self) -> bool {
        self.state == ViewState::Okay
    }

    pub fn status(&self) -> String {
        self.file_status.to_string()
    }

    pub fn is_snapshot_saved(&self, id: SnapshotId) -> bool {
        self.saved_snapshot == Some(id)
    }

    pub fn handle_cursor_moved(&mut self, cursor: Point2<f32>) {
        self.hover = self.contains(cursor);
    }

    ////////////////////////////////////////////////////////////////////////////

    fn saved(&mut self, id: SnapshotId, path: PathBuf) {
        self.file_status = FileStatus::Saved(path);
        self.saved_snapshot = Some(id);
    }
}

///////////////////////////////////////////////////////////////////////////////

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FileStatus {
    NoFile,
    New(PathBuf),
    Saved(PathBuf),
    Modified(PathBuf),
}

impl ToString for FileStatus {
    fn to_string(&self) -> String {
        match self {
            FileStatus::NoFile => String::new(),
            FileStatus::Saved(ref path) => format!("{}", path.display()),
            FileStatus::New(ref path) => format!("{} [new]", path.display()),
            FileStatus::Modified(ref path) => {
                format!("{} [modified]", path.display())
            }
        }
    }
}
