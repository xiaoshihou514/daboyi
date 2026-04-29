//! 刷子工具实现
#![allow(clippy::too_many_arguments)]

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use shared::map::MapProvince;

use crate::editor::{
    admin_area_by_id, can_assign_province_to_active_admin, can_assign_province_to_active_country,
    can_erase_province_from_active_selection, ActiveAdmin, ActiveCountry, AdminMap, BrushTool,
    CountryMap, DragState, NonPlayableProvinces, SpatialHash,
};
use crate::map::borders::BorderChanges;
use crate::map::{
    BorderDirty, BorderVersion, MapResource, PaintDebounce, PendingProvinceRecolor, MAP_WIDTH,
};
use crate::memory::MemoryMonitor;
use crate::ui::UiInputBlock;

#[derive(Resource, Default)]
pub struct BrushScratch {
    candidates: Vec<u32>,
}

#[derive(SystemParam)]
pub(crate) struct BrushAssignments<'w, 's> {
    admin_map: ResMut<'w, AdminMap>,
    country_map: ResMut<'w, CountryMap>,
    active_admin: Res<'w, ActiveAdmin>,
    active_country: Res<'w, ActiveCountry>,
    admin_areas: Res<'w, crate::editor::AdminAreas>,
    pending_province_recolor: ResMut<'w, PendingProvinceRecolor>,
    border_changes: ResMut<'w, BorderChanges>,
    border_dirty: ResMut<'w, BorderDirty>,
    border_version: ResMut<'w, BorderVersion>,
    debounce: ResMut<'w, PaintDebounce>,
    ui_input_block: Res<'w, UiInputBlock>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// 刷子输入处理系统
pub fn brush_input_system(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: EventReader<bevy::input::mouse::MouseWheel>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<Camera2d>>,
    map: Option<Res<MapResource>>,
    spatial_hash: Res<SpatialHash>,
    non_playable_provinces: Res<NonPlayableProvinces>,
    mut brush: ResMut<BrushTool>,
    mut drag: ResMut<DragState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut assignments: BrushAssignments,
    mut scratch: ResMut<BrushScratch>,
) {
    let Some(map) = map else { return };

    if mouse_input.just_pressed(MouseButton::Left) {
        MemoryMonitor::log_memory_usage("Before brush drag start");
    }

    if assignments.ui_input_block.0 {
        flush_immediately(&mut assignments, &drag);
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        return;
    }

    if keys.just_pressed(KeyCode::KeyB) {
        flush_immediately(&mut assignments, &drag);
        brush.enabled = !brush.enabled;
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        bevy::log::info!(target: "daboyi::editor", "刷子：{}", if brush.enabled { "开启" } else { "关闭" });
        return;
    }

    if keys.just_pressed(KeyCode::KeyE) && brush.enabled {
        brush.eraser_mode = !brush.eraser_mode;
        bevy::log::info!(
            target: "daboyi::editor",
            "橡皮擦：{}",
            if brush.eraser_mode {
                "开启"
            } else {
                "关闭"
            }
        );
    }

    for ev in mouse_wheel.read() {
        let adjusting_brush = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        if brush.enabled && adjusting_brush {
            brush.radius = (brush.radius + ev.y * 8.0).clamp(8.0, 240.0);
        }
    }

    if !brush.enabled {
        flush_immediately(&mut assignments, &drag);
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Some((world_pos, world_radius)) = get_mouse_world_pos(cursor_pos, &camera_q, brush.radius)
    else {
        return;
    };

    if mouse_input.just_pressed(MouseButton::Left) {
        drag.is_dragging = true;
        drag.painted_provinces.clear();
    }
    if mouse_input.just_released(MouseButton::Left) {
        if assignments.border_dirty.0 {
            assignments.border_dirty.0 = false;
            assignments.debounce.pending_border = true;
        }
        if assignments.debounce.pending_border {
            assignments.debounce.kick();
        }
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        MemoryMonitor::log_memory_usage("After brush drag end");
    }

    if !drag.is_dragging {
        return;
    }

    scratch.candidates.clear();
    find_provinces_in_radius_fast(
        world_pos,
        world_radius,
        &map,
        &spatial_hash,
        &non_playable_provinces,
        &mut scratch.candidates,
    );

    if scratch.candidates.is_empty() {
        return;
    }

    let target_admin = assignments.active_admin.0;
    let target_country = &assignments.active_country.0;

    let mut processed_provinces = Vec::new();
    let mut changed_any = false;

    if brush.eraser_mode {
        for &prov_id in &scratch.candidates {
            if drag.painted_provinces.contains(&prov_id) {
                continue;
            }
            if !can_erase_province_from_active_selection(
                target_country.as_deref(),
                target_admin,
                &assignments.admin_areas.0,
                &assignments.admin_map,
                &assignments.country_map,
                prov_id,
            ) {
                continue;
            }

            let mut erased_any = false;
            if let Some(admin_id) = target_admin {
                if assignments.admin_map.0.remove(&prov_id) == Some(admin_id) {
                    if let Some(area) = admin_area_by_id(&assignments.admin_areas.0, admin_id) {
                        assignments
                            .country_map
                            .0
                            .insert(prov_id, area.country_tag.clone());
                    }
                    erased_any = true;
                }
            } else if let Some(country_tag) = target_country.as_deref() {
                if assignments.country_map.0.remove(&prov_id).is_some() {
                    erased_any = true;
                } else if assignments
                    .admin_map
                    .0
                    .get(&prov_id)
                    .copied()
                    .and_then(|admin_id| admin_area_by_id(&assignments.admin_areas.0, admin_id))
                    .map(|area| area.country_tag.as_str() == country_tag)
                    .unwrap_or(false)
                {
                    erased_any = assignments.admin_map.0.remove(&prov_id).is_some();
                }
            }

            if erased_any {
                assignments.pending_province_recolor.0.insert(prov_id);
                assignments.border_changes.changed_provinces.insert(prov_id);
                changed_any = true;
            }
            processed_provinces.push(prov_id);
        }
    } else {
        for &prov_id in &scratch.candidates {
            if drag.painted_provinces.contains(&prov_id) {
                continue;
            }

            let old_country = assignments.country_map.0.get(&prov_id).cloned();
            let old_admin = assignments.admin_map.0.get(&prov_id).copied();

            if let Some(admin_id) = target_admin {
                if !can_assign_province_to_active_admin(
                    admin_id,
                    &assignments.admin_areas.0,
                    &assignments.admin_map,
                    &assignments.country_map,
                    prov_id,
                ) {
                    continue;
                }
                let admin_changed = old_admin != Some(admin_id);
                let owner_replaced = old_country.is_some() || old_admin.is_some();
                if admin_changed || owner_replaced {
                    assignments.admin_map.0.insert(prov_id, admin_id);
                    assignments.country_map.0.remove(&prov_id);
                    assignments.pending_province_recolor.0.insert(prov_id);
                    assignments.border_changes.changed_provinces.insert(prov_id);
                    changed_any = true;
                }
                processed_provinces.push(prov_id);
            } else if let Some(country_tag) = target_country {
                if !can_assign_province_to_active_country(
                    country_tag,
                    &assignments.admin_areas.0,
                    &assignments.admin_map,
                    &assignments.country_map,
                    prov_id,
                ) {
                    continue;
                }
                let country_changed = old_country.as_ref() != Some(country_tag);
                if country_changed {
                    assignments
                        .country_map
                        .0
                        .insert(prov_id, country_tag.clone());
                    assignments.pending_province_recolor.0.insert(prov_id);
                    assignments.border_changes.changed_provinces.insert(prov_id);
                    changed_any = true;
                }
                processed_provinces.push(prov_id);
            }
        }
    }

    for prov_id in processed_provinces {
        drag.painted_provinces.insert(prov_id);
    }

    if changed_any {
        assignments.border_dirty.0 = true;
    }
}

fn flush_immediately(assignments: &mut BrushAssignments, drag: &DragState) {
    let needs_border =
        assignments.border_dirty.0 || (drag.is_dragging && assignments.debounce.pending_border);
    if needs_border {
        assignments.border_dirty.0 = false;
        assignments.debounce.pending_border = false;
        assignments.border_version.0 += 1;
    }
}

pub fn brush_cursor_system(
    brush: Res<BrushTool>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<Camera2d>>,
    mut gizmos: Gizmos,
    ui_input_block: Res<UiInputBlock>,
) {
    if !brush.enabled || ui_input_block.0 {
        return;
    }

    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Some((world_pos, world_radius)) = get_mouse_world_pos(cursor_pos, &camera_q, brush.radius)
    else {
        return;
    };

    let outer_color = if brush.eraser_mode {
        Color::srgba(1.0, 0.2, 0.2, 0.7)
    } else {
        Color::srgba(1.0, 1.0, 1.0, 0.5)
    };
    gizmos.circle_2d(
        Vec2::new(world_pos[0], world_pos[1]),
        world_radius,
        outer_color,
    );
}

fn get_mouse_world_pos(
    cursor_pos: Vec2,
    camera_q: &Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<Camera2d>>,
    screen_radius: f32,
) -> Option<([f32; 2], f32)> {
    let Ok((camera, cam_transform, projection)) = camera_q.get_single() else {
        return None;
    };
    let world_pos = camera
        .viewport_to_world_2d(cam_transform, cursor_pos)
        .ok()?;
    let world_radius = screen_radius * projection.scale;
    Some(([world_pos.x, world_pos.y], world_radius))
}

fn find_provinces_in_radius_fast(
    pos: [f32; 2],
    radius: f32,
    map: &MapResource,
    spatial_hash: &SpatialHash,
    non_playable_provinces: &NonPlayableProvinces,
    result: &mut Vec<u32>,
) {
    result.clear();

    let mut candidates = Vec::new();
    spatial_hash.find_in_radius_into(pos, radius, &mut candidates);

    let center_over_playable = candidates.iter().any(|&prov_id| {
        !non_playable_provinces.0.contains(&prov_id)
            && map
                .0
                .provinces
                .get(prov_id as usize)
                .map(|prov| {
                    [-MAP_WIDTH, 0.0, MAP_WIDTH]
                        .into_iter()
                        .any(|x_off| point_in_province_shifted(pos, prov, x_off))
                })
                .unwrap_or(false)
    });
    if !center_over_playable {
        return;
    }

    for &prov_id in &candidates {
        if non_playable_provinces.0.contains(&prov_id) {
            continue;
        }
        let prov_index = prov_id as usize;
        let Some(prov) = map.0.provinces.get(prov_index) else {
            continue;
        };

        if province_intersects_brush(pos, radius, prov) {
            result.push(prov_id);
        }
    }
}

fn province_intersects_brush(center: [f32; 2], radius: f32, province: &MapProvince) -> bool {
    if province.boundary.is_empty() {
        return false;
    }
    for &x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
        if point_in_province_shifted(center, province, x_off) {
            return true;
        }
        for ring in &province.boundary {
            if ring_intersects_brush(center, radius, ring, x_off) {
                return true;
            }
        }
    }
    false
}

fn point_in_province_shifted(point: [f32; 2], province: &MapProvince, x_off: f32) -> bool {
    if !point_in_polygon_shifted(point, &province.boundary[0], x_off) {
        return false;
    }
    for hole in province.boundary.iter().skip(1) {
        if point_in_polygon_shifted(point, hole, x_off) {
            return false;
        }
    }
    true
}

fn point_in_polygon_shifted(point: [f32; 2], ring: &[[f32; 2]], x_off: f32) -> bool {
    if ring.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut last = *ring.last().unwrap();
    for &current in ring {
        let xi = current[0] + x_off;
        let yi = current[1];
        let xj = last[0] + x_off;
        let yj = last[1];
        if ((yi > point[1]) != (yj > point[1]))
            && (point[0] < (xj - xi) * (point[1] - yi) / (yj - yi) + xi)
        {
            inside = !inside;
        }
        last = current;
    }
    inside
}

fn ring_intersects_brush(center: [f32; 2], radius: f32, ring: &[[f32; 2]], x_off: f32) -> bool {
    if ring.len() < 2 {
        return false;
    }
    let radius_sq = radius * radius;
    let mut last = *ring.last().unwrap();
    for &current in ring {
        let segment_start = [last[0] + x_off, last[1]];
        let segment_end = [current[0] + x_off, current[1]];
        if point_segment_distance_sq(center, segment_start, segment_end) <= radius_sq {
            return true;
        }
        last = current;
    }
    false
}

fn point_segment_distance_sq(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let segment_len_sq = dx * dx + dy * dy;
    if segment_len_sq <= 1e-9 {
        let px = point[0] - start[0];
        let py = point[1] - start[1];
        return px * px + py * py;
    }
    let t = (((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / segment_len_sq)
        .clamp(0.0, 1.0);
    let nearest = [start[0] + t * dx, start[1] + t * dy];
    let px = point[0] - nearest[0];
    let py = point[1] - nearest[1];
    px * px + py * py
}

#[cfg(test)]
mod tests {
    use super::{point_segment_distance_sq, province_intersects_brush};
    use shared::map::MapProvince;

    fn province(boundary: Vec<Vec<[f32; 2]>>) -> MapProvince {
        MapProvince {
            id: 0,
            tag: "p".to_owned(),
            name: "P".to_owned(),
            topography: "mountains".to_owned(),
            vegetation: String::new(),
            climate: String::new(),
            raw_material: String::new(),
            harbor_suitability: 0.0,
            hex_color: [0.0, 0.0, 0.0, 1.0],
            port_sea_zone: None,
            boundary,
            vertices: Vec::new(),
            indices: Vec::new(),
            centroid: [0.0, 0.0],
        }
    }

    #[test]
    fn brush_hit_test_uses_geometry_not_topography() {
        let sakya = province(vec![vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]]);
        assert!(province_intersects_brush([1.0, 1.0], 0.1, &sakya));
    }

    #[test]
    fn brush_hit_test_rejects_nearby_non_intersecting_province() {
        let province = province(vec![vec![
            [10.0, 10.0],
            [12.0, 10.0],
            [12.0, 12.0],
            [10.0, 12.0],
        ]]);
        assert!(!province_intersects_brush([0.0, 0.0], 1.0, &province));
    }

    #[test]
    fn point_segment_distance_handles_projection() {
        let distance_sq = point_segment_distance_sq([1.0, 1.0], [0.0, 0.0], [2.0, 0.0]);
        assert!((distance_sq - 1.0).abs() < 1e-6);
    }
}
