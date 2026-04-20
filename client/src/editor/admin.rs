//! 行政区分配校验

use bevy::prelude::*;
use std::collections::HashMap;

use crate::editor::{AdminAreas, AdminId, AdminMap, CountryMap, ProvinceId};

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

pub fn admin_area_by_id(
    admin_areas: &[shared::AdminArea],
    admin_id: AdminId,
) -> Option<&shared::AdminArea> {
    admin_areas.iter().find(|area| area.id == admin_id)
}

pub fn province_country_tag<'a>(
    admin_areas: &'a [shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &'a CountryMap,
    province_id: ProvinceId,
) -> Option<&'a str> {
    admin_map
        .0
        .get(&province_id)
        .and_then(|admin_id| admin_area_by_id(admin_areas, *admin_id))
        .map(|area| area.country_tag.as_str())
        .or_else(|| country_map.0.get(&province_id).map(String::as_str))
}

fn admin_ancestor_chain(admin_id: AdminId, admin_areas: &[shared::AdminArea]) -> Vec<AdminId> {
    let mut chain = Vec::new();
    let mut current = Some(admin_id);
    while let Some(area_id) = current {
        chain.push(area_id);
        current = admin_area_by_id(admin_areas, area_id).and_then(|area| area.parent_id);
    }
    chain.reverse();
    chain
}

fn admin_owner_chain_for_province(
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    province_id: ProvinceId,
) -> Vec<AdminId> {
    admin_map
        .0
        .get(&province_id)
        .copied()
        .map(|admin_id| admin_ancestor_chain(admin_id, admin_areas))
        .unwrap_or_default()
}

pub fn should_show_admin_children(
    area_id: AdminId,
    admin_areas: &[shared::AdminArea],
    active_admin: Option<AdminId>,
) -> bool {
    let Some(selected_id) = active_admin else {
        return false;
    };
    selected_id == area_id || is_descendant_of(selected_id, area_id, admin_areas)
}

