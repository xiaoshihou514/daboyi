//! 编辑器资源和状态管理

use crate::memory::MemoryMonitor;
use bevy::prelude::*;
use shared::AdminArea;
use std::collections::{HashMap, HashSet};

use crate::map::MapResource;
use crate::state::AppState;

mod admin;
mod brush;
mod save_load;
mod spatial;

pub use admin::*;
pub use brush::*;
pub use save_load::*;
pub use spatial::*;
// 类型别名
pub type AdminId = u32;
pub type CountryTag = String;
pub type ProvinceId = u32;

/// 所有国家列表
#[derive(Resource, Default)]
pub struct Countries(pub Vec<shared::EditorCountry>);

/// 所有行政区列表
#[derive(Resource, Default)]
pub struct AdminAreas(pub Vec<AdminArea>);

/// 省份 → 国家归属映射
#[derive(Resource, Default)]
pub struct CountryMap(pub HashMap<ProvinceId, CountryTag>);

/// 省份 → 行政区归属映射
#[derive(Resource, Default)]
pub struct AdminMap(pub HashMap<ProvinceId, AdminId>);

/// 当前选中的国家标签
#[derive(Resource, Default)]
pub struct ActiveCountry(pub Option<CountryTag>);

/// 当前选中的行政区 ID
#[derive(Resource, Default)]
pub struct ActiveAdmin(pub Option<AdminId>);

/// 下一个可用的行政区 ID（自增计数器）
#[derive(Resource)]
pub struct NextAdminId(pub AdminId);

impl Default for NextAdminId {
    fn default() -> Self {
        Self(1)
    }
}

/// 刷子工具状态
#[derive(Resource)]
pub struct BrushTool {
    /// 是否激活
    pub enabled: bool,
    /// 刷子半径（世界坐标度数）
    pub radius: f32,
    /// 橡皮擦模式（移除归属，而非分配）
    pub eraser_mode: bool,
}

impl Default for BrushTool {
    fn default() -> Self {
        Self {
            enabled: false,
            radius: 2.0,
            eraser_mode: false,
        }
    }
}

/// 鼠标拖拽状态（用于刷子）
#[derive(Resource, Default)]
pub struct DragState {
    pub is_dragging: bool,
    pub painted_provinces: HashSet<ProvinceId>,
}

#[derive(Resource, Default)]
pub struct NonPlayableProvinces(pub HashSet<ProvinceId>);

#[derive(Resource, Default)]
struct EditorStartupState {
    non_playable_ready: bool,
    spatial_ready: bool,
}

/// 编辑器插件
pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Countries>()
            .init_resource::<AdminAreas>()
            .init_resource::<CountryMap>()
            .init_resource::<AdminMap>()
            .init_resource::<ActiveCountry>()
            .init_resource::<ActiveAdmin>()
            .init_resource::<NextAdminId>()
            .init_resource::<BrushTool>()
            .init_resource::<BrushScratch>()
            .init_resource::<DragState>()
            .init_resource::<NonPlayableProvinces>()
            .init_resource::<EditorStartupState>()
            .init_resource::<SpatialHash>()
            .add_event::<LoadColoringEvent>()
            .add_event::<SaveColoringEvent>()
            .add_systems(Startup, (load_coloring_on_startup,))
            .add_systems(
                Update,
                (build_spatial_hash,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (brush_input_system,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (brush_cursor_system,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (validate_admin_assignments,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (handle_load_coloring, handle_save_coloring)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (populate_non_playable_provinces,).run_if(in_state(AppState::Editing)),
            )
            .add_systems(
                Update,
                (
                    populate_non_playable_provinces,
                    build_spatial_hash,
                    update_editor_loading_progress,
                )
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

/// 启动时加载着色文件
fn load_coloring_on_startup(mut commands: Commands) {
    load_coloring(&mut commands);
    // 监控资源大小
    MemoryMonitor::log_memory_usage("After loading coloring");
}

fn populate_non_playable_provinces(
    map: Option<Res<MapResource>>,
    mut non_playable_provinces: ResMut<NonPlayableProvinces>,
    mut startup_state: ResMut<EditorStartupState>,
) {
    let Some(map) = map else {
        return;
    };
    if startup_state.non_playable_ready && !map.is_added() {
        return;
    }

    non_playable_provinces.0 = map
        .0
        .provinces
        .iter()
        .filter(|province| province.topography.contains("wasteland"))
        .map(|province| province.id)
        .collect();
    startup_state.non_playable_ready = true;
}

fn build_spatial_hash(
    map: Option<Res<MapResource>>,
    mut spatial_hash: ResMut<SpatialHash>,
    mut startup_state: ResMut<EditorStartupState>,
) {
    let Some(map) = map else { return };
    if !map.is_changed() && !spatial_hash.is_added() {
        return;
    }
    MemoryMonitor::log_memory_usage("Before building spatial hash");
    *spatial_hash = SpatialHash::build(&map.0.provinces);
    MemoryMonitor::log_memory_usage("After building spatial hash");
    startup_state.spatial_ready = true;
}

fn update_editor_loading_progress(
    startup_state: Res<EditorStartupState>,
    mut progress: ResMut<crate::ui::LoadingProgress>,
) {
    if startup_state.non_playable_ready && startup_state.spatial_ready {
        progress.editor = crate::ui::LoadingStage::Ready;
    } else {
        progress.editor = crate::ui::LoadingStage::Working {
            label: "正在准备编辑器".to_string(),
            progress: 0.6,
        };
    }
}
