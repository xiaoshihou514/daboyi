use bevy::prelude::*;
use std::collections::HashMap;

use crate::editor::{AdminMap, CountryMap, ProvinceId};
use crate::map::borders::BorderChanges;
use crate::map::{BorderDirty, BorderVersion, PendingProvinceRecolor};
use crate::ui::UiInputBlock;

const MAX_UNDO_DEPTH: usize = 100;

#[derive(Clone)]
pub struct ProvinceOwnership {
    pub country: Option<String>,
    pub admin: Option<u32>,
}

#[derive(Clone, Default)]
pub struct UndoAction {
    pub changes: HashMap<ProvinceId, ProvinceOwnership>,
}

#[derive(Resource, Default)]
pub struct UndoStack {
    pub undo: Vec<UndoAction>,
    pub redo: Vec<UndoAction>,
}

impl UndoStack {
    pub fn begin_action(&mut self) {
        self.undo.push(UndoAction::default());
    }

    pub fn current_action_mut(&mut self) -> Option<&mut UndoAction> {
        self.undo.last_mut()
    }

    pub fn commit_action(&mut self) {
        if let Some(action) = self.undo.last() {
            if action.changes.is_empty() {
                self.undo.pop();
                return;
            }
        }
        if self.undo.len() > MAX_UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

pub fn undo_redo_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut undo_stack: ResMut<UndoStack>,
    mut country_map: ResMut<CountryMap>,
    mut admin_map: ResMut<AdminMap>,
    mut pending_recolor: ResMut<PendingProvinceRecolor>,
    mut border_changes: ResMut<BorderChanges>,
    mut border_dirty: ResMut<BorderDirty>,
    mut border_version: ResMut<BorderVersion>,
    ui_input_block: Res<UiInputBlock>,
) {
    if ui_input_block.0 {
        return;
    }

    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    let do_undo = ctrl && !shift && keys.just_pressed(KeyCode::KeyZ);
    let do_redo =
        (ctrl && keys.just_pressed(KeyCode::KeyY)) || (ctrl && shift && keys.just_pressed(KeyCode::KeyZ));

    if do_undo {
        apply_undo(
            &mut undo_stack,
            &mut country_map,
            &mut admin_map,
            &mut pending_recolor,
            &mut border_changes,
            &mut border_dirty,
            &mut border_version,
        );
    } else if do_redo {
        apply_redo(
            &mut undo_stack,
            &mut country_map,
            &mut admin_map,
            &mut pending_recolor,
            &mut border_changes,
            &mut border_dirty,
            &mut border_version,
        );
    }
}

fn apply_undo(
    undo_stack: &mut UndoStack,
    country_map: &mut CountryMap,
    admin_map: &mut AdminMap,
    pending_recolor: &mut PendingProvinceRecolor,
    border_changes: &mut BorderChanges,
    border_dirty: &mut BorderDirty,
    border_version: &mut BorderVersion,
) {
    let Some(action) = undo_stack.undo.pop() else {
        return;
    };

    let mut redo_action = UndoAction::default();
    for (&prov_id, old_ownership) in &action.changes {
        let current = ProvinceOwnership {
            country: country_map.0.get(&prov_id).cloned(),
            admin: admin_map.0.get(&prov_id).copied(),
        };
        redo_action.changes.insert(prov_id, current);

        restore_ownership(prov_id, old_ownership, country_map, admin_map);
        pending_recolor.0.insert(prov_id);
        border_changes.changed_provinces.insert(prov_id);
    }

    if !redo_action.changes.is_empty() {
        undo_stack.redo.push(redo_action);
        border_dirty.0 = true;
        border_version.0 += 1;
    }
}

fn apply_redo(
    undo_stack: &mut UndoStack,
    country_map: &mut CountryMap,
    admin_map: &mut AdminMap,
    pending_recolor: &mut PendingProvinceRecolor,
    border_changes: &mut BorderChanges,
    border_dirty: &mut BorderDirty,
    border_version: &mut BorderVersion,
) {
    let Some(action) = undo_stack.redo.pop() else {
        return;
    };

    let mut undo_action = UndoAction::default();
    for (&prov_id, ownership) in &action.changes {
        let current = ProvinceOwnership {
            country: country_map.0.get(&prov_id).cloned(),
            admin: admin_map.0.get(&prov_id).copied(),
        };
        undo_action.changes.insert(prov_id, current);

        restore_ownership(prov_id, ownership, country_map, admin_map);
        pending_recolor.0.insert(prov_id);
        border_changes.changed_provinces.insert(prov_id);
    }

    if !undo_action.changes.is_empty() {
        undo_stack.undo.push(undo_action);
        border_dirty.0 = true;
        border_version.0 += 1;
    }
}

fn restore_ownership(
    prov_id: ProvinceId,
    ownership: &ProvinceOwnership,
    country_map: &mut CountryMap,
    admin_map: &mut AdminMap,
) {
    match &ownership.country {
        Some(tag) => {
            country_map.0.insert(prov_id, tag.clone());
        }
        None => {
            country_map.0.remove(&prov_id);
        }
    }
    match ownership.admin {
        Some(admin_id) => {
            admin_map.0.insert(prov_id, admin_id);
        }
        None => {
            admin_map.0.remove(&prov_id);
        }
    }
}
