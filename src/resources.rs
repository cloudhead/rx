use crate::view::layer::LayerId;
use crate::view::pixels::Pixels;
use crate::view::resource::{Snapshot, ViewResource};
use crate::view::{ViewExtent, ViewId};

use rgx::color::Rgba8;
use rgx::rect::Rect;

use std::cell::{Ref, RefCell, RefMut};
use std::collections::BTreeMap;
use std::rc::Rc;

pub struct ResourceManager {
    resources: Rc<RefCell<Resources>>,
}

pub struct Resources {
    data: BTreeMap<ViewId, ViewResource>,
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

    pub fn get_view(&self, id: ViewId) -> Option<&ViewResource> {
        self.data.get(&id)
    }

    pub fn get_view_mut(&mut self, id: ViewId) -> Option<&mut ViewResource> {
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

    pub fn add_view(&mut self, id: ViewId, extent: ViewExtent, pixels: Pixels) {
        self.resources
            .borrow_mut()
            .data
            .insert(id, ViewResource::new(pixels, extent));
    }
}
