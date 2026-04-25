#![allow(clippy::too_many_arguments)]
/// Camera controls and province click detection.
use bevy::prelude::*;

use super::{MapResource, SelectedProvince, MAP_WIDTH};
use crate::editor::BrushTool;
use crate::map::color::point_in_province;
use crate::ui::UiInputBlock;

/// Camera pan (middle-click drag) and zoom (scroll wheel).
pub fn camera_controls(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut scroll_evts: EventReader<bevy::input::mouse::MouseWheel>,
    mut motion_evts: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera_q: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
    windows: Query<&Window>,
    keys: Res<ButtonInput<KeyCode>>,
    brush: Res<BrushTool>,
    ui_input_block: Res<UiInputBlock>,
) {
    if ui_input_block.0 {
        motion_evts.clear();
        scroll_evts.clear();
        return;
    }

    let Ok((mut transform, mut projection)) = camera_q.get_single_mut() else {
        return;
    };

    if mouse_input.pressed(MouseButton::Middle) {
        for ev in motion_evts.read() {
            transform.translation.x -= ev.delta.x * projection.scale;
            transform.translation.y += ev.delta.y * projection.scale;
        }
        // Wrap x so the camera stays in [-180, 180) — map copies cover the rest.
        let half = MAP_WIDTH / 2.0;
        if transform.translation.x >= half {
            transform.translation.x -= MAP_WIDTH;
        } else if transform.translation.x < -half {
            transform.translation.x += MAP_WIDTH;
        }
    } else {
        motion_evts.clear();
    }

    let brush_resize_modifier =
        brush.enabled && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight));
    for ev in scroll_evts.read() {
        if brush_resize_modifier {
            continue;
        }
        let zoom_factor = 1.0 - ev.y * 0.1;
        projection.scale *= zoom_factor.clamp(0.5, 2.0);
        projection.scale = projection.scale.clamp(0.002, 0.15);
    }

    // Clamp Y so the viewport edges never leave the map (±90° latitude).
    // half_h: how many world-space degrees the viewport covers from center to edge.
    let window_height = windows.get_single().map(|w| w.height()).unwrap_or(1080.0);
    let half_h = projection.scale * (window_height / 2.0);
    let y_min = (-57.0 + half_h).min(0.0);
    let y_max = (77.0 - half_h).max(0.0);
    transform.translation.y = transform.translation.y.clamp(y_min, y_max);
}

/// Detect the province under the cursor.
/// Runs on initial left-click, and also on every frame during a drag (brush mode).
pub fn province_click(
    mouse_input: Res<ButtonInput<MouseButton>>,
    cursor_moved: EventReader<CursorMoved>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Option<Res<MapResource>>,
    brush: Res<BrushTool>,
    mut selected: ResMut<SelectedProvince>,
    ui_input_block: Res<UiInputBlock>,
) {
    if brush.enabled || ui_input_block.0 {
        return;
    }
    // Run on initial press, or while held and cursor has moved (brush drag).
    let should_run = mouse_input.just_pressed(MouseButton::Left)
        || (mouse_input.pressed(MouseButton::Left) && !cursor_moved.is_empty());
    if !should_run {
        return;
    }
    let Some(map) = map else { return };
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) else {
        return;
    };

    let py = world_pos.y;
    // Wrap x into map coordinate range [-180, 180) to hit-test any copy.
    let px = world_pos.x - (((world_pos.x + 180.0) / MAP_WIDTH).floor() * MAP_WIDTH);

    // Iterate in reverse so CN provinces (higher z, appended later) take priority.
    for mp in map.0.provinces.iter().rev() {
        if mp.boundary.is_empty() {
            continue;
        }
        // Bounding box pre-filter.
        let ring = &mp.boundary[0];
        let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
        let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
        for pt in ring {
            min_x = min_x.min(pt[0]);
            max_x = max_x.max(pt[0]);
            min_y = min_y.min(pt[1]);
            max_y = max_y.max(pt[1]);
        }
        if px < min_x || px > max_x || py < min_y || py > max_y {
            continue;
        }
        if point_in_province(px, py, mp) {
            selected.0 = Some(mp.id);
            return;
        }
    }
    // Only clear selection on fresh click, not on drag (keeps last valid selection).
    if mouse_input.just_pressed(MouseButton::Left) {
        selected.0 = None;
    }
}
