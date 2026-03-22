use bevy::prelude::*;

mod capitals;
mod editor;
mod map;
mod state;
mod terrain;
mod ui;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.26, 0.55)))
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "大博弈 地图编辑器".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .set(AssetPlugin {
                    file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../assets").to_string(),
                    ..Default::default()
                }),
        )
        .init_state::<state::AppState>()
        .init_resource::<editor::MapColoring>()
        .init_resource::<editor::EditorCountries>()
        .init_resource::<editor::AdminAreas>()
        .init_resource::<editor::AdminAssignments>()
        .init_resource::<editor::ActiveCountry>()
        .init_resource::<editor::ActiveArea>()
        .init_resource::<editor::NextAreaId>()
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(map::MapPlugin)
        .add_plugins(map::BordersPlugin)
        .add_plugins(capitals::CapitalsPlugin)
        .add_plugins(ui::UiPlugin)
        .run();
}
