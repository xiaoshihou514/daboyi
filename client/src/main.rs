use bevy::prelude::*;

rust_i18n::i18n!("../locales", fallback = "zh");

mod map;
mod net;
mod terrain;
mod ui;

fn main() {
    rust_i18n::set_locale("zh");

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
