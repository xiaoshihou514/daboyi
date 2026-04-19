//! 刷子工具实现

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use shared::conv::u32_to_usize;

use crate::editor::{
    classify_province_for_active_admin, ActiveAdmin, ActiveCountry, AdminBrushRelation, AdminMap,
    BrushTool, CountryMap, DragState, SpatialHash,
};
use crate::map::{
    BorderDirty, BorderVersion, MapResource, PaintDebounce, PendingProvinceRecolor,
    MAP_WIDTH,
};
use crate::ui::UiInputBlock;

#[derive(SystemParam)]
pub(crate) struct BrushAssignments<'w, 's> {
    admin_map: ResMut<'w, AdminMap>,
    country_map: ResMut<'w, CountryMap>,
    active_admin: Res<'w, ActiveAdmin>,
    active_country: Res<'w, ActiveCountry>,
    admin_areas: Res<'w, crate::editor::AdminAreas>,
    pending_province_recolor: ResMut<'w, PendingProvinceRecolor>,
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
    mut brush: ResMut<BrushTool>,
    mut drag: ResMut<DragState>,
    keys: Res<ButtonInput<KeyCode>>,
    mut assignments: BrushAssignments,
) {
    let Some(map) = map else { return };

    if assignments.ui_input_block.0 {
        flush_immediately(&mut assignments, &drag);
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        return;
    }

    // 切换刷子：B 键
    if keys.just_pressed(KeyCode::KeyB) {
        flush_immediately(&mut assignments, &drag);
        brush.enabled = !brush.enabled;
        drag.is_dragging = false;
        drag.painted_provinces.clear();
        eprintln!("刷子：{}", if brush.enabled { "开启" } else { "关闭" });
        return;
    }

    // 调整刷子大小：Shift + 滚轮（普通滚轮仍用于缩放视角）
    for ev in mouse_wheel.read() {
        let adjusting_brush =
            keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
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

    // 获取鼠标世界坐标
    let Ok(window) = windows.get_single() else { return };
    let Some(cursor_pos) = window.cursor_position() else { return };
    let Some((world_pos, world_radius)) = get_mouse_world_pos(cursor_pos, &camera_q, brush.radius)
    else {
        return;
    };

    // 左键按下开始拖拽
    if mouse_input.just_pressed(MouseButton::Left) {
        drag.is_dragging = true;
        drag.painted_provinces.clear();
    }
    if mouse_input.just_released(MouseButton::Left) {
        // Instead of triggering an immediate GPU rebuild, kick the debounce timer.
        // Rapid successive strokes will keep resetting the timer so that only one
        // border rebuild happens after the user pauses painting (≥150 ms).
        if assignments.border_dirty.0 {
            assignments.border_dirty.0 = false;
            assignments.debounce.pending_border = true;
        }
        if assignments.debounce.pending_border {
            assignments.debounce.kick();
        }
        drag.is_dragging = false;
        drag.painted_provinces.clear();
    }

    if !drag.is_dragging {
        return;
    }

    // 使用空间哈希查找半径内的所有省份（快速）
    let provinces_in_brush = find_provinces_in_radius_fast(world_pos, world_radius, &map, &spatial_hash);

    if provinces_in_brush.is_empty() {
        return;
    }

    // 获取目标行政区
    let target_admin = assignments.active_admin.0;
    let target_country = assignments.active_country.0.clone();

    let mut processed_provinces = Vec::new();
    let mut changed_any = false;

    // 分配省份到目标
    for &prov_id in &provinces_in_brush {
        if drag.painted_provinces.contains(&prov_id) {
            continue; // 跳过已处理的省份
        }

        let old_country = assignments.country_map.0.get(&prov_id).cloned();
        let old_admin = assignments.admin_map.0.get(&prov_id).copied();

        if let Some(admin_id) = target_admin {
            let Some(relation) = classify_province_for_active_admin(
                admin_id,
                &assignments.admin_areas.0,
                &assignments.admin_map,
                &assignments.country_map,
                prov_id,
            ) else {
                continue;
            };
            if !matches!(
                relation,
                AdminBrushRelation::Selected
                    | AdminBrushRelation::Sibling
                    | AdminBrushRelation::Unclaimed
            ) {
                continue;
            }
            let admin_changed = old_admin != Some(admin_id);
            let country_cleared = old_country.is_some();
            if admin_changed || country_cleared {
                assignments.admin_map.0.insert(prov_id, admin_id);
                assignments.country_map.0.remove(&prov_id);
                assignments.pending_province_recolor.0.insert(prov_id);
                changed_any = true;
            }
            processed_provinces.push(prov_id);
        } else if let Some(country_tag) = &target_country {
            let country_changed = old_country.as_ref() != Some(country_tag);
            let admin_cleared = old_admin.is_some();
            if country_changed || admin_cleared {
                assignments.admin_map.0.remove(&prov_id);
                assignments.country_map.0.insert(prov_id, country_tag.clone());
                assignments.pending_province_recolor.0.insert(prov_id);
                changed_any = true;
            }
            processed_provinces.push(prov_id);
        }
    }

    for prov_id in processed_provinces {
        drag.painted_provinces.insert(prov_id);
    }

    if changed_any {
        assignments.border_dirty.0 = true;
    }
}

/// Immediately apply any pending border rebuild, bypassing the debounce.
/// Used when the brush is disabled or UI captures input.
fn flush_immediately(assignments: &mut BrushAssignments, drag: &DragState) {
    let needs_border =
        assignments.border_dirty.0 || (drag.is_dragging && assignments.debounce.pending_border);
    if needs_border {
        assignments.border_dirty.0 = false;
        assignments.debounce.pending_border = false;
        assignments.border_version.0 += 1;
    }
}

/// 刷子光标渲染系统
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

    let Ok(window) = windows.get_single() else { return };
    let Some(cursor_pos) = window.cursor_position() else { return };
    let Some((world_pos, world_radius)) = get_mouse_world_pos(cursor_pos, &camera_q, brush.radius)
    else {
        return;
    };

    // 绘制刷子范围圆圈
    gizmos.circle_2d(
        Vec2::new(world_pos[0], world_pos[1]),
        world_radius,
        Color::srgba(1.0, 1.0, 1.0, 0.5),
    );

    // 绘制中心点
    gizmos.circle_2d(
        Vec2::new(world_pos[0], world_pos[1]),
        0.2,
        Color::srgba(1.0, 0.0, 0.0, 0.8),
    );
}

