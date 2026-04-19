//! 编辑器资源和状态管理

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
            .init_resource::<DragState>()
            .init_resource::<SpatialHash>()
            .add_systems(Startup, load_coloring_on_startup)
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
            );
    }
}

/// 启动时加载着色文件
fn load_coloring_on_startup(mut commands: Commands) {
    load_coloring(&mut commands);
}

fn build_spatial_hash(map: Option<Res<MapResource>>, mut spatial_hash: ResMut<SpatialHash>) {
    let Some(map) = map else { return };
    if !map.is_changed() && !spatial_hash.is_added() {
        return;
    }
    *spatial_hash = SpatialHash::build(&map.0.provinces);
}
