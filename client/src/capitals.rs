use bevy::prelude::*;

use crate::editor::{EditorCountries, MapColoring};
use crate::map::{MapMode, MapResource};
use crate::state::AppState;
use crate::ui::CjkFont;

pub struct CapitalsPlugin;

impl Plugin for CapitalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_capitals, update_capitals_scale).run_if(in_state(AppState::Editing)),
        );
    }
}

/// Tags a capital entity (star or name label) for lifecycle management.
#[derive(Component)]
pub struct CapitalMarker;

/// Drives EU4-style LOD: territory_factor = sqrt(province_count assigned to this country).
#[derive(Component)]
struct CapitalTerritoryFactor(f32);

/// Whether this marker is the star (true) or name label (false).
#[derive(Component)]
struct CapitalIsStar(bool);

/// Base world-space size (degrees) per unit of territory_factor for name labels.
const NAME_BASE_SIZE: f32 = 0.6;

fn spawn_capitals(
    mut commands: Commands,
    countries: Res<EditorCountries>,
    coloring: Res<MapColoring>,
    map: Option<Res<MapResource>>,
    cjk_res: Option<Res<CjkFont>>,
    mut last_fingerprint: Local<Vec<(String, Option<u32>)>>,
    existing: Query<Entity, With<CapitalMarker>>,
) {
    let Some(map) = map else { return };
    let Some(cjk_res) = cjk_res else { return };
    let cjk = cjk_res.0.clone();

    // Fingerprint: (tag, capital_province) — only re-spawn when this changes.
    let fingerprint: Vec<(String, Option<u32>)> = countries
        .0
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

    // Count province assignments per country tag.
    let mut province_counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for tag in coloring.assignments.values() {
        *province_counts.entry(tag.as_str()).or_insert(0) += 1;
    }

    for country in &countries.0 {
        let Some(cap_id) = country.capital_province else { continue };
        let cap_idx = cap_id as usize;
        if cap_idx >= map.0.provinces.len() {
            continue;
        }
        let centroid = map.0.provinces[cap_idx].centroid;
        let x = centroid[0];
        let y = centroid[1];
        let size = *province_counts.get(country.tag.as_str()).unwrap_or(&1);
        let territory_factor = (size as f32).sqrt();

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
            let entity_scale = 10.0 * cam_scale / 48.0;
            transform.scale = Vec3::splat(entity_scale);

            let max_cam = tf / 40.0;
            let min_cam = tf / 800.0;
            *vis = if political && cam_scale > min_cam && cam_scale < max_cam {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else {
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
