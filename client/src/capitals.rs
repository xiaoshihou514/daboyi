use bevy::prelude::*;

use crate::map::{MapMode, MapResource};
use crate::net::LatestGameState;
use crate::state::AppState;
use crate::ui::CjkFont;

pub struct CapitalsPlugin;

impl Plugin for CapitalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_capitals, update_capitals_scale).run_if(in_state(AppState::Playing)),
        );
    }
}

/// Tags a capital entity (star or name label) for lifecycle management.
#[derive(Component)]
pub struct CapitalMarker;

/// Drives EU4-style LOD: territory_factor = sqrt(province_count).
/// Name world-space size = territory_factor * NAME_BASE_SIZE.
/// Star is always constant screen-space size.
#[derive(Component)]
struct CapitalTerritoryFactor(f32);

/// Whether this marker is the star (true) or name label (false).
#[derive(Component)]
struct CapitalIsStar(bool);

/// Base world-space size (degrees) per unit of territory_factor for name labels.
const NAME_BASE_SIZE: f32 = 0.6;

fn spawn_capitals(
    mut commands: Commands,
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    cjk_res: Option<Res<CjkFont>>,
    mut last_fingerprint: Local<Vec<(String, u32)>>,
    existing: Query<Entity, With<CapitalMarker>>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };
    let Some(cjk_res) = cjk_res else { return };
    let cjk = cjk_res.0.clone();

    // Fingerprint: (tag, capital_province) — only re-spawn when this changes.
    let fingerprint: Vec<(String, u32)> = gs
        .countries
        .iter()
        .map(|c| (c.tag.clone(), c.capital_province))
        .collect();
    if *last_fingerprint == fingerprint {
        return;
    }
    *last_fingerprint = fingerprint;

    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    // Count provinces per country tag.
    let mut province_counts: std::collections::HashMap<&str, u32> =
        std::collections::HashMap::new();
    for prov in &gs.provinces {
        if let Some(owner) = prov.owner.as_deref() {
            *province_counts.entry(owner).or_insert(0) += 1;
        }
    }

    for country in &gs.countries {
        let cap_id = country.capital_province as usize;
        if cap_id >= map.0.provinces.len() {
            continue;
        }
        let centroid = map.0.provinces[cap_id].centroid;
        let x = centroid[0];
        let y = centroid[1];
        let size = *province_counts.get(country.tag.as_str()).unwrap_or(&1);
        let territory_factor = (size as f32).sqrt();

        // Name world-space size is fixed (not zoom-dependent) — proportional to territory.
        let name_world = territory_factor * NAME_BASE_SIZE;
        let name_scale = name_world / 36.0;
        let name_y = y - name_world * 0.6;

        commands.spawn((
            Text2d::new("★"),
            TextFont { font: cjk.clone(), font_size: 48.0, ..default() },
            TextColor(Color::srgba(1.0, 0.9, 0.0, 0.95)),
            Transform::from_xyz(x, y, 1.5),
            Visibility::Hidden,
            CapitalMarker,
            CapitalTerritoryFactor(territory_factor),
            CapitalIsStar(true),
        ));

        commands.spawn((
            Text2d::new(country.name.clone()),
            TextFont { font: cjk.clone(), font_size: 36.0, ..default() },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
            Transform::from_xyz(x, name_y, 1.5).with_scale(Vec3::splat(name_scale)),
            Visibility::Hidden,
            CapitalMarker,
            CapitalTerritoryFactor(territory_factor),
            CapitalIsStar(false),
        ));
    }
}

/// Every frame: update star scale (zoom-aware) and visibility (mode + EU4-style LOD).
fn update_capitals_scale(
    mode: Res<MapMode>,
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    mut markers: Query<
        (&mut Transform, &mut Visibility, &CapitalTerritoryFactor, &CapitalIsStar),
        With<CapitalMarker>,
    >,
) {
    let Ok(proj) = camera_q.get_single() else { return };
    let cam_scale = proj.scale;
    let political = *mode == MapMode::Political;

    for (mut transform, mut vis, CapitalTerritoryFactor(tf), CapitalIsStar(is_star)) in
        markers.iter_mut()
    {
        if *is_star {
            // Star: constant ~10 screen pixels, zoom-aware.
            let entity_scale = 10.0 * cam_scale / 48.0;
            transform.scale = Vec3::splat(entity_scale);

            // Show when territory is in a reasonable zoom window.
            let max_cam = tf / 40.0;
            let min_cam = tf / 800.0;
            *vis = if political && cam_scale > min_cam && cam_scale < max_cam {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else {
            // Name: world-space scale is fixed (set at spawn). Only update visibility.
            // Show when the name would appear between ~8px and ~150px on screen.
            let name_world = tf * NAME_BASE_SIZE;
            let screen_size = name_world / cam_scale;
            *vis = if political && screen_size > 8.0 && screen_size < 150.0 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}
