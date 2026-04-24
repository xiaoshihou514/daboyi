use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use bevy_egui::EguiPlugin;

mod capitals;
mod editor;
mod labels;
mod map;
mod memory;
mod state;
mod terrain;
mod ui;
mod web_io;

fn app_plugins() -> impl PluginGroup {
    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: "大博弈 地图编辑器".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    #[cfg(target_arch = "wasm32")]
    let plugins = DefaultPlugins.set(window_plugin).set(AssetPlugin {
        meta_check: AssetMetaCheck::Never,
        ..Default::default()
    });
    #[cfg(not(target_arch = "wasm32"))]
    let plugins = DefaultPlugins.set(window_plugin).set(AssetPlugin {
        file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../assets").to_string(),
        meta_check: AssetMetaCheck::Never,
        ..Default::default()
    });
    plugins
}

fn main() {
    memory::MemoryMonitor::log_memory_usage("Application start");
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.1, 0.26, 0.55)))
        .add_plugins(app_plugins())
        .add_plugins(EguiPlugin)
        .init_state::<state::AppState>()
        .add_plugins(editor::EditorPlugin)
        .add_plugins(terrain::TerrainPlugin)
        .add_plugins(map::MapPlugin)
        .add_plugins(map::BordersPlugin)
        .add_plugins(capitals::CapitalsPlugin)
        .add_plugins(labels::LabelsPlugin)
        .add_plugins(ui::UiPlugin)
        .run();
}