/// 获取鼠标世界坐标
fn get_mouse_world_pos(
    cursor_pos: Vec2,
    camera_q: &Query<(&Camera, &GlobalTransform, &OrthographicProjection), With<Camera2d>>,
    screen_radius: f32,
) -> Option<([f32; 2], f32)> {
    let Ok((camera, cam_transform, projection)) = camera_q.get_single() else { return None };
    let world_pos = camera.viewport_to_world_2d(cam_transform, cursor_pos).ok()?;
    let world_radius = screen_radius * projection.scale;
    Some(([world_pos.x, world_pos.y], world_radius))
}

/// 查找半径内的所有省份（旧版本，保留备用）
#[allow(dead_code)]
fn find_provinces_in_radius(
    pos: [f32; 2],
    radius: f32,
    map: &MapResource,
) -> Vec<u32> {
    let mut result = Vec::new();
    let radius_sq = radius * radius;

    for prov in &map.0.provinces {
        // 处理环绕：检查多个副本
        for x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
            let cx = prov.centroid[0] + x_off;
            let dx = cx - pos[0];
            let dy = prov.centroid[1] - pos[1];
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= radius_sq {
                if !result.contains(&prov.id) {
                    result.push(prov.id);
                }
                break;
            }
        }
    }

    result
}

/// 使用空间哈希快速查找半径内的所有省份
fn find_provinces_in_radius_fast(
    pos: [f32; 2],
    radius: f32,
    map: &MapResource,
    spatial_hash: &SpatialHash,
) -> Vec<u32> {
    let radius_sq = radius * radius;
    let mut result = Vec::new();

    // 使用空间哈希获取候选省份
    let candidates = spatial_hash.find_in_radius(pos, radius);

    // 精确距离过滤
    for &prov_id in &candidates {
        // 找到对应的省份
        let prov_index = u32_to_usize(prov_id);
        let Some(prov) = map.0.provinces.get(prov_index) else {
            continue;
        };

        // 处理环绕：检查多个副本
        for x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
            let cx = prov.centroid[0] + x_off;
            let dx = cx - pos[0];
            let dy = prov.centroid[1] - pos[1];
            let dist_sq = dx * dx + dy * dy;

            if dist_sq <= radius_sq {
                result.push(prov_id);
                break;
            }
        }
    }

    result
}
