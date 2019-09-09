use crate::resources::SnapshotId;
use crate::session::{Session, SessionCoords};
use crate::util;

use rgx::core::Rect;
use rgx::kit::Animation;
use rgx::math::*;

use std::collections::btree_map;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::ops::Deref;
use std::path::PathBuf;
use std::time;

/// View identifier.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone, Debug)]
pub struct ViewId(u16);

impl Default for ViewId {
    fn default() -> Self {
        ViewId(0)
    }
}

impl fmt::Display for ViewId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// View coordinates.
///
/// These coordinates are relative to the bottom left corner of the view.
#[derive(Copy, Clone, PartialEq)]
pub struct ViewCoords<T>(Point2<T>);

impl<T> ViewCoords<T> {
    pub fn new(x: T, y: T) -> Self {
        Self(Point2::new(x, y))
    }
}

impl ViewCoords<i32> {
    pub fn clamp(&mut self, rect: Rect<i32>) {
        util::clamp(&mut self.0, rect);
    }
}

impl<T> Deref for ViewCoords<T> {
    type Target = Point2<T>;

    fn deref(&self) -> &Point2<T> {
        &self.0
    }
}

impl Into<ViewCoords<i32>> for ViewCoords<f32> {
    fn into(self) -> ViewCoords<i32> {
        ViewCoords::new(self.x.round() as i32, self.y.round() as i32)
    }
}

impl Into<ViewCoords<u32>> for ViewCoords<f32> {
    fn into(self) -> ViewCoords<u32> {
        ViewCoords::new(self.x.round() as u32, self.y.round() as u32)
    }
}

/// Current state of the view.
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

/// A view operation to be carried out by the renderer.
#[derive(Debug, Clone, Copy)]
pub enum ViewOp {
    /// Copy an area of the view to another area.
    Blit(Rect<f32>, Rect<f32>),
}

/// A view on a sprite or image.
#[derive(Debug)]
pub struct View {
    /// Frame width.
    pub fw: u32,
    /// Frame height.
    pub fh: u32,
    /// View offset relative to the session workspace.
    pub offset: Vector2<f32>,
    /// Identifier.
    pub id: ViewId,
    /// Zoom level.
    pub zoom: f32,
    /// List of operations to carry out on the view.  Cleared every frame.
    pub ops: Vec<ViewOp>,
    /// Whether the view is flipped in the X axis.
    pub flip_x: bool,
    /// Whether the view is flipped in the Y axis.
    pub flip_y: bool,
    /// Whether the cursor is hovering over this view.
    pub hover: bool,
    /// Status of the file displayed by this view.
    pub file_status: FileStatus,
    /// State of the view.
    pub state: ViewState,
    /// Animation state of the sprite displayed by this view.
    pub animation: Animation<Rect<f32>>,

    /// Which view snapshot has been saved to disk, if any.
    saved_snapshot: Option<SnapshotId>,
}

impl View {
    /// The default frame delay for animations.
    const DEFAULT_ANIMATION_DELAY: u64 = 160;

    /// Create a new view. Takes a frame width and height.
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

    /// View width. Basically frame-width times number of frames.
    pub fn width(&self) -> u32 {
        self.fw * self.animation.len() as u32
    }

    /// View height.
    pub fn height(&self) -> u32 {
        self.fh
    }

    /// View file name, if any.
    pub fn file_name(&self) -> Option<&PathBuf> {
        match self.file_status {
            FileStatus::New(ref f) => Some(f),
            FileStatus::Modified(ref f) => Some(f),
            FileStatus::Saved(ref f) => Some(f),
            FileStatus::NoFile => None,
        }
    }

    /// Mark the view as saved at a specific snapshot and with
    /// the given path.
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

    /// Extend the view by one frame.
    pub fn extend(&mut self) {
        let w = self.width() as f32;
        let fw = self.fw as f32;
        let fh = self.fh as f32;

        self.animation.push_frame(Rect::new(w, 0., w + fw, fh));

        self.touch();
    }

    /// Shrink the view by one frame.
    pub fn shrink(&mut self) {
        // Don't allow the view to have zero frames.
        if self.animation.len() > 1 {
            self.animation.pop_frame();
            self.touch();
        }
    }

    /// Extend the view by one frame, by cloning an existing frame,
    /// by index.
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

    /// Resize view frames to the given size.
    pub fn resize_frames(&mut self, fw: u32, fh: u32) {
        self.reset(fw, fh, self.animation.len());
    }

    /// Reset the view by providing frame size and number of frames.
    pub fn reset(&mut self, fw: u32, fh: u32, nframes: usize) {
        self.fw = fw;
        self.fh = fh;

        let mut frames = Vec::new();
        let origin = Rect::origin(self.fw as f32, self.fh as f32);

        for i in 0..nframes {
            frames.push(origin + Vector2::new(i as f32 * self.fw as f32, 0.));
        }
        self.animation = Animation::new(&frames, self.animation.delay);
    }

