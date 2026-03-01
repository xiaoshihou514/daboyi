use bevy::prelude::*;

use crate::map::{MapMode, MapResource};
use crate::net::LatestGameState;
use crate::state::AppState;

pub struct ArmiesPlugin;

impl Plugin for ArmiesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (spawn_army_labels, sync_army_visibility).run_if(in_state(AppState::Playing)),
        );
    }
}

/// Tags an army label entity for lifecycle management.
#[derive(Component)]
pub struct ArmyLabel;

/// Spawn (or re-spawn on tick change) text labels for all armies.
fn spawn_army_labels(
    mut commands: Commands,
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    mode: Res<MapMode>,
    mut last_tick: Local<u64>,
    existing: Query<Entity, With<ArmyLabel>>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };

    if gs.tick == *last_tick && !mode.is_changed() {
        return;
    }
    *last_tick = gs.tick;

    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }

    let visible = *mode == MapMode::Political;

    for army in &gs.armies {
        let pid = army.province_id as usize;
        if pid >= map.0.provinces.len() {
            continue;
        }
        let centroid = map.0.provinces[pid].centroid;
        // Offset slightly above province centroid so it doesn't clash with capital star.
        let x = centroid[0];
        let y = centroid[1] - 0.8;

        commands.spawn((
            Text2d::new(format!("⚔ {}", army.size)),
            TextFont {
                font_size: 3.0,
                ..default()
            },
            TextColor(Color::srgba(0.95, 0.95, 0.95, 1.0)),
            Transform::from_xyz(x, y, 2.0),
            Visibility::from(if visible { Visibility::Visible } else { Visibility::Hidden }),
            ArmyLabel,
        ));
    }
}

/// Show/hide army labels when map mode changes.
fn sync_army_visibility(
    mode: Res<MapMode>,
    mut labels: Query<&mut Visibility, With<ArmyLabel>>,
) {
    if !mode.is_changed() {
        return;
    }
    let visible = *mode == MapMode::Political;
    for mut vis in labels.iter_mut() {
        *vis = if visible { Visibility::Visible } else { Visibility::Hidden };
    }
}
