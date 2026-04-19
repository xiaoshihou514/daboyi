//! 行政区分配校验

use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use crate::editor::{AdminAreas, AdminId, AdminMap, CountryMap, ProvinceId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminBrushRelation {
    Selected,
    Sibling,
    Unclaimed,
}

/// 验证行政区分配：确保映射只指向仍然存在的行政区，且父级链完整。
pub fn validate_admin_assignments(admin_areas: Res<AdminAreas>, mut admin_map: ResMut<AdminMap>) {
    let admin_children: HashMap<AdminId, Vec<AdminId>> = {
        let mut map = HashMap::new();
        for area in &admin_areas.0 {
            if let Some(parent_id) = area.parent_id {
                map.entry(parent_id).or_insert_with(Vec::new).push(area.id);
            }
        }
        map
    };

    let mut to_remove = Vec::new();
    for (&prov_id, &admin_id) in &admin_map.0 {
        if !is_valid_assignment(prov_id, admin_id, &admin_areas.0, &admin_children) {
            to_remove.push(prov_id);
        }
    }

    for prov_id in to_remove {
        admin_map.0.remove(&prov_id);
    }
}

fn is_valid_assignment(
    _province_id: ProvinceId,
    admin_id: AdminId,
    admin_areas: &[shared::AdminArea],
    _admin_children: &HashMap<AdminId, Vec<AdminId>>,
) -> bool {
    let Some(target_admin) = admin_areas.iter().find(|area| area.id == admin_id) else {
        return false;
    };

    if let Some(parent_id) = target_admin.parent_id {
        return admin_areas.iter().any(|area| area.id == parent_id);
    }

    true
}

pub fn classify_province_for_active_admin(
    selected_admin_id: AdminId,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> Option<AdminBrushRelation> {
    let selected = admin_areas
        .iter()
        .find(|area| area.id == selected_admin_id)?;

    match selected.parent_id {
        Some(parent_id) => classify_in_parent_scope(
            selected_admin_id,
            parent_id,
            admin_areas,
            admin_map,
            province_id,
        ),
        None => classify_in_country_scope(
            selected,
            selected_admin_id,
            admin_areas,
            admin_map,
            country_map,
            province_id,
        ),
    }
}

fn classify_in_parent_scope(
    selected_admin_id: AdminId,
    parent_id: AdminId,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    province_id: ProvinceId,
) -> Option<AdminBrushRelation> {
    let sibling_ids: HashSet<AdminId> = admin_areas
        .iter()
        .filter(|area| area.parent_id == Some(parent_id))
        .map(|area| area.id)
        .collect();

    let assigned = admin_map.0.get(&province_id).copied()?;
    if assigned == parent_id {
        return Some(AdminBrushRelation::Unclaimed);
    }
    if !sibling_ids.contains(&assigned) {
        return None;
    }
    if assigned == selected_admin_id {
        Some(AdminBrushRelation::Selected)
    } else {
        Some(AdminBrushRelation::Sibling)
    }
}

fn classify_in_country_scope(
    selected: &shared::AdminArea,
    selected_admin_id: AdminId,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> Option<AdminBrushRelation> {
    let owner = country_map.0.get(&province_id)?;
    if owner != &selected.country_tag {
        return None;
    }

    let top_level_ids: HashSet<AdminId> = admin_areas
        .iter()
        .filter(|area| area.country_tag == selected.country_tag && area.parent_id.is_none())
        .map(|area| area.id)
        .collect();

    let province_country = admin_map
        .0
        .get(&province_id)
        .and_then(|admin_id| admin_areas.iter().find(|area| area.id == *admin_id))
        .map(|area| area.country_tag.as_str())
        .or_else(|| country_map.0.get(&province_id).map(String::as_str));

    if province_country != Some(selected.country_tag.as_str()) {
        return None;
    }

    match admin_map.0.get(&province_id).copied() {
        None => Some(AdminBrushRelation::Unclaimed),
        Some(assigned) if !top_level_ids.contains(&assigned) => None,
        Some(assigned) if assigned == selected_admin_id => Some(AdminBrushRelation::Selected),
        Some(_) => Some(AdminBrushRelation::Sibling),
    }
}
