use bevy::prelude::*;
use bevy::window::Window;
use bevy_egui::{egui, EguiContexts};

use crate::state::AppState;

pub mod egui_ui;

pub struct UiPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct UiPass;

const FONT_PATH: &str = "NotoSansCJKsc-Regular.otf";
const PROVINCE_FONT_PATH: &str = "演示秋鸿楷.ttf";
const CJK_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");
const INITIAL_CAMERA_SCALE: f32 = 0.15;
const INITIAL_CAMERA_X: f32 = 105.0;
const INITIAL_CAMERA_Y: f32 = 35.0;

/// Holds the loaded CJK font handle (Simplified Chinese).
#[derive(Resource)]
pub struct CjkFont(pub Handle<Font>);

#[derive(Resource)]
pub struct ProvinceLabelFont(pub Handle<Font>);

#[derive(Resource, Default)]
pub struct UiInputBlock(pub bool);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiInputBlock>()
            .add_systems(
                Startup,
                (
                    load_font,
                    setup_camera,
                    enable_ime_input,
                    configure_egui_fonts,
                ),
            )
            .add_systems(
                Update,
                egui_ui::egui_ui_system
                    .in_set(UiPass)
                    .run_if(in_state(AppState::Editing)),
            );
    }
}

fn load_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load(FONT_PATH);
    let province_font = asset_server.load(PROVINCE_FONT_PATH);
    commands.insert_resource(CjkFont(font));
    commands.insert_resource(ProvinceLabelFont(province_font));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Transform::from_xyz(INITIAL_CAMERA_X, INITIAL_CAMERA_Y, 0.0),
        OrthographicProjection {
            scale: INITIAL_CAMERA_SCALE,
            ..OrthographicProjection::default_2d()
        },
    ));
}

fn enable_ime_input(mut windows: Query<&mut Window>) {
    let Ok(mut window) = windows.get_single_mut() else {
        return;
    };
    window.ime_enabled = true;
}

fn configure_egui_fonts(mut contexts: EguiContexts) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "noto-sans-cjk".to_string(),
        egui::FontData::from_static(CJK_FONT_BYTES),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, "noto-sans-cjk".to_string());
    }

    ctx.set_fonts(fonts);
}
