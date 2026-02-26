use bevy::prelude::*;

use crate::net::LatestGameState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_hud);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // HUD: date + tick counter in the top-left corner.
    commands.spawn((
        Text::new("Connecting…"),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        DateLabel,
    ));
}

#[derive(Component)]
struct DateLabel;

fn update_hud(state: Res<LatestGameState>, mut query: Query<&mut Text, With<DateLabel>>) {
    if let Some(gs) = &state.0 {
        for mut text in query.iter_mut() {
            *text = Text::new(format!(
                "Date: {}-{:02}-{:02}   Tick: {}",
                gs.date.year, gs.date.month, gs.date.day, gs.tick
            ));
        }
    }
}
