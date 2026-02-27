use bevy::prelude::*;

mod map;
mod net;
mod terrain;
mod ui;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Daboyi".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(net::NetPlugin)
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(map::MapPlugin)
        .add_plugins(ui::UiPlugin)
        .run();
}
