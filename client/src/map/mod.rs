pub mod borders;
mod color;
mod interact;
mod material;

use crate::memory::MemoryMonitor;
#[cfg(target_arch = "wasm32")]
use crate::web_io::{fetch_bytes, fetch_text};
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::{RenderAssetUsages, RenderAssets};
use bevy::render::render_resource::{Extent3d, ImageDataLayout, TextureDimension, TextureFormat};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderSet};
use bevy::sprite::Material2dPlugin;
use material::{ProvinceMapMaterial, ProvinceMapParams};
use shared::map::{MapData, ProvinceAdjacencyCache};
use std::collections::{HashMap, HashSet};
#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, Mutex};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

use crate::editor::{
    province_country_tag, visible_admin_id_for_province, ActiveAdmin, ActiveCountry, AdminAreas,
    AdminMap, Countries, CountryMap, NonPlayableProvinces,
};
use crate::state::AppState;
use crate::ui::{LoadingProgress, LoadingStage};
pub use borders::BordersPlugin;
use color::{brighten, owner_color_rgba, terrain_province_color};
pub use interact::camera_controls;
use interact::province_click;

pub const MAP_BIN_PATH: &str = "assets/map.bin";
/// Equal Earth x-range width: longitude ±180° maps exactly to x ∈ [-180, 180].
pub const MAP_WIDTH: f32 = 360.0;
/// Path to the province adjacency cache file.
const ADJACENCY_BIN_PATH: &str = "assets/province_adjacency.bin";

/// Province tag (lowercased) → Chinese display name.
#[derive(Resource, Default)]
pub struct ProvinceNames(pub HashMap<String, String>);

/// 颜色更新队列资源，用于批量处理颜色更新
#[derive(Resource, Default)]
pub struct ColorUpdateQueue {
    /// 需要更新颜色的省份ID
    pub queue: HashSet<usize>,
    /// 是否需要完全重建
    pub full_rebuild: bool,
}

pub struct MapPlugin;

/// Loaded map geometry, available as a Bevy resource.
#[derive(Resource)]
pub struct MapResource(pub MapData);

#[derive(Resource, Default)]
pub struct MissingMapMessage(pub Option<String>);

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
struct MapLoadTask(pub Option<Arc<Mutex<Option<Result<LoadedMapAssets, String>>>>>);

#[derive(Resource, Default)]
struct PendingMapBuild(pub Option<LoadedMapAssets>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default, PartialEq, Eq)]
enum NativeMapLoadPhase {
    #[default]
    NotStarted,
    Loading,
    Building,
    Done,
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default, PartialEq, Eq)]
enum WasmMapBuildPhase {
    #[default]
    NotStarted,
    Building,
    Done,
}

/// Currently selected province.
#[derive(Resource, Default)]
pub struct SelectedProvince(pub Option<u32>);

/// Map display mode.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MapMode {
    #[default]
    Map,
}

struct LoadedMapAssets {
    map_data: MapData,
    province_names: ProvinceNames,
    adjacency: Option<ProvinceAdjacencyCache>,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self {
            MapMode::Map => "地图",
        };
        write!(f, "{s}")
    }
}

/// Width of the province colour texture in texels (columns).
/// Using 256 keeps the texture within the guaranteed 8192-texel limit and
/// gives a height of ceil(21111/256) = 83 for the expected province count.
const TEX_WIDTH: usize = 256;

/// Province colour pixel data shared between main world and render world.
///
/// Instead of going through `Assets<Image>::get_mut` (which creates a NEW `GpuImage`
/// and breaks the material bind group), the render world writes this buffer directly
/// to the EXISTING GPU texture via `RenderQueue::write_texture`.  The bind group is
/// never stale.
#[derive(Resource, Clone)]
pub struct ProvinceColorBuffer {
    /// Raw RGBA8 pixels: length = tex_width * tex_height * 4.
    pub data: Vec<u8>,
    /// Monotonic version incremented on every color update.  The render world
    /// system skips the write when this equals its `last_version` local.
    pub version: u64,
    /// Used by the render world to look up the `GpuImage` in `RenderAssets<GpuImage>`.
    pub image_handle: Handle<Image>,
    pub tex_width: u32,
    pub tex_height: u32,
}

impl ExtractResource for ProvinceColorBuffer {
    type Source = ProvinceColorBuffer;
    fn extract_resource(source: &Self::Source) -> Self {
        MemoryMonitor::log_estimated_allocation(
            "ProvinceColorBuffer render clone",
            source.data.capacity(),
            0,
            "ExtractResource clones the full CPU-side texture buffer into the render world",
        );
        source.clone()
    }
}

