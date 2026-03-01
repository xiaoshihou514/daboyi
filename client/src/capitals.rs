use bevy::prelude::*;

use crate::map::{MapMode, MapResource};
use crate::net::LatestGameState;
use crate::state::AppState;
use crate::ui::CjkFont;

/// Minimum province count a country must have to be visible at a given zoom level.
/// Show if: province_count as f32 >= camera_scale * LOD_K
/// scale=0.15 → need ≥30; scale=0.01 → need ≥2; scale=0.002 → need ≥0.4 (all)
const LOD_K: f32 = 200.0;

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

/// Province count of the owning country — drives LOD visibility.
#[derive(Component)]
struct CapitalCountrySize(u32);

/// Whether this marker is the star (true) or name label (false) — drives scale target.
#[derive(Component)]
struct CapitalIsStar(bool);

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
        let size = *province_counts.get(country.tag.as_str()).unwrap_or(&0);

        // Spawn with scale=1 — update_capitals_scale sets the real scale each frame.
        commands.spawn((
            Text2d::new("★"),
            TextFont { font: cjk.clone(), font_size: 48.0, ..default() },
            TextColor(Color::srgba(1.0, 0.9, 0.0, 0.95)),
            Transform::from_xyz(x, y, 1.5),
            Visibility::Hidden,
            CapitalMarker,
            CapitalCountrySize(size),
            CapitalIsStar(true),
        ));

        commands.spawn((
            Text2d::new(country.name.clone()),
            TextFont { font: cjk.clone(), font_size: 36.0, ..default() },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
            Transform::from_xyz(x, y - 1.5, 1.5),
            Visibility::Hidden,
            CapitalMarker,
            CapitalCountrySize(size),
            CapitalIsStar(false),
        ));
    }
}

/// Every frame: update scale (zoom-aware) and visibility (mode + LOD) for all capital entities.
fn update_capitals_scale(
    mode: Res<MapMode>,
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    mut markers: Query<
        (&mut Transform, &mut Visibility, &CapitalCountrySize, &CapitalIsStar),
        With<CapitalMarker>,
    >,
) {
    let Ok(proj) = camera_q.get_single() else { return };
    let cam_scale = proj.scale;
    let political = *mode == MapMode::Political;

    for (mut transform, mut vis, CapitalCountrySize(size), CapitalIsStar(is_star)) in
        markers.iter_mut()
    {
        // LOD: hide if country is too small for current zoom level.
        let lod_ok = (*size as f32) >= cam_scale * LOD_K;
        *vis = if political && lod_ok {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        // Scale so the entity appears at a constant screen-space pixel size.
        // entity_scale = target_screen_px * cam_scale / font_size
        let entity_scale = if *is_star {
            12.0 * cam_scale / 48.0
        } else {
            10.0 * cam_scale / 36.0
        };
        transform.scale = Vec3::splat(entity_scale);
    }
}