    /// Slice the view into the given number of frames.
    pub fn slice(&mut self, nframes: usize) -> bool {
        if self.width() % nframes as u32 == 0 {
            let fw = self.width() / nframes as u32;
            self.reset(fw, self.fh, nframes);
            return true;
        }
        false
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

    /// Set the delay between animation frames.
    pub fn set_animation_delay(&mut self, ms: u64) {
        self.animation.delay = time::Duration::from_millis(ms);
    }

    /// Set the view state to `Okay`.
    pub fn okay(&mut self) {
        self.state = ViewState::Okay;
        self.ops.clear();
    }

    /// Update the view by one "tick".
    pub fn update(&mut self, delta: time::Duration) {
        self.animation.step(delta);
    }

    /// Return the view area, including the offset.
    pub fn rect(&self) -> Rect<f32> {
        Rect::new(
            self.offset.x,
            self.offset.y,
            self.offset.x + self.width() as f32 * self.zoom,
            self.offset.y + self.height() as f32 * self.zoom,
        )
    }

    /// Check whether the session coordinates are contained within the view.
    pub fn contains(&self, p: SessionCoords) -> bool {
        self.rect().contains(*p)
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

    /// Check whether the view is damaged.
    pub fn is_damaged(&self) -> bool {
        self.state == ViewState::Damaged
    }

    /// Check whether the view is dirty.
    pub fn is_dirty(&self) -> bool {
        self.state == ViewState::Dirty
    }

    /// Check whether the view is okay.
    pub fn is_okay(&self) -> bool {
        self.state == ViewState::Okay
    }

    /// Return the file status as a string.
    pub fn status(&self) -> String {
        self.file_status.to_string()
    }

    /// Check whether the given snapshot has been saved to disk.
    pub fn is_snapshot_saved(&self, id: SnapshotId) -> bool {
        self.saved_snapshot == Some(id)
    }

    /// Handle cursor movement.
    pub fn handle_cursor_moved(&mut self, cursor: SessionCoords) {
        self.hover = self.contains(cursor);
    }

    ////////////////////////////////////////////////////////////////////////////

    fn saved(&mut self, id: SnapshotId, path: PathBuf) {
        self.file_status = FileStatus::Saved(path);
        self.saved_snapshot = Some(id);
    }
}

///////////////////////////////////////////////////////////////////////////////

/// Status of the underlying file displayed by the view.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FileStatus {
    /// There is no file being displayed.
    NoFile,
    /// The file is new and unsaved.
    New(PathBuf),
    /// The file is saved and unmodified.
    Saved(PathBuf),
    /// The file has been modified since the last save.
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

///////////////////////////////////////////////////////////////////////////////

/// Manages views.
#[derive(Debug)]
pub struct ViewManager {
    /// Currently active view.
    pub active_id: ViewId,

    /// View dictionary.
    views: BTreeMap<ViewId, View>,

    /// The next `ViewId`.
    next_id: ViewId,

    /// A last-recently-used list of views.
    lru: VecDeque<ViewId>,
}

impl ViewManager {
    /// Maximum number of views in the view LRU list.
    const MAX_LRU: usize = Session::MAX_VIEWS;

    /// New empty view manager.
    pub fn new() -> Self {
        Self {
            active_id: ViewId::default(),
            next_id: ViewId(1),
            views: BTreeMap::new(),
            lru: VecDeque::new(),
        }
    }

    /// Add a view.
    pub fn add(&mut self, fs: FileStatus, w: u32, h: u32) -> ViewId {
        let id = self.gen_id();
        let view = View::new(id, fs, w, h);

        self.views.insert(id, view);

        id
    }

    /// Remove a view.
    pub fn remove(&mut self, id: &ViewId) {
        assert!(!self.lru.is_empty());
        self.views.remove(id);
        self.lru.retain(|v| v != id);

        if let Some(v) = self.last() {
            self.activate(v);
        } else {
            self.active_id = ViewId::default();
        }
    }

    /// Return the id of the last recently active view, if any.
    pub fn last(&self) -> Option<ViewId> {
        self.lru.front().map(|v| *v)
    }

    /// Return the currently active view, if any.
    pub fn active(&self) -> Option<&View> {
        self.views.get(&self.active_id)
    }

    /// Activate a view.
    pub fn activate(&mut self, id: ViewId) {
        debug_assert!(
            self.views.contains_key(&id),
            "the view being activated exists"
        );
        if self.active_id == id {
            return;
        }
        self.active_id = id;
        self.lru.push_front(id);
        self.lru.truncate(Self::MAX_LRU);
    }

    /// Iterate over views, mutably.
    pub fn iter_mut(&mut self) -> btree_map::IterMut<'_, ViewId, View> {
        self.views.iter_mut()
    }

    /// Get a view, mutably.
    pub fn get_mut(&mut self, id: &ViewId) -> Option<&mut View> {
        self.views.get_mut(id)
    }

    /// Generate a new view id.
    fn gen_id(&mut self) -> ViewId {
        let ViewId(id) = self.next_id;
        self.next_id = ViewId(id + 1);

        ViewId(id)
    }
}

impl Deref for ViewManager {
    type Target = BTreeMap<ViewId, View>;

    fn deref(&self) -> &Self::Target {
        &self.views
    }
}