/// Tracks the last coloring state to avoid redundant recoloring.
#[derive(Resource, Default)]
struct LastColorState {
    mode: Option<MapMode>,
    selected: Option<u32>,
    active_admin: Option<u32>,
    active_country: Option<String>,
    coloring_version: u64,
}

#[derive(Resource, Clone)]
struct ProvinceMaterialHandle(pub Handle<ProvinceMapMaterial>);

/// Incremented when editor data changes require a full recolor.
#[derive(Resource, Default)]
pub struct ColoringVersion(pub u64);

/// Province IDs whose colours need a targeted texture update this frame.
#[derive(Resource, Default)]
pub struct PendingProvinceRecolor(pub HashSet<u32>);

/// Incremented when ownership or admin assignments change and borders must rebuild.
#[derive(Resource, Default)]
pub struct BorderVersion(pub u64);

/// Set while edits have changed border semantics but the expensive rebuild is intentionally deferred.
#[derive(Resource, Default)]
pub struct BorderDirty(pub bool);

/// Debounce for border rebuilds: fires 150 ms after the last brush stroke ends.
#[derive(Resource)]
pub struct PaintDebounce {
    timer: Timer,
    /// A border rebuild needs to happen once the cooldown expires.
    pub pending_border: bool,
}

impl Default for PaintDebounce {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.15, TimerMode::Once),
            pending_border: false,
        }
    }
}

