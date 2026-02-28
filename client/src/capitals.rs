use bevy::prelude::*;

use crate::map::{MapMode, MapResource};
use crate::net::LatestGameState;
use crate::state::AppState;

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
    mut last_tick: Local<u64>,
    existing: Query<Entity, With<CapitalMarker>>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };

    // Only re-spawn when the game state tick changes.
    if gs.tick == *last_tick {
        return;
    }
    *last_tick = gs.tick;

    // Despawn old markers.
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    let visible = *mode == MapMode::Political;

    for country in &gs.countries {
        let cap_id = country.capital_province as usize;
        if cap_id >= map.0.provinces.len() {
            continue;
        }
        let centroid = map.0.provinces[cap_id].centroid;
        let x = centroid[0];
        let y = centroid[1];

        // Star marker: world-space Text2d at the capital province centroid.
        commands.spawn((
            Text2d::new("★"),
            TextFont {
                font_size: 4.0,
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.0, 0.95)),
            Transform::from_xyz(x, y, 1.5),
            Visibility::from(if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            }),
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