pub fn can_assign_province_to_active_country(
    _selected_country_tag: &str,
    _admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> bool {
    if country_map.0.contains_key(&province_id) || admin_map.0.contains_key(&province_id) {
        return false;
    }
    true
}

pub fn can_assign_province_to_active_admin(
    selected_admin_id: AdminId,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> bool {
    let Some(selected) = admin_area_by_id(admin_areas, selected_admin_id) else {
        return false;
    };

    if country_map.0.contains_key(&province_id) && admin_map.0.contains_key(&province_id) {
        return false;
    }

    if let Some(parent_id) = selected.parent_id {
        return admin_map.0.get(&province_id).copied() == Some(parent_id);
    }

    if country_map.0.get(&province_id).map(String::as_str) == Some(selected.country_tag.as_str()) {
        return !admin_map.0.contains_key(&province_id);
    }

    false
}

pub fn can_erase_province_from_active_selection(
    active_country: Option<&str>,
    active_admin: Option<AdminId>,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> bool {
    if let Some(admin_id) = active_admin {
        return admin_map.0.get(&province_id).copied() == Some(admin_id);
    }

    let Some(country_tag) = active_country else {
        return false;
    };

    if country_map.0.get(&province_id).map(String::as_str) == Some(country_tag) {
        return true;
    }

    admin_map
        .0
        .get(&province_id)
        .and_then(|admin_id| admin_area_by_id(admin_areas, *admin_id))
        .map(|area| area.country_tag.as_str() == country_tag)
        .unwrap_or(false)
}

pub fn child_admin_ids_in_tree(
    area_id: AdminId,
    admin_areas: &[shared::AdminArea],
) -> Vec<AdminId> {
    let Some(parent_area) = admin_areas.iter().find(|area| area.id == area_id) else {
        return Vec::new();
    };

    admin_areas
        .iter()
        .filter(|area| {
            area.parent_id == Some(area_id) && area.country_tag == parent_area.country_tag
        })
        .map(|area| area.id)
        .collect()
}

pub fn visible_admin_id_for_province(
    active_country: Option<&str>,
    active_admin: Option<AdminId>,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    province_id: ProvinceId,
) -> Option<AdminId> {
    let province_country = province_country_tag(admin_areas, admin_map, country_map, province_id)?;
    if active_country != Some(province_country) {
        return None;
    }

    let owner_chain = admin_owner_chain_for_province(admin_areas, admin_map, province_id);
    if owner_chain.is_empty() {
        return None;
    }

    let active_path = active_admin
        .map(|admin_id| admin_ancestor_chain(admin_id, admin_areas))
        .unwrap_or_default();

    let mut shared_depth = 0;
    while shared_depth < owner_chain.len()
        && shared_depth < active_path.len()
        && owner_chain[shared_depth] == active_path[shared_depth]
    {
        shared_depth += 1;
    }

    owner_chain
        .get(shared_depth)
        .copied()
        .or_else(|| owner_chain.last().copied())
}

fn is_descendant_of(
    area_id: AdminId,
    ancestor_id: AdminId,
    admin_areas: &[shared::AdminArea],
) -> bool {
    let mut current = admin_areas
        .iter()
        .find(|area| area.id == area_id)
        .and_then(|area| area.parent_id);
    while let Some(parent_id) = current {
        if parent_id == ancestor_id {
            return true;
        }
        current = admin_areas
            .iter()
            .find(|area| area.id == parent_id)
            .and_then(|area| area.parent_id);
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn area(id: AdminId, country_tag: &str, parent_id: Option<AdminId>) -> shared::AdminArea {
        shared::AdminArea {
            id,
            name: format!("ADM{id}"),
            country_tag: country_tag.to_owned(),
            parent_id,
            color: None,
        }
    }

    #[test]
    fn top_level_selection_does_not_expand_its_children() {
        let admin_areas = vec![
            area(1, "A", None),
            area(11, "A", Some(1)),
            area(12, "A", Some(1)),
        ];

        assert!(should_show_admin_children(1, &admin_areas, Some(1)));
        assert!(should_show_admin_children(1, &admin_areas, Some(11)));
        assert!(should_show_admin_children(1, &admin_areas, Some(12)));
        assert!(!should_show_admin_children(1, &admin_areas, None));
    }

    #[test]
    fn admin_selection_shows_selected_node_children() {
        let admin_areas = vec![
            area(1, "A", None),
            area(11, "A", Some(1)),
            area(111, "A", Some(11)),
        ];

        assert!(should_show_admin_children(11, &admin_areas, Some(11)));
        assert!(!should_show_admin_children(111, &admin_areas, Some(11)));
    }

    #[test]
    fn country_painting_only_allows_unclaimed_provinces() {
        let admin_areas = vec![area(11, "A", Some(1))];
        let admin_map = AdminMap(HashMap::from([(3, 11)]));
        let country_map = CountryMap(HashMap::from([
            (1, String::from("A")),
            (2, String::from("B")),
        ]));

        assert!(can_assign_province_to_active_country(
            "A",
            &admin_areas,
            &admin_map,
            &country_map,
            99,
        ));
        assert!(!can_assign_province_to_active_country(
            "A",
            &admin_areas,
            &admin_map,
            &country_map,
            1,
        ));
        assert!(!can_assign_province_to_active_country(
            "A",
            &admin_areas,
            &admin_map,
            &country_map,
            2,
        ));
        assert!(!can_assign_province_to_active_country(
            "A",
            &admin_areas,
            &admin_map,
            &country_map,
            3,
        ));
    }

    #[test]
    fn child_tree_ids_ignore_cross_country_children() {
        let admin_areas = vec![
            area(1, "A", None),
            area(11, "A", Some(1)),
            area(12, "B", Some(1)),
        ];

        assert_eq!(child_admin_ids_in_tree(1, &admin_areas), vec![11]);
    }

    #[test]
    fn admin_painting_only_allows_parent_owned_provinces() {
        let admin_areas = vec![
            area(1, "A", None),
            area(11, "A", Some(1)),
            area(12, "A", Some(1)),
        ];
        let admin_map = AdminMap(HashMap::from([(7, 1), (8, 12), (9, 11)]));
        let country_map = CountryMap(HashMap::from([(3, String::from("A"))]));

        assert!(can_assign_province_to_active_admin(
            1,
            &admin_areas,
            &admin_map,
            &country_map,
            3,
        ));
        assert!(!can_assign_province_to_active_admin(
            1,
            &admin_areas,
            &admin_map,
            &country_map,
            7,
        ));
        assert!(can_assign_province_to_active_admin(
            11,
            &admin_areas,
            &admin_map,
            &country_map,
            7,
        ));
        assert!(!can_assign_province_to_active_admin(
            11,
            &admin_areas,
            &admin_map,
            &country_map,
            8,
        ));
        assert!(!can_assign_province_to_active_admin(
            11,
            &admin_areas,
            &admin_map,
            &country_map,
            9,
        ));
    }

    #[test]
    fn erase_scope_respects_selected_node() {
        let admin_areas = vec![
            area(1, "A", None),
            area(11, "A", Some(1)),
            area(2, "B", None),
        ];
        let admin_map = AdminMap(HashMap::from([(7, 11), (8, 2)]));
        let country_map = CountryMap(HashMap::from([(3, String::from("A"))]));

        assert!(can_erase_province_from_active_selection(
            Some("A"),
            None,
            &admin_areas,
            &admin_map,
            &country_map,
            3,
        ));
        assert!(can_erase_province_from_active_selection(
            Some("A"),
            None,
            &admin_areas,
            &admin_map,
            &country_map,
            7,
        ));
        assert!(!can_erase_province_from_active_selection(
            Some("A"),
            None,
            &admin_areas,
            &admin_map,
            &country_map,
            8,
        ));
        assert!(can_erase_province_from_active_selection(
            Some("A"),
            Some(11),
            &admin_areas,
            &admin_map,
            &country_map,
            7,
        ));
        assert!(!can_erase_province_from_active_selection(
            Some("A"),
            Some(11),
            &admin_areas,
            &admin_map,
            &country_map,
            3,
        ));
    }

    #[test]
    fn visible_admin_collapses_to_direct_child_of_selected_path() {
        let admin_areas = vec![
            area(1, "A", None),
            area(2, "A", None),
            area(11, "A", Some(1)),
            area(12, "A", Some(1)),
            area(111, "A", Some(11)),
        ];
        let admin_map = AdminMap(HashMap::from([(7, 1), (8, 11), (9, 111), (10, 2)]));
        let country_map = CountryMap(HashMap::from([
            (3, String::from("A")),
            (4, String::from("B")),
        ]));

        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                None,
                &admin_areas,
                &admin_map,
                &country_map,
                8
            ),
            Some(1)
        );
        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                None,
                &admin_areas,
                &admin_map,
                &country_map,
                9
            ),
            Some(1)
        );
        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                Some(1),
                &admin_areas,
                &admin_map,
                &country_map,
                8
            ),
            Some(11)
        );
        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                Some(11),
                &admin_areas,
                &admin_map,
                &country_map,
                9
            ),
            Some(111)
        );
        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                Some(11),
                &admin_areas,
                &admin_map,
                &country_map,
                10
            ),
            Some(2)
        );
        assert_eq!(
            visible_admin_id_for_province(
                Some("A"),
                Some(11),
                &admin_areas,
                &admin_map,
                &country_map,
                4
            ),
            None
        );
    }
}