impl PaintDebounce {
    /// (Re)start the 150 ms countdown after a stroke ends.
    pub fn kick(&mut self) {
        self.timer.reset();
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        let app = app
            .add_plugins(Material2dPlugin::<ProvinceMapMaterial>::default())
            .add_plugins(ExtractResourcePlugin::<ProvinceColorBuffer>::default())
            .insert_resource(SelectedProvince::default())
            .insert_resource(MapMode::default())
            .insert_resource(MissingMapMessage::default())
            .insert_resource(LastColorState::default())
            .insert_resource(ColoringVersion::default())
            .insert_resource(PendingProvinceRecolor::default())
            .insert_resource(BorderVersion::default())
            .insert_resource(BorderDirty::default())
            .insert_resource(PaintDebounce::default())
            .insert_resource(ProvinceNames::default())
            .insert_resource(ColorUpdateQueue::default())
            .add_systems(
                Update,
                flush_paint_debounce.run_if(in_state(AppState::Editing)),
            )
            .add_systems(
                Update,
                color_provinces
                    .run_if(in_state(AppState::Editing))
                    .after(flush_paint_debounce),
            )
            .add_systems(
                Update,
                (camera_controls,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                update_zoom_visuals
                    .run_if(in_state(AppState::Editing))
                    .after(camera_controls),
            )
            .add_systems(
                Update,
                (province_click,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            );
        #[cfg(not(target_arch = "wasm32"))]
        let app = app
            .init_resource::<PendingMapBuild>()
            .init_resource::<NativeMapLoadPhase>()
            .add_systems(
                Update,
                (
                    queue_native_map_load,
                    load_map_native,
                    build_loaded_map_native,
                )
                    .run_if(in_state(AppState::Loading)),
            );
        #[cfg(target_arch = "wasm32")]
        let app = app
            .init_resource::<MapLoadTask>()
            .init_resource::<PendingMapBuild>()
            .init_resource::<WasmMapBuildPhase>()
            .add_systems(Startup, start_map_load)
            .add_systems(
                Update,
                (poll_map_load, build_loaded_map).run_if(in_state(AppState::Loading)),
            );

        // Register the render-world system that writes the province colour buffer
        // directly onto the existing GPU texture via `write_texture`.
        let render_app = app.sub_app_mut(RenderApp);
        render_app.add_systems(
            Render,
            update_province_texture_gpu.in_set(RenderSet::PrepareResources),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn queue_native_map_load(
    phase: Res<NativeMapLoadPhase>,
    mut progress: ResMut<LoadingProgress>,
    mut commands: Commands,
) {
    if !matches!(*phase, NativeMapLoadPhase::NotStarted) {
        return;
    }
    progress.map = LoadingStage::Working {
        label: "正在读取省份地图与名称".to_string(),
        progress: 0.2,
    };
    commands.insert_resource(NativeMapLoadPhase::Loading);
}

#[cfg(not(target_arch = "wasm32"))]
fn load_map_assets_native() -> Result<LoadedMapAssets, String> {
    MemoryMonitor::log_memory_usage("Before loading map");
    MemoryMonitor::log_detailed_memory_usage("Before loading map");
    let map_data = match MapData::load(MAP_BIN_PATH) {
        Ok(d) => d,
        Err(error) => return Err(format!("Failed to load {MAP_BIN_PATH}: {error}")),
    };
    let province_names =
        load_province_names_from_text(std::fs::read_to_string("assets/province_names.tsv").ok());
    
    let adjacency = match ProvinceAdjacencyCache::load(ADJACENCY_BIN_PATH) {
        Ok(cache) => Some(cache),
        Err(e) => {
            bevy::log::warn!(target: "daboyi::startup", "Failed to load {ADJACENCY_BIN_PATH}: {e}");
            None
        }
    };
    
    Ok(LoadedMapAssets {
        map_data,
        province_names,
        adjacency,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn load_map_native(
    phase: Res<NativeMapLoadPhase>,
    mut pending_build: ResMut<PendingMapBuild>,
    mut progress: ResMut<LoadingProgress>,
    mut missing_map_message: ResMut<MissingMapMessage>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    if !matches!(*phase, NativeMapLoadPhase::Loading) {
        return;
    }
    match load_map_assets_native() {
        Ok(loaded) => {
            progress.map = LoadingStage::Working {
                label: "正在构建省份网格".to_string(),
                progress: 0.72,
            };
            pending_build.0 = Some(loaded);
            commands.insert_resource(NativeMapLoadPhase::Building);
        }
        Err(error) => {
            bevy::log::error!(target: "daboyi::startup", "{error}");
            bevy::log::warn!(
                target: "daboyi::startup",
                "Map will not be rendered. Run mapgen first."
            );
            missing_map_message.0 = Some(
                "未找到 assets/map.bin。请先运行 mapgen 生成基础地图，然后再加载着色文件。"
                    .to_string(),
            );
            progress.map = LoadingStage::Failed(error);
            next_state.set(AppState::Editing);
        }
    }
}

fn load_province_names_from_text(content: Option<String>) -> ProvinceNames {
    let mut province_name_map: HashMap<String, String> = HashMap::new();
    if let Some(content) = content {
        for line in content.lines() {
            let mut parts = line.splitn(2, '\t');
            if let (Some(en), Some(zh)) = (parts.next(), parts.next()) {
                province_name_map.insert(en.trim().to_lowercase(), zh.trim().to_string());
            }
        }
        bevy::log::info!(
            target: "daboyi::startup",
            "已加载 {} 个省份名称",
            province_name_map.len()
        );
    }
    ProvinceNames(province_name_map)
}

fn finalize_loaded_map(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    province_materials: &mut Assets<ProvinceMapMaterial>,
    images: &mut Assets<Image>,
    missing_map_message: &mut MissingMapMessage,
    loaded: LoadedMapAssets,
) {
    let LoadedMapAssets {
        map_data,
        province_names,
        adjacency,
    } = loaded;
    missing_map_message.0 = None;

    bevy::log::info!(
        target: "daboyi::startup",
        "Loaded map: {} provinces",
        map_data.provinces.len()
    );
    MemoryMonitor::log_memory_usage("After loading map data");
    MemoryMonitor::log_detailed_memory_usage("After loading map data");
    MemoryMonitor::log_collection_size("Map provinces", &map_data.provinces);

    // Handle adjacency data
    if let Some(cache) = adjacency {
        let province_count = map_data.provinces.len() as u32;
        if cache.province_count == province_count {
            bevy::log::info!(
                target: "daboyi::startup",
                "Loaded province adjacency cache: {} pairs",
                cache.borders.len()
            );
            commands.insert_resource(crate::map::borders::ProvinceAdjacency(cache.borders));
        } else {
            bevy::log::warn!(
                target: "daboyi::startup",
                "Adjacency cache province count mismatch: expected {}, got {}",
                province_count,
                cache.province_count
            );
        }
    } else {
        bevy::log::warn!(target: "daboyi::startup", "No adjacency cache loaded");
    }

    let n = map_data.provinces.len();
    let tex_height = (n + TEX_WIDTH - 1) / TEX_WIDTH;

    let mut all_positions: Vec<[f32; 3]> = Vec::new();
    let mut all_normals: Vec<[f32; 3]> = Vec::new();
    // UV stores raw texel coordinates [col, row] so the fragment shader can
    // do a direct textureLoad without normalised UV arithmetic.
    let mut all_uvs: Vec<[f32; 2]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    // Province display textures: one RGBA8 pixel per province at offset pid*4.
    let mut political_pixel_data = vec![0u8; tex_height * TEX_WIDTH * 4];
    let mut terrain_pixel_data = vec![0u8; tex_height * TEX_WIDTH * 4];

    for (pid, mp) in map_data.provinces.iter().enumerate() {
        let political_color: [f32; 4] = [0.55, 0.55, 0.55, 1.0];
        let terrain_color = terrain_province_color(&mp.topography);
        let offset = pid * 4;
        political_pixel_data[offset] = (political_color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
        political_pixel_data[offset + 1] =
            (political_color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
        political_pixel_data[offset + 2] =
            (political_color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
        political_pixel_data[offset + 3] =
            (political_color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        terrain_pixel_data[offset] = (terrain_color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
        terrain_pixel_data[offset + 1] = (terrain_color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
        terrain_pixel_data[offset + 2] = (terrain_color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
        terrain_pixel_data[offset + 3] = (terrain_color[3].clamp(0.0, 1.0) * 255.0).round() as u8;

        if mp.vertices.is_empty() || mp.indices.is_empty() {
            continue;
        }

        let col_f = (pid % TEX_WIDTH) as f32;
        let row_f = (pid / TEX_WIDTH) as f32;
        let base_idx = all_positions.len() as u32;

        for v in &mp.vertices {
            all_positions.push([v[0], v[1], 0.0]);
            all_normals.push([0.0, 0.0, 1.0]);
            all_uvs.push([col_f, row_f]);
        }
        for idx in &mp.indices {
            all_indices.push(idx + base_idx);
        }
    }

    bevy::log::info!(
        target: "daboyi::startup",
        "Map mesh: {} vertices, {} triangles",
        all_positions.len(),
        all_indices.len() / 3
    );
    let map_mesh_cpu_bytes = all_positions
        .capacity()
        .saturating_mul(std::mem::size_of::<[f32; 3]>())
        .saturating_add(
            all_normals
                .capacity()
                .saturating_mul(std::mem::size_of::<[f32; 3]>()),
        )
        .saturating_add(
            all_uvs
                .capacity()
                .saturating_mul(std::mem::size_of::<[f32; 2]>()),
        )
        .saturating_add(
            all_indices
                .capacity()
                .saturating_mul(std::mem::size_of::<u32>()),
        );
    MemoryMonitor::log_estimated_allocation(
        "Map mesh asset",
        map_mesh_cpu_bytes,
        map_mesh_cpu_bytes,
        "MAIN_WORLD mesh bytes plus matching GPU vertex/index buffers",
    );
    MemoryMonitor::log_vec_allocation("Map mesh positions", &all_positions);
    MemoryMonitor::log_vec_allocation("Map mesh normals", &all_normals);
    MemoryMonitor::log_vec_allocation("Map mesh uvs", &all_uvs);
    MemoryMonitor::log_vec_allocation("Map mesh indices", &all_indices);
    MemoryMonitor::log_memory_usage("After collecting mesh data");

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, all_positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, all_normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, all_uvs);
    mesh.insert_indices(Indices::U32(all_indices));

    let mesh_handle = meshes.add(mesh);
    MemoryMonitor::log_memory_usage("After creating mesh");

    // Province colour lookup texture: 256 × ceil(N/256), Rgba8Unorm.
    // We keep a separate clone of pixel_data in ProvinceColorBuffer — the Image
    // copy is used only to bootstrap the GpuImage at startup.  All subsequent
    // colour updates go through `write_texture` in update_province_texture_gpu
    // so that the GpuImage (and its TextureView) is never recreated.
    let color_buf_data = political_pixel_data.clone();
    let texture_bytes = TEX_WIDTH * tex_height * 4;
    MemoryMonitor::log_estimated_allocation(
        "Province political image asset",
        texture_bytes,
        texture_bytes,
        "political texture bytes retained in Assets<Image> plus GPU texture",
    );
    MemoryMonitor::log_estimated_allocation(
        "Province terrain image asset",
        texture_bytes,
        texture_bytes,
        "terrain texture bytes retained in Assets<Image> plus GPU texture",
    );
    MemoryMonitor::log_estimated_allocation(
        "ProvinceColorBuffer main copy",
        color_buf_data.capacity(),
        0,
        "CPU-side mutable political color buffer kept in the main world",
    );
    MemoryMonitor::log_vec_allocation("Province political pixels", &political_pixel_data);
    MemoryMonitor::log_vec_allocation("Province terrain pixels", &terrain_pixel_data);
    MemoryMonitor::log_vec_allocation("ProvinceColorBuffer pixels", &color_buf_data);

    // Log texture creation details
    bevy::log::info!(target: "daboyi::startup", "Creating province texture: {}x{} ({} MB)", 
        TEX_WIDTH, tex_height, (TEX_WIDTH * tex_height * 4) as f64 / (1024.0 * 1024.0));

    let mut political_image = Image::new(
        Extent3d {
            width: TEX_WIDTH as u32,
            height: tex_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        political_pixel_data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    political_image.sampler = ImageSampler::nearest();
    let political_tex_handle = images.add(political_image);

    let mut terrain_image = Image::new(
        Extent3d {
            width: TEX_WIDTH as u32,
            height: tex_height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        terrain_pixel_data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    terrain_image.sampler = ImageSampler::nearest();
    let terrain_tex_handle = images.add(terrain_image);

    let material_handle = province_materials.add(ProvinceMapMaterial {
        political_texture: political_tex_handle.clone(),
        terrain_texture: terrain_tex_handle,
        params: ProvinceMapParams::default(),
    });

    for &x_offset in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
        commands.spawn((
            Mesh2d(mesh_handle.clone()),
            MeshMaterial2d(material_handle.clone()),
            Transform::from_xyz(x_offset, 0.0, 0.0),
        ));
    }

    commands.insert_resource(ProvinceColorBuffer {
        data: color_buf_data,
        version: 1,
        image_handle: political_tex_handle,
        tex_width: TEX_WIDTH as u32,
        tex_height: tex_height as u32,
    });
    commands.insert_resource(ProvinceMaterialHandle(material_handle.clone()));

    let mut stripped_map = map_data;
    for province in &mut stripped_map.provinces {
        province.vertices.clear();
        province.indices.clear();
        province.vertices.shrink_to_fit();
        province.indices.shrink_to_fit();
    }
    commands.insert_resource(MapResource(stripped_map));
    commands.insert_resource(province_names);

    MemoryMonitor::log_memory_usage("After loading province names");
}

#[cfg(target_arch = "wasm32")]
fn start_map_load(mut task: ResMut<MapLoadTask>, mut progress: ResMut<LoadingProgress>) {
    if task.0.is_some() {
        return;
    }
    progress.map = LoadingStage::Working {
        label: "正在下载省份地图与名称".to_string(),
        progress: 0.2,
    };

    let slot = Arc::new(Mutex::new(None));
    let slot_for_task = slot.clone();
    spawn_local(async move {
        let result = async {
            let map_bytes = fetch_bytes(MAP_BIN_PATH).await?;
            let map_data = bincode::deserialize::<MapData>(&map_bytes)
                .map_err(|error| format!("解析 {MAP_BIN_PATH} 失败：{error}"))?;
            let province_names =
                load_province_names_from_text(fetch_text("assets/province_names.tsv").await.ok());
            let adjacency = match fetch_bytes(ADJACENCY_BIN_PATH).await {
                Ok(adj_bytes) => match bincode::deserialize::<ProvinceAdjacencyCache>(&adj_bytes) {
                    Ok(cache) => Some(cache),
                    Err(e) => {
                        bevy::log::warn!(target: "daboyi::startup", "解析 {ADJACENCY_BIN_PATH} 失败：{e}");
                        None
                    }
                },
                Err(e) => {
                    bevy::log::warn!(target: "daboyi::startup", "加载 {ADJACENCY_BIN_PATH} 失败：{e}");
                    None
                }
            };
            Ok(LoadedMapAssets {
                map_data,
                province_names,
                adjacency,
            })
        }
        .await;
        *slot_for_task.lock().unwrap() = Some(result);
    });
    task.0 = Some(slot);
}

#[cfg(target_arch = "wasm32")]
fn poll_map_load(
    mut map_load_task: ResMut<MapLoadTask>,
    mut pending_build: ResMut<PendingMapBuild>,
    mut missing_map_message: ResMut<MissingMapMessage>,
    mut progress: ResMut<LoadingProgress>,
    mut build_phase: ResMut<WasmMapBuildPhase>,
) {
    let Some(slot) = map_load_task.0.as_ref() else {
        return;
    };
    let Some(result) = slot.lock().unwrap().take() else {
        return;
    };
    map_load_task.0 = None;
    match result {
        Ok(loaded) => {
            pending_build.0 = Some(loaded);
            progress.map = LoadingStage::Working {
                label: "正在构建省份网格".to_string(),
                progress: 0.72,
            };
            *build_phase = WasmMapBuildPhase::Building;
        }
        Err(error) => {
            bevy::log::error!(target: "daboyi::startup", "{error}");
            progress.map = LoadingStage::Failed(error.clone());
            missing_map_message.0 = Some(format!(
                "浏览器版本无法加载基础地图：{error}。请确认 web 服务器正在提供 assets/map.bin。"
            ));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_loaded_map(
    mut commands: Commands,
    mut pending_build: ResMut<PendingMapBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut province_materials: ResMut<Assets<ProvinceMapMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut missing_map_message: ResMut<MissingMapMessage>,
    mut progress: ResMut<LoadingProgress>,
) {
    let Some(loaded) = pending_build.0.take() else {
        return;
    };
    finalize_loaded_map(
        &mut commands,
        &mut meshes,
        &mut province_materials,
        &mut images,
        &mut missing_map_message,
        loaded,
    );
    progress.map = LoadingStage::Ready;
}

#[cfg(target_arch = "wasm32")]
fn build_loaded_map(
    mut commands: Commands,
    mut pending_build: ResMut<PendingMapBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut province_materials: ResMut<Assets<ProvinceMapMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut missing_map_message: ResMut<MissingMapMessage>,
    mut progress: ResMut<LoadingProgress>,
    build_phase: Res<WasmMapBuildPhase>,
) {
    if !matches!(*build_phase, WasmMapBuildPhase::Building) {
        return;
    }
    let Some(loaded) = pending_build.0.take() else {
        return;
    };
    finalize_loaded_map(
        &mut commands,
        &mut meshes,
        &mut province_materials,
        &mut images,
        &mut missing_map_message,
        loaded,
    );
    progress.map = LoadingStage::Ready;
}

#[cfg(not(target_arch = "wasm32"))]
fn build_loaded_map_native(
    phase: Res<NativeMapLoadPhase>,
    mut commands: Commands,
    pending_build: ResMut<PendingMapBuild>,
    meshes: ResMut<Assets<Mesh>>,
    province_materials: ResMut<Assets<ProvinceMapMaterial>>,
    images: ResMut<Assets<Image>>,
    missing_map_message: ResMut<MissingMapMessage>,
    progress: ResMut<LoadingProgress>,
) {
    if !matches!(*phase, NativeMapLoadPhase::Building) {
        return;
    }
    build_loaded_map(
        commands.reborrow(),
        pending_build,
        meshes,
        province_materials,
        images,
        missing_map_message,
        progress,
    );
    commands.insert_resource(NativeMapLoadPhase::Done);
}

fn update_zoom_visuals(
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    material_handle: Option<Res<ProvinceMaterialHandle>>,
    mut province_materials: ResMut<Assets<ProvinceMapMaterial>>,
    mut last_focus: Local<Option<f32>>,
) {
    let Some(material_handle) = material_handle else {
        return;
    };
    let proj_scale = camera_q
        .get_single()
        .map(|projection| projection.scale)
        .unwrap_or(0.05);
    let terrain_focus = terrain_focus_amount(proj_scale);
    let focus_changed = last_focus
        .map(|last_focus| (last_focus - terrain_focus).abs() > 0.02)
        .unwrap_or(true);
    if !focus_changed {
        return;
    }
    let Some(material) = province_materials.get_mut(&material_handle.0) else {
        return;
    };
    material.params.terrain_focus = terrain_focus;
    *last_focus = Some(terrain_focus);
}

pub(crate) fn terrain_focus_amount(proj_scale: f32) -> f32 {
    zoom_factor(proj_scale, 0.012, 0.002)
}

fn zoom_factor(proj_scale: f32, zoomed_out_scale: f32, zoomed_in_scale: f32) -> f32 {
    ((zoomed_out_scale - proj_scale) / (zoomed_out_scale - zoomed_in_scale)).clamp(0.0, 1.0)
}

/// Ticks the paint debounce timer; bumps `BorderVersion` when it fires.
fn flush_paint_debounce(
    time: Res<Time>,
    mut debounce: ResMut<PaintDebounce>,
    mut border_version: ResMut<BorderVersion>,
) {
    if !debounce.pending_border {
        return;
    }
    debounce.timer.tick(time.delta());
    if debounce.timer.just_finished() {
        debounce.pending_border = false;
        border_version.0 += 1;
    }
}

/// Bundles last-state tracking into a single SystemParam.
#[derive(SystemParam)]
struct ColoringGuard<'w> {
    coloring_version: Res<'w, ColoringVersion>,
    last: ResMut<'w, LastColorState>,
}

/// Write province RGBA8 into the colour texture at pixel offset `pid * 4`.
fn write_color(data: &mut [u8], pid: usize, color: [f32; 4]) {
    let offset = pid * 4;
    if offset + 4 > data.len() {
        return;
    }
    data[offset] = (color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
    data[offset + 1] = (color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
    data[offset + 2] = (color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
    data[offset + 3] = (color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
}

fn country_color_for_tag(tag: &str, lookup: &HashMap<&str, [f32; 4]>) -> [f32; 4] {
    lookup
        .get(tag)
        .copied()
        .unwrap_or_else(|| owner_color_rgba(tag))
}

/// Recolor the province colour texture based on current mode and coloring assignments.
///
/// Colour updates are cheap (~84 KB texture write) so they happen immediately —
/// no deferral needed.  Only `pending_border` / border rebuilds remain debounced.
///
/// Writes to `ProvinceColorBuffer.data` (main world only) and bumps `version`.
/// The render-world system `update_province_texture_gpu` picks up the new version
/// and calls `queue.write_texture` on the EXISTING `GpuImage`, so the material
/// bind group is never invalidated.
fn color_provinces(
    map: Option<Res<MapResource>>,
    color_buf: Option<ResMut<ProvinceColorBuffer>>,
    mode: Res<MapMode>,
    selected: Res<SelectedProvince>,
    active_admin: Res<ActiveAdmin>,
    active_country: Res<ActiveCountry>,
    country_map: Res<CountryMap>,
    countries: Res<Countries>,
    admin_areas: Res<AdminAreas>,
    admin_assignments: Res<AdminMap>,
    non_playable_provinces: Res<NonPlayableProvinces>,
    mut guard: ColoringGuard,
    mut pending_province_recolor: ResMut<PendingProvinceRecolor>,
    mut color_update_queue: ResMut<ColorUpdateQueue>,
) {
    let (Some(map), Some(mut color_buf)) = (map, color_buf) else {
        return;
    };

    // Check if we actually need to update colors
    let mode_changed = guard.last.mode != Some(*mode);
    let selection_changed = guard.last.selected != selected.0;
    let active_admin_changed = guard.last.active_admin != active_admin.0;
    let active_country_changed = guard.last.active_country != active_country.0;
    let coloring_changed = guard.last.coloring_version != guard.coloring_version.0;
    let has_pending_province = !pending_province_recolor.0.is_empty();
    let has_queue_updates = !color_update_queue.queue.is_empty() || color_update_queue.full_rebuild;

    // Early return if no changes
    if !mode_changed
        && !selection_changed
        && !active_admin_changed
        && !active_country_changed
        && !coloring_changed
        && !has_pending_province
        && !has_queue_updates
    {
        return;
    }

    MemoryMonitor::log_memory_usage("Before color_provinces");
    MemoryMonitor::track_memory_growth("Before color_provinces");
    MemoryMonitor::log_hashset_lower_bound("PendingProvinceRecolor", &pending_province_recolor.0);
    MemoryMonitor::log_hashset_lower_bound("ColorUpdateQueue", &color_update_queue.queue);

    // Check if we have queue-based updates
    let needs_full_recolor = mode_changed
        || coloring_changed
        || active_admin_changed
        || active_country_changed
        || color_update_queue.full_rebuild;

    if !needs_full_recolor && !selection_changed && !has_pending_province && !has_queue_updates {
        return;
    }

    // Build a quick lookup: tag → color for editor countries.
    let country_color_lookup: HashMap<&str, [f32; 4]> = countries
        .0
        .iter()
        .map(|c| (c.tag.as_str(), c.color))
        .collect();

    let base_color = |pid: usize| -> [f32; 4] {
        let prov_id = map.0.provinces[pid].id;
        if non_playable_provinces.0.contains(&prov_id) {
            return [0.0, 0.0, 0.0, 0.0];
        }

        if let Some(area_id) = visible_admin_id_for_province(
            active_country.0.as_deref(),
            active_admin.0,
            &admin_areas.0,
            &admin_assignments,
            &country_map,
            prov_id,
        ) {
            return resolve_area_color(area_id, &admin_areas.0, &country_color_lookup);
        }

        if let Some(tag) =
            province_country_tag(&admin_areas.0, &admin_assignments, &country_map, prov_id)
        {
            return country_color_for_tag(tag, &country_color_lookup);
        }

        [0.55, 0.55, 0.55, 1.0]
    };

    if needs_full_recolor {
        // Full texture rewrite (~84 KB): recompute every province's colour.
        for pid in 0..map.0.provinces.len() {
            let is_selected = selected.0 == Some(pid as u32);
            let base = base_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            write_color(&mut color_buf.data, pid, color);
        }
        color_buf.version += 1;
        guard.last.mode = Some(*mode);
        guard.last.selected = selected.0;
        guard.last.active_admin = active_admin.0;
        guard.last.active_country = active_country.0.clone();
        guard.last.coloring_version = guard.coloring_version.0;
        pending_province_recolor.0.clear();
        color_update_queue.queue.clear();
        color_update_queue.full_rebuild = false;
        return;
    }

    // Handle queue-based updates
    if has_queue_updates {
        let mut updated = false;
        // Process queue updates
        let queue = std::mem::take(&mut color_update_queue.queue);
        for pid in queue {
            if pid >= map.0.provinces.len() {
                continue;
            }
            let is_selected = selected.0 == Some(pid as u32);
            let base = base_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            write_color(&mut color_buf.data, pid, color);
            updated = true;
        }
        if updated {
            color_buf.version += 1;
            guard.last.active_country = active_country.0.clone();
        }
        return;
    }

    // Targeted province update (during/after brush drag): only update
    // the provinces that actually changed this frame.
    if has_pending_province {
        let ids: Vec<u32> = pending_province_recolor.0.drain().collect();
        for prov_id in ids {
            let pid = prov_id as usize;
            if pid >= map.0.provinces.len() {
                continue;
            }
            let is_selected = selected.0 == Some(prov_id);
            let base = base_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            write_color(&mut color_buf.data, pid, color);
        }
        color_buf.version += 1;
        // Selection state hasn't changed — keep last.selected current.
        guard.last.active_country = active_country.0.clone();
        return;
    }

    // Selection-only update: restore old selected province, highlight new one.
    if selection_changed {
        if let Some(old_u32) = guard.last.selected {
            let old_pid = old_u32 as usize;
            if old_pid < map.0.provinces.len() {
                write_color(&mut color_buf.data, old_pid, base_color(old_pid));
            }
        }
        if let Some(new_u32) = selected.0 {
            let new_pid = new_u32 as usize;
            if new_pid < map.0.provinces.len() {
                write_color(&mut color_buf.data, new_pid, brighten(base_color(new_pid)));
            }
        }
        color_buf.version += 1;
        guard.last.selected = selected.0;
        guard.last.active_country = active_country.0.clone();
    }

    MemoryMonitor::log_memory_usage("After color_provinces");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_country_hides_foreign_admin_colors() {
        let lookup = HashMap::from([("C001", [0.1, 0.2, 0.3, 1.0])]);
        assert_eq!(country_color_for_tag("C001", &lookup), [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(country_color_for_tag("C009", &lookup)[3], 1.0);
    }
}

/// Render-world system: write province colour data directly onto the existing
/// GPU texture via `RenderQueue::write_texture`.
///
/// This avoids re-creating the `GpuImage` (and breaking the material bind
/// group) that `images.get_mut` would trigger through `prepare_assets::<GpuImage>`.
/// The wgpu `Texture` object stays the same; only its texel data is updated.
fn update_province_texture_gpu(
    color_buf: Option<Res<ProvinceColorBuffer>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
    mut last_version: Local<u64>,
) {
    let Some(color_buf) = color_buf else {
        return;
    };
    if color_buf.version == *last_version {
        return;
    }
    let Some(gpu_image) = gpu_images.get(color_buf.image_handle.id()) else {
        return;
    };
    queue.write_texture(
        gpu_image.texture.as_image_copy(),
        &color_buf.data,
        ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(color_buf.tex_width * 4),
            rows_per_image: None,
        },
        Extent3d {
            width: color_buf.tex_width,
            height: color_buf.tex_height,
            depth_or_array_layers: 1,
        },
    );
    *last_version = color_buf.version;
}
fn resolve_area_color(
    area_id: u32,
    admin_areas: &[shared::AdminArea],
    country_color_lookup: &HashMap<&str, [f32; 4]>,
) -> [f32; 4] {
    let mut current = area_id;
    loop {
        if let Some(area) = admin_areas.iter().find(|a| a.id == current) {
            if let Some(col) = area.color {
                return col;
            }
            match area.parent_id {
                Some(pid) => current = pid,
                None => {
                    return country_color_lookup
                        .get(area.country_tag.as_str())
                        .copied()
                        .unwrap_or([0.55, 0.55, 0.55, 1.0]);
                }
            }
        } else {
            return [0.55, 0.55, 0.55, 1.0];
        }
    }
}
