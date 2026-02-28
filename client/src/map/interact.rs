/// Camera controls, province click detection, and map mode switching.
use bevy::prelude::*;

use super::{MapMode, MapResource, SelectedProvince, MAP_WIDTH};
use crate::map::color::point_in_province;

/// Camera pan (right-click drag) and zoom (scroll wheel).
pub fn camera_controls(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut scroll_evts: EventReader<bevy::input::mouse::MouseWheel>,
    mut motion_evts: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera_q: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_q.get_single_mut() else {
        return;
    };

    if mouse_input.pressed(MouseButton::Right) {
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
        // Clamp y to equirectangular pole-to-pole range (±90°).
        transform.translation.y = transform.translation.y.clamp(-90.0, 90.0);
    } else {
        motion_evts.clear();
    }

    for ev in scroll_evts.read() {
        let zoom_factor = 1.0 - ev.y * 0.1;
        projection.scale *= zoom_factor.clamp(0.5, 2.0);
        // Max scale 0.1 ensures the visible width ≤ MAP_WIDTH on screens up to ~3600px.
        projection.scale = projection.scale.clamp(0.01, 0.1);
    }
}

/// Detect left-click on a province. Uses bounding box pre-filter.
/// Iterates in reverse so CN provinces (higher z, later IDs) are checked first.
pub fn province_click(
    mouse_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Option<Res<MapResource>>,
    mut selected: ResMut<SelectedProvince>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
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
    selected.0 = None;
}

/// Keyboard shortcuts: 1 = Province, 2 = Population, 3 = Production, 4 = Terrain, 5 = Political.
pub fn map_mode_switch(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<MapMode>) {
    if keys.just_pressed(KeyCode::Digit1) {
        *mode = MapMode::Province;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        *mode = MapMode::Population;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        *mode = MapMode::Production;
    }
    if keys.just_pressed(KeyCode::Digit4) {
        *mode = MapMode::Terrain;
    }
    if keys.just_pressed(KeyCode::Digit5) {
        *mode = MapMode::Political;
    }
}
