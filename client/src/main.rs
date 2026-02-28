use bevy::prelude::*;

rust_i18n::i18n!("../locales", fallback = "zh");

mod capitals;
mod map;
mod menu;
mod net;
mod state;
mod terrain;
mod ui;

fn main() {
    rust_i18n::set_locale("zh");

    // Deep ocean color (matches terrain.bin ocean polygons) for empty map background.
    let ocean_bg = ClearColor(Color::srgb(0.1, 0.26, 0.55));

    App::new()
        .insert_resource(ocean_bg)
        .add_plugins(DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Daboyi".into(),
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../assets").to_string(),
                ..default()
            })
        )
        .init_state::<state::AppState>()
        .init_resource::<state::PlayerCountry>()
        .add_plugins(net::NetPlugin)
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(map::MapPlugin)
        .add_plugins(capitals::CapitalsPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(menu::MenuPlugin)
        .run();
}
