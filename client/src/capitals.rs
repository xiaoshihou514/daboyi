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
            (spawn_capitals, sync_capitals_visibility).run_if(in_state(AppState::Playing)),
        );
    }
}

/// Tags a capital star entity so we can find/despawn them later.
#[derive(Component)]
pub struct CapitalMarker;

/// Spawn (or re-spawn on state change) a "★" marker at each country's capital province centroid.
fn spawn_capitals(
    mut commands: Commands,
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    mode: Res<MapMode>,
    cjk_res: Option<Res<CjkFont>>,
    // Store fingerprint of (tag, capital_province) pairs — only respawn if this changes.
    mut last_fingerprint: Local<Vec<(String, u32)>>,
    existing: Query<Entity, With<CapitalMarker>>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };
    let Some(cjk_res) = cjk_res else { return };
    let cjk = cjk_res.0.clone();

    // Build fingerprint from country capital assignments.
    let fingerprint: Vec<(String, u32)> = gs
        .countries
        .iter()
        .map(|c| (c.tag.clone(), c.capital_province))
        .collect();

    if *last_fingerprint == fingerprint {
        return;
    }
    *last_fingerprint = fingerprint;

    // Despawn old markers.
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    let visible = *mode == MapMode::Political;

    // Render at high font_size (48px) for sharp glyphs, then scale the
    // transform down so the label appears at the correct world-space size.
    // Camera scale=0.1 → 1 world unit = 10 screen px. Target: star ≈ 3 world
    // units tall → scale = 3/48 ≈ 0.0625.
    let star_scale = Vec3::splat(3.0 / 48.0);
    let name_scale = Vec3::splat(2.0 / 36.0);

    for country in &gs.countries {
        let cap_id = country.capital_province as usize;
        if cap_id >= map.0.provinces.len() {
            continue;
        }
        let centroid = map.0.provinces[cap_id].centroid;
        let x = centroid[0];
        let y = centroid[1];

        // Star marker at capital centroid.
        commands.spawn((
            Text2d::new("★"),
            TextFont {
                font: cjk.clone(),
                font_size: 48.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.0, 0.95)),
            Transform {
                translation: Vec3::new(x, y, 1.5),
                scale: star_scale,
                ..default()
            },
            Visibility::from(if visible { Visibility::Visible } else { Visibility::Hidden }),
            CapitalMarker,
        ));

        // Country name label slightly below the star.
        commands.spawn((
            Text2d::new(country.name.clone()),
            TextFont {
                font: cjk.clone(),
                font_size: 36.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
            Transform {
                translation: Vec3::new(x, y - 1.5, 1.5),
                scale: name_scale,
                ..default()
            },
            Visibility::from(if visible { Visibility::Visible } else { Visibility::Hidden }),
            CapitalMarker,
        ));
    }
}

/// Show/hide capital stars whenever the map mode changes.
fn sync_capitals_visibility(
    mode: Res<MapMode>,
    mut stars: Query<&mut Visibility, With<CapitalMarker>>,
) {
    if !mode.is_changed() {
        return;
    }
    let visible = *mode == MapMode::Political;
    for mut vis in stars.iter_mut() {
        *vis = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}
