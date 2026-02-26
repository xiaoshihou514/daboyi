use bevy::prelude::*;

mod net;
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
        .add_plugins(ui::UiPlugin)
        .run();
}
