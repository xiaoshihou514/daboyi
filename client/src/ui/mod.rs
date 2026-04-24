use bevy::prelude::*;
use bevy::window::Window;
use bevy_egui::{egui, EguiContexts};

use crate::state::AppState;

pub mod egui_ui;

pub struct UiPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct UiPass;

const FONT_PATH: &str = "fonts/NotoSansCJKsc-Regular.otf";
const PROVINCE_FONT_PATH: &str = "fonts/LXGWWenKai-Bold.ttf";
const CJK_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");
const LOADING_BACKGROUND_BYTES: &[u8] = include_bytes!("../../../assets/loading.jpg");
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

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Clone)]
pub enum LoadingStage {
    Pending,
    Working { label: String, progress: f32 },
    Ready,
    Failed(String),
}

impl Default for LoadingStage {
    fn default() -> Self {
        Self::Pending
    }
}

impl LoadingStage {
    fn label(&self, fallback: &str) -> String {
        match self {
            Self::Pending => fallback.to_string(),
            Self::Working { label, .. } => label.clone(),
            Self::Ready => "已完成".to_string(),
            Self::Failed(message) => format!("失败：{message}"),
        }
    }

    fn progress(&self) -> f32 {
        match self {
            Self::Pending => 0.0,
            Self::Working { progress, .. } => *progress,
            Self::Ready | Self::Failed(_) => 1.0,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }
}

#[derive(Resource, Default)]
pub struct LoadingProgress {
    pub map: LoadingStage,
    pub terrain: LoadingStage,
    pub terrain_adjacency: LoadingStage,
    pub borders: LoadingStage,
    pub labels: LoadingStage,
    pub editor: LoadingStage,
}

impl LoadingProgress {
    fn overall_progress(&self) -> f32 {
        let stages = [
            &self.map,
            &self.terrain,
            &self.terrain_adjacency,
            &self.borders,
            &self.labels,
            &self.editor,
        ];
        stages.iter().map(|stage| stage.progress()).sum::<f32>() / stages.len() as f32
    }

    fn status_text(&self) -> String {
        let ordered = [
            ("省份地图", &self.map),
            ("地形/河流", &self.terrain),
            ("地形邻接", &self.terrain_adjacency),
            ("边界网格", &self.borders),
            ("标签缓存", &self.labels),
            ("编辑器", &self.editor),
        ];
        for (name, stage) in ordered {
            if stage.is_failed() || !stage.is_ready() {
                return format!("{name}：{}", stage.label("等待开始"));
            }
        }
        "加载完毕".to_string()
    }

    fn has_error(&self) -> bool {
        [
            &self.map,
            &self.terrain,
            &self.terrain_adjacency,
            &self.borders,
            &self.labels,
            &self.editor,
        ]
        .iter()
        .any(|stage| stage.is_failed())
    }

    fn all_ready(&self) -> bool {
        [
            &self.map,
            &self.terrain,
            &self.terrain_adjacency,
            &self.borders,
            &self.labels,
            &self.editor,
        ]
        .iter()
        .all(|stage| stage.is_ready())
    }
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UiInputBlock>()
            .init_resource::<LoadingProgress>()
            .add_systems(
                Startup,
                (
                    load_ui_assets,
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
            )
            .add_systems(
                Update,
                (loading_ui_system, finish_loading_when_ready)
                    .in_set(UiPass)
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

fn load_ui_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
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

fn loading_ui_system(
    mut contexts: EguiContexts,
    progress: Res<LoadingProgress>,
    mut background_texture: Local<Option<egui::TextureHandle>>,
) {
    let Some(ctx) = contexts.try_ctx_mut() else {
        return;
    };
    if background_texture.is_none() {
        if let Ok(image) = image::load_from_memory(LOADING_BACKGROUND_BYTES) {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            *background_texture = Some(ctx.load_texture(
                "loading-background",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    let total_progress = progress.overall_progress();
    let has_error = progress.has_error();

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(egui::Color32::BLACK))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            if let Some(background_texture) = background_texture.as_ref() {
                ui.put(
                    rect,
                    egui::widgets::Image::new(egui::load::SizedTexture::new(
                        background_texture.id(),
                        rect.size(),
                    )),
                );
            }
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(72));
        });

    egui::TopBottomPanel::bottom("loading-progress-bar")
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(196))
                .inner_margin(egui::Margin::same(18.0)),
        )
        .show(ctx, |ui| {
            ui.heading("加载地图资源");
            ui.add_space(6.0);
            ui.label(progress.status_text());
            if has_error {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 190, 140),
                    "加载未完成，请检查错误信息与日志。",
                );
            }
            ui.add_space(8.0);
            ui.add(
                egui::ProgressBar::new(total_progress)
                    .desired_width(ui.available_width() - 8.0)
                    .text(format!("{:.0}%", total_progress * 100.0)),
            );
        });
}

fn finish_loading_when_ready(
    progress: Res<LoadingProgress>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if progress.all_ready() {
        next_state.set(AppState::Editing);
    }
}
