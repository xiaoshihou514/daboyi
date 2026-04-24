use crate::editor::{
    province_country_tag, AdminAreas, AdminMap, Countries, CountryMap, NonPlayableProvinces,
};
use crate::map::{BorderVersion, ColoringVersion, MapResource};
use crate::memory::MemoryMonitor;
use crate::state::AppState;
use crate::ui::{LoadingProgress, LoadingStage};
#[cfg(target_arch = "wasm32")]
use crate::web_io::fetch_bytes;
use bevy::log::info;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use serde::{Deserialize, Serialize};
use shared::map::{RiverData, TerrainData};
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
use std::io;
#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, Mutex};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

const TERRAIN_BIN_PATH: &str = "assets/terrain.bin";
pub const RIVERS_BIN_PATH: &str = "assets/rivers.bin";
const TERRAIN_ADJACENCY_BIN_PATH: &str = "assets/terrain_adjacency.bin";
const TERRAIN_ADJACENCY_CACHE_VERSION: u32 = 3;
const SURROUND_THRESHOLD: f32 = 0.8;
/// Three copies of the 360°-wide world for seamless horizontal wrapping.
const WORLD_OFFSETS: [f32; 3] = [-360.0, 0.0, 360.0];

/// River width in world-space degrees per width class.
pub const RIVER_WIDTHS: [f32; 3] = [0.03, 0.06, 0.1];
/// River RGBA color.
const RIVER_COLOR: [f32; 4] = [0.18, 0.47, 0.75, 1.0];
const RIVER_LOCAL_Z: f32 = 0.0;
const RIVER_LAYER_Z: f32 = 0.04;
const WATER_OVERLAY_Z: f32 = 0.05;

pub struct TerrainPlugin;

#[derive(Clone)]
struct TerrainPolygonMeta {
    original_color: [f32; 4],
    vertex_count: usize,
    boundary_segments: Vec<[[f32; 2]; 2]>,
}

#[derive(Resource)]
pub struct TerrainMeshData {
    pub mesh_handle: Handle<Mesh>,
    polygons: Vec<TerrainPolygonMeta>,
    total_vertices: usize,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TerrainPolygonAdjacency {
    pub adjacent_provinces: Vec<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TerrainProvinceBorder {
    pub terrain_index: u32,
    pub province_id: u32,
    pub chains: Vec<Vec<[f32; 2]>>,
}

#[derive(Resource, Default)]
pub struct TerrainAdjacencyData {
    pub polygons: Vec<TerrainPolygonAdjacency>,
    pub borders: Vec<TerrainProvinceBorder>,
    pub component_ids: Vec<u32>,
    pub water_polygons: Vec<bool>,
}

#[derive(Clone, Debug)]
pub struct TerrainOwnerResolution {
    pub owner_tag: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct TerrainAdjacencyCache {
    version: u32,
    province_count: u32,
    terrain_polygon_count: u32,
    polygons: Vec<TerrainPolygonAdjacency>,
    borders: Vec<TerrainProvinceBorder>,
    component_ids: Vec<u32>,
    water_polygons: Vec<bool>,
}

#[derive(Default)]
struct LastTerrainVisualState {
    border_version: u64,
    coloring_version: u64,
}

const PAINT_MEMORY_LOG_INTERVAL_SECS: f32 = 1.0;

struct TerrainMemoryDiagnostics {
    timer: Timer,
    recolor_count: u32,
    admin_map_updates: u32,
    country_map_updates: u32,
    border_version_updates: u32,
    coloring_version_updates: u32,
    uploaded_bytes: usize,
}

impl Default for TerrainMemoryDiagnostics {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(PAINT_MEMORY_LOG_INTERVAL_SECS, TimerMode::Repeating),
            recolor_count: 0,
            admin_map_updates: 0,
            country_map_updates: 0,
            border_version_updates: 0,
            coloring_version_updates: 0,
            uploaded_bytes: 0,
        }
    }
}

#[derive(Resource, Default)]
struct TerrainAdjacencyBuildTask {
    #[cfg(not(target_arch = "wasm32"))]
    handle: Option<std::thread::JoinHandle<TerrainAdjacencyData>>,
    province_count: u32,
    terrain_polygon_count: u32,
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
struct TerrainLoadTask(Option<Arc<Mutex<Option<Result<LoadedTerrainAssets, String>>>>>);

#[cfg(target_arch = "wasm32")]
#[derive(Resource, Default)]
struct TerrainAdjacencyLoadTask {
    slot: Option<Arc<Mutex<Option<Result<TerrainAdjacencyData, String>>>>>,
    load_attempted: bool,
}

struct LoadedTerrainAssets {
    terrain: TerrainData,
    rivers: Option<RiverData>,
}

#[derive(Resource, Default)]
struct PendingTerrainBuild(Option<LoadedTerrainAssets>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource, Default, PartialEq, Eq)]
enum NativeTerrainLoadPhase {
    #[default]
    NotStarted,
    Loading,
    Building,
    Done,
}

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        let app = app
            .init_resource::<TerrainAdjacencyData>()
            .init_resource::<TerrainAdjacencyBuildTask>()
            .add_systems(
                Update,
                (compute_terrain_adjacency, update_terrain_visuals)
                    .chain()
                    .run_if(in_state(AppState::Editing)),
            );
        #[cfg(not(target_arch = "wasm32"))]
        let app = app
            .add_systems(
                Update,
                (compute_terrain_adjacency, update_terrain_loading_progress)
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            );
        #[cfg(not(target_arch = "wasm32"))]
        let _ = app
            .init_resource::<PendingTerrainBuild>()
            .init_resource::<NativeTerrainLoadPhase>()
            .add_systems(
                Update,
                (
                    queue_native_terrain_load,
                    load_terrain_native,
                    build_loaded_terrain_native,
                )
                    .run_if(in_state(AppState::Loading)),
            );
        #[cfg(target_arch = "wasm32")]
        let _ = app
            .init_resource::<TerrainLoadTask>()
            .init_resource::<TerrainAdjacencyLoadTask>()
            .init_resource::<PendingTerrainBuild>()
            .add_systems(Startup, start_terrain_load)
            .add_systems(
                Update,
                (poll_terrain_load, build_loaded_terrain).run_if(in_state(AppState::Loading)),
            )
            .add_systems(
                Update,
                (
                    start_terrain_adjacency_load,
                    poll_terrain_adjacency_load,
                    compute_terrain_adjacency,
                    update_terrain_loading_progress,
                )
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn queue_native_terrain_load(
    phase: Res<NativeTerrainLoadPhase>,
    mut progress: ResMut<LoadingProgress>,
    mut commands: Commands,
) {
    if !matches!(*phase, NativeTerrainLoadPhase::NotStarted) {
        return;
    }
    progress.terrain = LoadingStage::Working {
        label: "正在读取地形与河流".to_string(),
        progress: 0.2,
    };
    commands.insert_resource(NativeTerrainLoadPhase::Loading);
}

fn spawn_terrain_from_data(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    terrain: TerrainData,
) {
    // Count total vertices/indices up front for pre-allocation.
    let total_verts: usize = terrain.polygons.iter().map(|p| p.vertices.len()).sum();
    let total_idxs: usize = terrain.polygons.iter().map(|p| p.indices.len()).sum();

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(total_verts);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(total_verts);
    let mut indices: Vec<u32> = Vec::with_capacity(total_idxs);
    let mut water_positions: Vec<[f32; 3]> = Vec::new();
    let mut water_colors: Vec<[f32; 4]> = Vec::new();
    let mut water_indices: Vec<u32> = Vec::new();
    let mut polygon_meta: Vec<TerrainPolygonMeta> = Vec::with_capacity(terrain.polygons.len());
    let mut boundary_segment_count = 0usize;

    for poly in &terrain.polygons {
        let base = positions.len() as u32;
        for &[x, y] in &poly.vertices {
            positions.push([x, y, 0.0]);
            colors.push(poly.color);
        }
        for &i in &poly.indices {
            indices.push(i + base);
        }
        if is_water_terrain_color(poly.color) {
            let water_base = water_positions.len() as u32;
            for &[x, y] in &poly.vertices {
                water_positions.push([x, y, 0.0]);
                water_colors.push(poly.color);
            }
            for &i in &poly.indices {
                water_indices.push(i + water_base);
            }
        }
        polygon_meta.push(TerrainPolygonMeta {
            original_color: poly.color,
            vertex_count: poly.vertices.len(),
            boundary_segments: terrain_polygon_boundary_segments(poly),
        });
        boundary_segment_count += polygon_meta
            .last()
            .map(|meta| meta.boundary_segments.len())
            .unwrap_or(0);
    }

    let terrain_mesh_bytes = positions
        .capacity()
        .saturating_mul(std::mem::size_of::<[f32; 3]>())
        .saturating_add(
            colors
                .capacity()
                .saturating_mul(std::mem::size_of::<[f32; 4]>()),
        )
        .saturating_add(
            indices
                .capacity()
                .saturating_mul(std::mem::size_of::<u32>()),
        );
    MemoryMonitor::log_estimated_allocation(
        "Terrain mesh asset",
        terrain_mesh_bytes,
        terrain_mesh_bytes,
        "terrain mesh bytes retained in the main world plus GPU vertex/index buffers",
    );
    MemoryMonitor::log_vec_allocation("Terrain mesh positions", &positions);
    MemoryMonitor::log_vec_allocation("Terrain mesh colors", &colors);
    MemoryMonitor::log_vec_allocation("Terrain mesh indices", &indices);
    if !water_positions.is_empty() {
        let water_mesh_bytes = water_positions
            .capacity()
            .saturating_mul(std::mem::size_of::<[f32; 3]>())
            .saturating_add(
                water_colors
                    .capacity()
                    .saturating_mul(std::mem::size_of::<[f32; 4]>()),
            )
            .saturating_add(
                water_indices
                    .capacity()
                    .saturating_mul(std::mem::size_of::<u32>()),
            );
        MemoryMonitor::log_estimated_allocation(
            "Terrain water overlay asset",
            water_mesh_bytes,
            water_mesh_bytes,
            "water overlay mesh bytes retained in the main world plus GPU buffers",
        );
        MemoryMonitor::log_vec_allocation("Terrain water positions", &water_positions);
        MemoryMonitor::log_vec_allocation("Terrain water colors", &water_colors);
        MemoryMonitor::log_vec_allocation("Terrain water indices", &water_indices);
    }
    let terrain_meta_bytes = polygon_meta
        .capacity()
        .saturating_mul(std::mem::size_of::<TerrainPolygonMeta>())
        .saturating_add(
            boundary_segment_count.saturating_mul(std::mem::size_of::<[[f32; 2]; 2]>()),
        );
    MemoryMonitor::log_estimated_allocation(
        "Terrain polygon metadata",
        terrain_meta_bytes,
        0,
        "polygon metadata retained on the CPU for ownership tinting and adjacency work",
    );

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    let handle = meshes.add(mesh);
    let material = materials.add(ColorMaterial::default());
    let water_overlay_handle = if water_positions.is_empty() {
        None
    } else {
        let mut water_mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        water_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, water_positions);
        water_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, water_colors);
        water_mesh.insert_indices(Indices::U32(water_indices));
        Some(meshes.add(water_mesh))
    };

    for &x_off in &WORLD_OFFSETS {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, -0.2),
        ));
        if let Some(water_overlay_handle) = water_overlay_handle.as_ref() {
            commands.spawn((
                Mesh2d(water_overlay_handle.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(x_off, 0.0, WATER_OVERLAY_Z),
            ));
        }
    }

    commands.insert_resource(TerrainMeshData {
        mesh_handle: handle,
        polygons: polygon_meta,
        total_vertices: total_verts,
    });

    bevy::log::info!(
        target: "daboyi::startup",
        "Terrain: {} polygons, {} vertices",
        terrain.polygons.len(),
        total_verts,
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn load_terrain_native(
    phase: Res<NativeTerrainLoadPhase>,
    mut pending_build: ResMut<PendingTerrainBuild>,
    mut progress: ResMut<LoadingProgress>,
    mut next_state: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    if !matches!(*phase, NativeTerrainLoadPhase::Loading) {
        return;
    }
    let terrain = match TerrainData::load(TERRAIN_BIN_PATH) {
        Ok(terrain) => terrain,
        Err(error) => {
            bevy::log::error!(
                target: "daboyi::startup",
                "Failed to load {TERRAIN_BIN_PATH}: {error}"
            );
            bevy::log::error!(
                target: "daboyi::startup",
                "Terrain will not be rendered. Run mapgen first."
            );
            progress.terrain =
                LoadingStage::Failed(format!("加载 {TERRAIN_BIN_PATH} 失败：{error}"));
            next_state.set(AppState::Editing);
            return;
        }
    };
    let rivers = match RiverData::load(RIVERS_BIN_PATH) {
        Ok(rivers) => Some(rivers),
        Err(error) => {
            bevy::log::warn!(
                target: "daboyi::startup",
                "无法加载 {RIVERS_BIN_PATH}：{error}"
            );
            None
        }
    };
    pending_build.0 = Some(LoadedTerrainAssets { terrain, rivers });
    progress.terrain = LoadingStage::Working {
        label: "正在构建地形与河流网格".to_string(),
        progress: 0.72,
    };
    commands.insert_resource(NativeTerrainLoadPhase::Building);
}

#[cfg(target_arch = "wasm32")]
fn compute_terrain_adjacency(
    map: Option<Res<MapResource>>,
    terrain_mesh: Option<Res<TerrainMeshData>>,
    mut terrain_adjacency: ResMut<TerrainAdjacencyData>,
    mut build_task: ResMut<TerrainAdjacencyBuildTask>,
    load_task: ResMut<TerrainAdjacencyLoadTask>,
) {
    if !terrain_adjacency.polygons.is_empty() {
        return;
    }
    let (Some(map), Some(terrain_mesh)) = (map, terrain_mesh) else {
        return;
    };
    let province_count = map.0.provinces.len() as u32;
    let terrain_polygon_count = terrain_mesh.polygons.len() as u32;

    if !load_task.load_attempted {
        return;
    }

    if build_task.province_count != 0 || build_task.terrain_polygon_count != 0 {
        return;
    }

    bevy::log::info!(
        target: "daboyi::startup",
        "Terrain adjacency: building cache for {} polygons",
        terrain_mesh.polygons.len()
    );
    let province_boundaries: Vec<(u32, Vec<Vec<[f32; 2]>>)> = map
        .0
        .provinces
        .iter()
        .map(|province| (province.id, province.boundary.clone()))
        .collect();
    let terrain_boundaries: Vec<Vec<[[f32; 2]; 2]>> = terrain_mesh
        .polygons
        .iter()
        .map(|polygon| polygon.boundary_segments.clone())
        .collect();
    let terrain_is_water: Vec<bool> = terrain_mesh
        .polygons
        .iter()
        .map(|polygon| is_water_terrain_color(polygon.original_color))
        .collect();
    build_task.province_count = province_count;
    build_task.terrain_polygon_count = terrain_polygon_count;
    *terrain_adjacency =
        build_terrain_adjacency(&province_boundaries, &terrain_boundaries, &terrain_is_water);
    build_task.province_count = 0;
    build_task.terrain_polygon_count = 0;
    bevy::log::info!(
        target: "daboyi::startup",
        "Terrain adjacency: ready ({} terrain borders)",
        terrain_adjacency.borders.len()
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn compute_terrain_adjacency(
    map: Option<Res<MapResource>>,
    terrain_mesh: Option<Res<TerrainMeshData>>,
    mut terrain_adjacency: ResMut<TerrainAdjacencyData>,
    mut build_task: ResMut<TerrainAdjacencyBuildTask>,
) {
    if !terrain_adjacency.polygons.is_empty() {
        return;
    }
    let (Some(map), Some(terrain_mesh)) = (map, terrain_mesh) else {
        return;
    };
    let province_count = map.0.provinces.len() as u32;
    let terrain_polygon_count = terrain_mesh.polygons.len() as u32;

    if let Some(handle) = build_task.handle.as_ref() {
        if !handle.is_finished() {
            return;
        }
        let handle = build_task.handle.take().unwrap();
        let adjacency = match handle.join() {
            Ok(adjacency) => adjacency,
            Err(_) => {
                bevy::log::error!(
                    target: "daboyi::startup",
                    "Terrain adjacency: background build thread panicked"
                );
                build_task.province_count = 0;
                build_task.terrain_polygon_count = 0;
                return;
            }
        };
        *terrain_adjacency = adjacency;
        if let Err(error) = save_cached_terrain_adjacency(
            build_task.province_count,
            build_task.terrain_polygon_count,
            &terrain_adjacency,
        ) {
            bevy::log::warn!(
                target: "daboyi::startup",
                "Failed to save {TERRAIN_ADJACENCY_BIN_PATH}: {error}"
            );
        }
        build_task.province_count = 0;
        build_task.terrain_polygon_count = 0;
        eprintln!(
            "Terrain adjacency: ready ({} terrain borders)",
            terrain_adjacency.borders.len()
        );
        return;
    }

    match load_cached_terrain_adjacency(province_count, terrain_polygon_count) {
        Ok(Some(cache)) => {
            bevy::log::info!(
                target: "daboyi::startup",
                "Loaded terrain adjacency cache: {} polygons, {} borders",
                cache.polygons.len(),
                cache.borders.len()
            );
            *terrain_adjacency = cache;
            return;
        }
        Ok(None) => {}
        Err(error) => {
            bevy::log::warn!(
                target: "daboyi::startup",
                "Failed to load {TERRAIN_ADJACENCY_BIN_PATH}: {error}"
            );
        }
    }

    if build_task.province_count != 0 || build_task.terrain_polygon_count != 0 {
        return;
    }

    bevy::log::info!(
        target: "daboyi::startup",
        "Terrain adjacency: building cache for {} polygons",
        terrain_mesh.polygons.len()
    );
    let province_boundaries: Vec<(u32, Vec<Vec<[f32; 2]>>)> = map
        .0
        .provinces
        .iter()
        .map(|province| (province.id, province.boundary.clone()))
        .collect();
    let terrain_boundaries: Vec<Vec<[[f32; 2]; 2]>> = terrain_mesh
        .polygons
        .iter()
        .map(|polygon| polygon.boundary_segments.clone())
        .collect();
    let terrain_is_water: Vec<bool> = terrain_mesh
        .polygons
        .iter()
        .map(|polygon| is_water_terrain_color(polygon.original_color))
        .collect();
    build_task.province_count = province_count;
    build_task.terrain_polygon_count = terrain_polygon_count;
    build_task.handle = Some(std::thread::spawn(move || {
        build_terrain_adjacency(&province_boundaries, &terrain_boundaries, &terrain_is_water)
    }));
}

fn update_terrain_loading_progress(
    map: Option<Res<MapResource>>,
    terrain_mesh: Option<Res<TerrainMeshData>>,
    terrain_adjacency: Res<TerrainAdjacencyData>,
    build_task: Res<TerrainAdjacencyBuildTask>,
    mut progress: ResMut<LoadingProgress>,
) {
    if terrain_adjacency.polygons.is_empty() {
        if map.is_some() && terrain_mesh.is_some() {
            let label = if build_task.province_count != 0 || build_task.terrain_polygon_count != 0 {
                "正在构建地形邻接缓存"
            } else {
                "等待地形邻接构建"
            };
            progress.terrain_adjacency = LoadingStage::Working {
                label: label.to_string(),
                progress: 0.6,
            };
        }
        return;
    }
    progress.terrain_adjacency = LoadingStage::Ready;
}

fn build_terrain_adjacency(
    province_boundaries: &[(u32, Vec<Vec<[f32; 2]>>)],
    terrain_boundaries: &[Vec<[[f32; 2]; 2]>],
    terrain_is_water: &[bool],
) -> TerrainAdjacencyData {
    let mut province_edges: HashMap<[(i32, i32); 2], Vec<u32>> = HashMap::new();
    for (province_id, rings) in province_boundaries {
        for ring in rings {
            let ring_len = ring.len();
            for index in 0..ring_len {
                let segment = [ring[index], ring[(index + 1) % ring_len]];
                province_edges
                    .entry(quantized_segment_key(segment))
                    .or_default()
                    .push(*province_id);
            }
        }
    }

    let mut terrain_pair_segments: HashMap<(u32, u32), Vec<[[f32; 2]; 2]>> = HashMap::new();
    let mut terrain_edges: HashMap<[(i32, i32); 2], Vec<u32>> = HashMap::new();
    for (terrain_index, segments) in terrain_boundaries.iter().enumerate() {
        let terrain_index = terrain_index as u32;
        for &segment in segments {
            terrain_edges
                .entry(quantized_segment_key(segment))
                .or_default()
                .push(terrain_index);
            if let Some(province_ids) = province_edges.get(&quantized_segment_key(segment)) {
                for province_id in province_ids {
                    terrain_pair_segments
                        .entry((terrain_index, *province_id))
                        .or_default()
                        .push(segment);
                }
            }
        }
    }

    let mut polygons = vec![TerrainPolygonAdjacency::default(); terrain_boundaries.len()];
    let mut borders = Vec::new();
    for ((terrain_index, province_id), segments) in terrain_pair_segments {
        let chains = merge_unordered_segments(segments);
        if chains.is_empty() {
            continue;
        }
        polygons[terrain_index as usize]
            .adjacent_provinces
            .push(province_id);
        borders.push(TerrainProvinceBorder {
            terrain_index,
            province_id,
            chains,
        });
    }

    for polygon in &mut polygons {
        polygon.adjacent_provinces.sort_unstable();
        polygon.adjacent_provinces.dedup();
    }

    let component_ids =
        terrain_component_ids(terrain_boundaries.len(), terrain_edges, terrain_is_water);

    TerrainAdjacencyData {
        polygons,
        borders,
        component_ids,
        water_polygons: terrain_is_water.to_vec(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_cached_terrain_adjacency(
    province_count: u32,
    terrain_polygon_count: u32,
) -> io::Result<Option<TerrainAdjacencyData>> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = province_count;
        let _ = terrain_polygon_count;
        Ok(None)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let bytes = match fs::read(TERRAIN_ADJACENCY_BIN_PATH) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let cache: TerrainAdjacencyCache = bincode::deserialize(&bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if cache.version != TERRAIN_ADJACENCY_CACHE_VERSION
            || cache.province_count != province_count
            || cache.terrain_polygon_count != terrain_polygon_count
        {
            return Ok(None);
        }
        Ok(Some(TerrainAdjacencyData {
            polygons: cache.polygons,
            borders: cache.borders,
            component_ids: cache.component_ids,
            water_polygons: cache.water_polygons,
        }))
    }
}

fn save_cached_terrain_adjacency(
    province_count: u32,
    terrain_polygon_count: u32,
    terrain_adjacency: &TerrainAdjacencyData,
) -> io::Result<()> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = province_count;
        let _ = terrain_polygon_count;
        let _ = terrain_adjacency;
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let bytes = bincode::serialize(&TerrainAdjacencyCache {
            version: TERRAIN_ADJACENCY_CACHE_VERSION,
            province_count,
            terrain_polygon_count,
            polygons: terrain_adjacency.polygons.clone(),
            borders: terrain_adjacency.borders.clone(),
            component_ids: terrain_adjacency.component_ids.clone(),
            water_polygons: terrain_adjacency.water_polygons.clone(),
        })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(TERRAIN_ADJACENCY_BIN_PATH, bytes)
    }
}

fn update_terrain_visuals(
    terrain_mesh: Option<Res<TerrainMeshData>>,
    terrain_adjacency: Res<TerrainAdjacencyData>,
    mut meshes: ResMut<Assets<Mesh>>,
    countries: Res<Countries>,
    admin_areas: Res<AdminAreas>,
    admin_map: Res<AdminMap>,
    country_map: Res<CountryMap>,
    non_playable_provinces: Res<NonPlayableProvinces>,
    border_version: Res<BorderVersion>,
    coloring_version: Res<ColoringVersion>,
    mut last_state: Local<LastTerrainVisualState>,
    time: Res<Time>,
    mut diagnostics: Local<TerrainMemoryDiagnostics>,
) {
    diagnostics.timer.tick(time.delta());
    let flush_diagnostics = |diagnostics: &mut TerrainMemoryDiagnostics| {
        if diagnostics.recolor_count == 0 || !diagnostics.timer.just_finished() {
            return;
        }
        let uploaded_mib = diagnostics.uploaded_bytes as f64 / (1024.0 * 1024.0);
        info!(
            target: "daboyi::paint::memory",
            "terrain recolor activity: recolors={} uploaded_mib={uploaded_mib:.1} admin_map_updates={} country_map_updates={} border_version_updates={} coloring_version_updates={}",
            diagnostics.recolor_count,
            diagnostics.admin_map_updates,
            diagnostics.country_map_updates,
            diagnostics.border_version_updates,
            diagnostics.coloring_version_updates,
        );
        diagnostics.recolor_count = 0;
        diagnostics.admin_map_updates = 0;
        diagnostics.country_map_updates = 0;
        diagnostics.border_version_updates = 0;
        diagnostics.coloring_version_updates = 0;
        diagnostics.uploaded_bytes = 0;
    };

    let Some(terrain_mesh) = terrain_mesh else {
        flush_diagnostics(&mut diagnostics);
        return;
    };
    if terrain_adjacency.polygons.len() != terrain_mesh.polygons.len() {
        flush_diagnostics(&mut diagnostics);
        return;
    }
    let admin_map_changed = admin_map.is_changed();
    let country_map_changed = country_map.is_changed();
    let border_version_changed = last_state.border_version != border_version.0;
    let coloring_version_changed = last_state.coloring_version != coloring_version.0;
    // Raw admin/country map edits can happen every brush frame. Recoloring the
    // full terrain mesh (~2.1M vertices) on each of those changes is far too
    // expensive, so terrain tinting follows the existing debounced ownership
    // update signal (`BorderVersion`) instead.
    let changed =
        terrain_adjacency.is_changed() || border_version_changed || coloring_version_changed;
    if !changed {
        flush_diagnostics(&mut diagnostics);
        return;
    }

    let Some(mesh) = meshes.get_mut(&terrain_mesh.mesh_handle) else {
        return;
    };

    let country_color_lookup: HashMap<&str, [f32; 4]> = countries
        .0
        .iter()
        .map(|country| (country.tag.as_str(), country.color))
        .collect();

    let mut colors = Vec::with_capacity(terrain_mesh.total_vertices);
    let owner_tint_strength = 0.35;
    for (terrain_index, polygon) in terrain_mesh.polygons.iter().enumerate() {
        let display_color = terrain_display_color(
            terrain_index as u32,
            polygon.original_color,
            owner_tint_strength,
            &terrain_adjacency,
            &country_color_lookup,
            &admin_areas.0,
            &admin_map,
            &country_map,
            &non_playable_provinces,
        );
        for _ in 0..polygon.vertex_count {
            colors.push(display_color);
        }
    }

    diagnostics.recolor_count = diagnostics.recolor_count.saturating_add(1);
    diagnostics.admin_map_updates = diagnostics
        .admin_map_updates
        .saturating_add(u32::from(admin_map_changed));
    diagnostics.country_map_updates = diagnostics
        .country_map_updates
        .saturating_add(u32::from(country_map_changed));
    diagnostics.border_version_updates = diagnostics
        .border_version_updates
        .saturating_add(u32::from(border_version_changed));
    diagnostics.coloring_version_updates = diagnostics
        .coloring_version_updates
        .saturating_add(u32::from(coloring_version_changed));
    diagnostics.uploaded_bytes = diagnostics
        .uploaded_bytes
        .saturating_add(colors.len().saturating_mul(std::mem::size_of::<[f32; 4]>()));

    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    last_state.border_version = border_version.0;
    last_state.coloring_version = coloring_version.0;
    flush_diagnostics(&mut diagnostics);
}

fn terrain_display_color(
    terrain_index: u32,
    original_color: [f32; 4],
    owner_tint_strength: f32,
    terrain_adjacency: &TerrainAdjacencyData,
    country_color_lookup: &HashMap<&str, [f32; 4]>,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    non_playable_provinces: &NonPlayableProvinces,
) -> [f32; 4] {
    if is_water_terrain_color(original_color) {
        return original_color;
    }
    let Some(resolution) = terrain_owner_resolution(
        terrain_index,
        terrain_adjacency,
        admin_areas,
        admin_map,
        country_map,
        non_playable_provinces,
    ) else {
        return original_color;
    };
    let Some(ref tag) = resolution.owner_tag else {
        return original_color;
    };
    let owner_color = country_color_lookup
        .get(tag.as_str())
        .copied()
        .unwrap_or([0.55, 0.55, 0.55, 1.0]);
    mix_colors(original_color, owner_color, owner_tint_strength)
}

pub fn terrain_polygon_surrounding_tag(
    terrain_index: u32,
    terrain_adjacency: &TerrainAdjacencyData,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    non_playable_provinces: &NonPlayableProvinces,
) -> Option<String> {
    terrain_owner_resolution(
        terrain_index,
        terrain_adjacency,
        admin_areas,
        admin_map,
        country_map,
        non_playable_provinces,
    )
    .and_then(|resolution| resolution.owner_tag)
}

pub fn terrain_owner_resolution(
    terrain_index: u32,
    terrain_adjacency: &TerrainAdjacencyData,
    admin_areas: &[shared::AdminArea],
    admin_map: &AdminMap,
    country_map: &CountryMap,
    non_playable_provinces: &NonPlayableProvinces,
) -> Option<TerrainOwnerResolution> {
    let component_id = *terrain_adjacency
        .component_ids
        .get(terrain_index as usize)?;
    let mut total_boundary = 0.0_f32;
    let mut tag_lengths: HashMap<String, f32> = HashMap::new();
    for border in &terrain_adjacency.borders {
        if terrain_adjacency
            .component_ids
            .get(border.terrain_index as usize)
            .copied()
            != Some(component_id)
        {
            continue;
        }
        if non_playable_provinces.0.contains(&border.province_id) {
            continue;
        }
        let border_length = border
            .chains
            .iter()
            .map(|chain| chain_length(chain))
            .sum::<f32>();
        total_boundary += border_length;
        if let Some(tag) =
            province_country_tag(admin_areas, admin_map, country_map, border.province_id)
        {
            *tag_lengths.entry(tag.to_owned()).or_insert(0.0) += border_length;
        }
    }
    if total_boundary <= 1e-6 {
        return Some(TerrainOwnerResolution { owner_tag: None });
    }
    let mut tag_lengths_vec: Vec<(String, f32)> = tag_lengths.into_iter().collect();
    tag_lengths_vec.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    let winner = tag_lengths_vec.first().cloned();
    let (winner_tag, covered_length) = match winner {
        Some((tag, length)) => (Some(tag), length),
        None => (None, 0.0),
    };
    let coverage_ratio = if total_boundary <= 1e-6 {
        0.0
    } else {
        covered_length / total_boundary
    };
    let owner_tag = if coverage_ratio >= SURROUND_THRESHOLD {
        winner_tag.clone()
    } else {
        None
    };
    Some(TerrainOwnerResolution { owner_tag })
}

pub fn terrain_polygon_is_water(
    terrain_index: u32,
    terrain_adjacency: &TerrainAdjacencyData,
) -> bool {
    terrain_adjacency
        .water_polygons
        .get(terrain_index as usize)
        .copied()
        .unwrap_or(false)
}

fn mix_colors(base: [f32; 4], overlay: [f32; 4], overlay_strength: f32) -> [f32; 4] {
    let base_strength = 1.0 - overlay_strength;
    [
        base[0] * base_strength + overlay[0] * overlay_strength,
        base[1] * base_strength + overlay[1] * overlay_strength,
        base[2] * base_strength + overlay[2] * overlay_strength,
        1.0,
    ]
}

fn chain_length(chain: &[[f32; 2]]) -> f32 {
    chain.windows(2).fold(0.0, |acc, segment| {
        let dx = segment[1][0] - segment[0][0];
        let dy = segment[1][1] - segment[0][1];
        acc + (dx * dx + dy * dy).sqrt()
    })
}

fn same_terrain_color(color: [f32; 4], expected: [f32; 4]) -> bool {
    (0..4).all(|index| (color[index] - expected[index]).abs() < 0.0001)
}

fn is_water_terrain_color(color: [f32; 4]) -> bool {
    [
        [0.027, 0.106, 0.314, 1.0],
        [0.039, 0.165, 0.416, 1.0],
        [0.051, 0.227, 0.604, 1.0],
        [0.102, 0.333, 0.722, 1.0],
        [0.071, 0.282, 0.659, 1.0],
        [0.157, 0.439, 0.816, 1.0],
        [0.102, 0.384, 0.753, 1.0],
        [0.847, 0.800, 0.667, 1.0],
    ]
    .into_iter()
    .any(|expected| same_terrain_color(color, expected))
}

fn quantized_segment_key(segment: [[f32; 2]; 2]) -> [(i32, i32); 2] {
    let start = quantized_point(segment[0]);
    let end = quantized_point(segment[1]);
    if start <= end {
        [start, end]
    } else {
        [end, start]
    }
}

fn quantized_point(point: [f32; 2]) -> (i32, i32) {
    (
        (point[0] * 100.0).round() as i32,
        (point[1] * 100.0).round() as i32,
    )
}

fn terrain_component_ids(
    polygon_count: usize,
    terrain_edges: HashMap<[(i32, i32); 2], Vec<u32>>,
    terrain_is_water: &[bool],
) -> Vec<u32> {
    let mut adjacency: Vec<Vec<u32>> = vec![Vec::new(); polygon_count];
    for polygons in terrain_edges.into_values() {
        if polygons.len() < 2 {
            continue;
        }
        for left_index in 0..polygons.len() {
            for right_index in left_index + 1..polygons.len() {
                let left = polygons[left_index];
                let right = polygons[right_index];
                if terrain_is_water[left as usize] != terrain_is_water[right as usize] {
                    continue;
                }
                adjacency[left as usize].push(right);
                adjacency[right as usize].push(left);
            }
        }
    }

    let mut component_ids = vec![u32::MAX; polygon_count];
    let mut next_component = 0_u32;
    for polygon_index in 0..polygon_count {
        if component_ids[polygon_index] != u32::MAX {
            continue;
        }
        let mut stack = vec![polygon_index as u32];
        component_ids[polygon_index] = next_component;
        while let Some(current) = stack.pop() {
            for &neighbor in &adjacency[current as usize] {
                let neighbor_index = neighbor as usize;
                if component_ids[neighbor_index] != u32::MAX {
                    continue;
                }
                component_ids[neighbor_index] = next_component;
                stack.push(neighbor);
            }
        }
        next_component = next_component.saturating_add(1);
    }
    component_ids
}

fn terrain_polygon_boundary_segments(poly: &shared::map::TerrainPolygon) -> Vec<[[f32; 2]; 2]> {
    let mut edge_counts: HashMap<(u32, u32), u32> = HashMap::new();
    let mut edge_points: HashMap<(u32, u32), [[f32; 2]; 2]> = HashMap::new();

    for triangle in poly.indices.chunks_exact(3) {
        for &(start, end) in &[
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            *edge_counts.entry(key).or_insert(0) += 1;
            edge_points
                .entry(key)
                .or_insert([poly.vertices[start as usize], poly.vertices[end as usize]]);
        }
    }

    edge_counts
        .into_iter()
        .filter_map(|(key, count)| {
            if count == 1 {
                edge_points.get(&key).copied()
            } else {
                None
            }
        })
        .collect()
}

fn merge_unordered_segments(segments: Vec<[[f32; 2]; 2]>) -> Vec<Vec<[f32; 2]>> {
    let mut chains: Vec<Vec<[f32; 2]>> = segments
        .into_iter()
        .map(|segment| vec![segment[0], segment[1]])
        .collect();

    'restart: loop {
        let chain_count = chains.len();
        for left_index in 0..chain_count {
            for right_index in 0..chain_count {
                if left_index == right_index {
                    continue;
                }
                let left_first = quantized_point(chains[left_index][0]);
                let left_last = quantized_point(*chains[left_index].last().unwrap());
                let right_first = quantized_point(chains[right_index][0]);
                let right_last = quantized_point(*chains[right_index].last().unwrap());

                if left_last == right_first {
                    let tail = chains[right_index][1..].to_vec();
                    chains[left_index].extend(tail);
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_last == right_last {
                    let mut reversed = chains[right_index].clone();
                    reversed.reverse();
                    chains[left_index].extend_from_slice(&reversed[1..]);
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_first == right_last {
                    let prefix = chains[right_index][..chains[right_index].len() - 1].to_vec();
                    let tail = std::mem::take(&mut chains[left_index]);
                    let mut new_chain = prefix;
                    new_chain.extend(tail);
                    chains[left_index] = new_chain;
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_first == right_first {
                    let mut reversed = chains[right_index].clone();
                    reversed.reverse();
                    let tail = std::mem::take(&mut chains[left_index]);
                    reversed.extend_from_slice(&tail[1..]);
                    chains[left_index] = reversed;
                    chains.remove(right_index);
                    continue 'restart;
                }
            }
        }
        break;
    }

    chains
}

/// Apply one iteration of Chaikin's curve subdivision to smooth a polyline.
/// Preserves the endpoints; interior points are replaced by 1/4 and 3/4 pairs.
fn chaikin_smooth(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let n = pts.len();
    let mut result = Vec::with_capacity(n * 2);
    result.push(pts[0]);
    for i in 0..n - 1 {
        let [ax, ay] = pts[i];
        let [bx, by] = pts[i + 1];
        result.push([0.75 * ax + 0.25 * bx, 0.75 * ay + 0.25 * by]);
        result.push([0.25 * ax + 0.75 * bx, 0.25 * ay + 0.75 * by]);
    }
    result.push(pts[n - 1]);
    result
}

fn compute_strip_sides(points: &[[f32; 2]], half_w: f32) -> Option<(Vec<[f32; 2]>, Vec<[f32; 2]>)> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len();

    // For each point compute the (left, right) world-space positions.
    let mut left: Vec<[f32; 2]> = Vec::with_capacity(n);
    let mut right: Vec<[f32; 2]> = Vec::with_capacity(n);

    // Perpendicular (left-hand) unit vector of a segment direction.
    let perp = |dx: f32, dy: f32| -> (f32, f32) {
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            (0.0, 0.0)
        } else {
            (-dy / len, dx / len)
        }
    };

    for i in 0..n {
        let [x, y] = points[i];

        let offset: (f32, f32) = if i == 0 {
            // Endpoint: perpendicular of the first segment.
            let [x1, y1] = points[1];
            let (px, py) = perp(x1 - x, y1 - y);
            (px * half_w, py * half_w)
        } else if i == n - 1 {
            // Endpoint: perpendicular of the last segment.
            let [xp, yp] = points[n - 2];
            let (px, py) = perp(x - xp, y - yp);
            (px * half_w, py * half_w)
        } else {
            // Interior: miter of the two adjacent perpendiculars.
            let [xp, yp] = points[i - 1];
            let [xn, yn] = points[i + 1];
            let (p0x, p0y) = perp(x - xp, y - yp);
            let (p1x, p1y) = perp(xn - x, yn - y);
            // Miter direction = bisector of the two perpendiculars (normalised).
            let mx = p0x + p1x;
            let my = p0y + p1y;
            let mlen = (mx * mx + my * my).sqrt();
            if mlen < 1e-9 {
                (p0x * half_w, p0y * half_w)
            } else {
                let mux = mx / mlen;
                let muy = my / mlen;
                // Scale so the component along p0 = half_w; cap at 4× to avoid spikes.
                let dot = mux * p0x + muy * p0y;
                let scale = if dot.abs() < 1e-6 {
                    half_w
                } else {
                    (half_w / dot).min(4.0 * half_w)
                };
                (mux * scale, muy * scale)
            }
        };

        left.push([x - offset.0, y - offset.1]);
        right.push([x + offset.0, y + offset.1]);
    }
    Some((left, right))
}

/// Emit a continuous subset of a precomputed quad strip.
fn emit_quad_strip_range(
    left: &[[f32; 2]],
    right: &[[f32; 2]],
    point_start: usize,
    point_end: usize,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    z: f32,
) {
    if point_end <= point_start || point_end >= left.len() || left.len() != right.len() {
        return;
    }

    let base = positions.len() as u32;
    for i in point_start..=point_end {
        positions.push([left[i][0], left[i][1], z]);
        positions.push([right[i][0], right[i][1], z]);
        colors.push(RIVER_COLOR);
        colors.push(RIVER_COLOR);
    }

    let point_count = point_end - point_start + 1;
    for i in 0..(point_count - 1) {
        let local = i as u32;
        let l0 = base + local * 2;
        let r0 = l0 + 1;
        let l1 = l0 + 2;
        let r1 = l0 + 3;
        indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
    }
}

fn endpoint_merge_points(
    left: &[[f32; 2]],
    right: &[[f32; 2]],
    at_start: bool,
) -> Option<[[f32; 2]; 4]> {
    if left.len() < 2 || left.len() != right.len() {
        return None;
    }
    if !at_start {
        let n = left.len();
        return Some([left[n - 1], right[n - 1], right[n - 2], left[n - 2]]);
    }
    Some([left[0], right[0], right[1], left[1]])
}

fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

fn dedup_points(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut deduped: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for &point in points {
        let is_duplicate = deduped.iter().any(|existing| {
            (existing[0] - point[0]).abs() < 1e-5 && (existing[1] - point[1]).abs() < 1e-5
        });
        if !is_duplicate {
            deduped.push(point);
        }
    }
    deduped
}

fn convex_hull(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut sorted = dedup_points(points);
    sorted.sort_by(|lhs, rhs| {
        lhs[0]
            .partial_cmp(&rhs[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                lhs[1]
                    .partial_cmp(&rhs[1])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    if sorted.len() < 3 {
        return sorted;
    }

    let mut lower: Vec<[f32; 2]> = Vec::with_capacity(sorted.len());
    for &point in &sorted {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper: Vec<[f32; 2]> = Vec::with_capacity(sorted.len());
    for &point in sorted.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn add_junction_fill(
    rim_points: &[[f32; 2]],
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    z: f32,
) {
    if rim_points.len() < 3 {
        return;
    }

    let polygon = convex_hull(rim_points);
    if polygon.len() < 3 {
        return;
    }

    let center = {
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        for point in &polygon {
            sum_x += point[0];
            sum_y += point[1];
        }
        let inv_len = 1.0 / polygon.len() as f32;
        [sum_x * inv_len, sum_y * inv_len]
    };

    let base = positions.len() as u32;
    positions.push([center[0], center[1], z]);
    colors.push(RIVER_COLOR);
    for point in &polygon {
        positions.push([point[0], point[1], z]);
        colors.push(RIVER_COLOR);
    }
    let poly_len = polygon.len() as u32;
    for k in 0..poly_len {
        indices.push(base);
        indices.push(base + 1 + k);
        indices.push(base + 1 + ((k + 1) % poly_len));
    }
}

fn spawn_rivers_from_data(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    river_data: RiverData,
) {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut junction_rims: HashMap<u32, Vec<[f32; 2]>> = HashMap::new();
    let mut node_degrees: HashMap<u32, u32> = HashMap::new();

    for edge in &river_data.edges {
        *node_degrees.entry(edge.start_node).or_insert(0) += 1;
        *node_degrees.entry(edge.end_node).or_insert(0) += 1;
    }

    for edge in &river_data.edges {
        let half_w = RIVER_WIDTHS[u32::from(edge.width_class) as usize] / 2.0;
        let raw = edge.points.to_vec();
        let pts = chaikin_smooth(&chaikin_smooth(&raw));
        if pts.len() < 2 {
            continue;
        }
        let Some((left, right)) = compute_strip_sides(&pts, half_w) else {
            continue;
        };

        if left.len() >= 2 {
            emit_quad_strip_range(
                &left,
                &right,
                0,
                left.len() - 1,
                &mut positions,
                &mut colors,
                &mut indices,
                RIVER_LOCAL_Z,
            );
        }

        let start_is_junction = node_degrees.get(&edge.start_node).copied().unwrap_or(0) > 1;
        let end_is_junction = node_degrees.get(&edge.end_node).copied().unwrap_or(0) > 1;

        if start_is_junction {
            if let Some(merge_points) = endpoint_merge_points(&left, &right, true) {
                let entry = junction_rims.entry(edge.start_node).or_default();
                entry.extend_from_slice(&merge_points);
            }
        }
        if end_is_junction {
            if let Some(merge_points) = endpoint_merge_points(&left, &right, false) {
                let entry = junction_rims.entry(edge.end_node).or_default();
                entry.extend_from_slice(&merge_points);
            }
        }
    }

    for (node_id, mut rim_points) in junction_rims {
        rim_points.push(river_data.nodes[node_id as usize].position);
        if rim_points.len() >= 4 {
            add_junction_fill(
                &rim_points,
                &mut positions,
                &mut colors,
                &mut indices,
                RIVER_LOCAL_Z,
            );
        }
    }

    if positions.is_empty() {
        bevy::log::warn!(
            target: "daboyi::startup",
            "rivers.bin contained no renderable river segments"
        );
        return;
    }

    let river_quad_count = indices.len() / 6;
    let river_mesh_bytes = positions
        .capacity()
        .saturating_mul(std::mem::size_of::<[f32; 3]>())
        .saturating_add(
            colors
                .capacity()
                .saturating_mul(std::mem::size_of::<[f32; 4]>()),
        )
        .saturating_add(
            indices
                .capacity()
                .saturating_mul(std::mem::size_of::<u32>()),
        );
    MemoryMonitor::log_estimated_allocation(
        "River mesh asset",
        river_mesh_bytes,
        river_mesh_bytes,
        "river mesh bytes retained in the main world plus GPU buffers",
    );
    MemoryMonitor::log_vec_allocation("River mesh positions", &positions);
    MemoryMonitor::log_vec_allocation("River mesh colors", &colors);
    MemoryMonitor::log_vec_allocation("River mesh indices", &indices);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    let handle = meshes.add(mesh);
    let material = materials.add(ColorMaterial::default());

    for &x_off in &WORLD_OFFSETS {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, RIVER_LAYER_Z),
        ));
    }

    bevy::log::info!(
        target: "daboyi::startup",
        "Rivers: {} edges, {} quads",
        river_data.edges.len(),
        river_quad_count,
    );
}

#[cfg(target_arch = "wasm32")]
fn start_terrain_load(mut task: ResMut<TerrainLoadTask>, mut progress: ResMut<LoadingProgress>) {
    if task.0.is_some() {
        return;
    }
    progress.terrain = LoadingStage::Working {
        label: "正在下载地形与河流".to_string(),
        progress: 0.2,
    };

    let slot = Arc::new(Mutex::new(None));
    let slot_for_task = slot.clone();
    spawn_local(async move {
        let result = async {
            let terrain_bytes = fetch_bytes(TERRAIN_BIN_PATH).await?;
            let terrain = bincode::deserialize::<TerrainData>(&terrain_bytes)
                .map_err(|error| format!("解析 {TERRAIN_BIN_PATH} 失败：{error}"))?;
            let rivers = match fetch_bytes(RIVERS_BIN_PATH).await {
                Ok(bytes) => Some(
                    bincode::deserialize::<RiverData>(&bytes)
                        .map_err(|error| format!("解析 {RIVERS_BIN_PATH} 失败：{error}"))?,
                ),
                Err(error) => {
                    bevy::log::warn!(
                        target: "daboyi::startup",
                        "无法加载 {RIVERS_BIN_PATH}：{error}"
                    );
                    None
                }
            };
            Ok(LoadedTerrainAssets { terrain, rivers })
        }
        .await;
        *slot_for_task.lock().unwrap() = Some(result);
    });
    task.0 = Some(slot);
}

#[cfg(target_arch = "wasm32")]
fn poll_terrain_load(
    mut terrain_load_task: ResMut<TerrainLoadTask>,
    mut pending_build: ResMut<PendingTerrainBuild>,
    mut progress: ResMut<LoadingProgress>,
) {
    let Some(slot) = terrain_load_task.0.as_ref() else {
        return;
    };
    let Some(result) = slot.lock().unwrap().take() else {
        return;
    };
    terrain_load_task.0 = None;
    match result {
        Ok(loaded) => {
            progress.terrain = LoadingStage::Working {
                label: "正在构建地形与河流网格".to_string(),
                progress: 0.72,
            };
            pending_build.0 = Some(loaded);
        }
        Err(error) => {
            bevy::log::warn!(target: "daboyi::startup", "无法加载 terrain web 资源：{error}");
            progress.terrain = LoadingStage::Failed(error);
        }
    }
}

fn build_loaded_terrain(
    mut commands: Commands,
    mut pending_build: ResMut<PendingTerrainBuild>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut progress: ResMut<LoadingProgress>,
) {
    let Some(loaded) = pending_build.0.take() else {
        return;
    };
    spawn_terrain_from_data(&mut commands, &mut meshes, &mut materials, loaded.terrain);
    if let Some(rivers) = loaded.rivers {
        spawn_rivers_from_data(&mut commands, &mut meshes, &mut materials, rivers);
    }
    progress.terrain = LoadingStage::Ready;
}

#[cfg(target_arch = "wasm32")]
fn start_terrain_adjacency_load(
    mut task: ResMut<TerrainAdjacencyLoadTask>,
    terrain_mesh: Option<Res<TerrainMeshData>>,
    map: Option<Res<MapResource>>,
    mut progress: ResMut<LoadingProgress>,
) {
    if task.slot.is_some() {
        return;
    }
    let Some(terrain_mesh) = terrain_mesh else {
        return;
    };
    let Some(map) = map else {
        return;
    };
    let province_count = map.0.provinces.len() as u32;
    let terrain_polygon_count = terrain_mesh.polygons.len() as u32;
    progress.terrain_adjacency = LoadingStage::Working {
        label: "正在下载地形邻接缓存".to_string(),
        progress: 0.5,
    };

    let slot = Arc::new(Mutex::new(None));
    let slot_for_task = slot.clone();
    spawn_local(async move {
        let result = async {
            let bytes = fetch_bytes(TERRAIN_ADJACENCY_BIN_PATH).await?;
            let cache: TerrainAdjacencyCache = bincode::deserialize(&bytes)
                .map_err(|error| format!("解析 {TERRAIN_ADJACENCY_BIN_PATH} 失败：{error}"))?;
            if cache.version != TERRAIN_ADJACENCY_CACHE_VERSION
                || cache.province_count != province_count
                || cache.terrain_polygon_count != terrain_polygon_count
            {
                return Err(format!(
                    "地形邻接缓存版本不匹配：expected version={}, prov={}, terr={}, got version={}, prov={}, terr={}",
                    TERRAIN_ADJACENCY_CACHE_VERSION,
                    province_count,
                    terrain_polygon_count,
                    cache.version,
                    cache.province_count,
                    cache.terrain_polygon_count
                ));
            }
            Ok(TerrainAdjacencyData {
                polygons: cache.polygons,
                borders: cache.borders,
                component_ids: cache.component_ids,
                water_polygons: cache.water_polygons,
            })
        }
        .await;
        *slot_for_task.lock().unwrap() = Some(result);
    });
    task.slot = Some(slot);
    task.load_attempted = true;
}

#[cfg(target_arch = "wasm32")]
fn poll_terrain_adjacency_load(
    mut task: ResMut<TerrainAdjacencyLoadTask>,
    mut terrain_adjacency: ResMut<TerrainAdjacencyData>,
    mut progress: ResMut<LoadingProgress>,
) {
    let Some(slot) = task.slot.as_ref() else {
        return;
    };
    let Some(result) = slot.lock().unwrap().take() else {
        return;
    };
    task.slot = None;
    match result {
        Ok(adjacency) => {
            bevy::log::info!(
                target: "daboyi::startup",
                "Loaded terrain adjacency cache: {} polygons, {} borders",
                adjacency.polygons.len(),
                adjacency.borders.len()
            );
            *terrain_adjacency = adjacency;
            progress.terrain_adjacency = LoadingStage::Ready;
        }
        Err(error) => {
            bevy::log::warn!(
                target: "daboyi::startup",
                "无法加载地形邻接缓存：{error}，将实时计算"
            );
            progress.terrain_adjacency = LoadingStage::Working {
                label: "正在构建地形邻接缓存".to_string(),
                progress: 0.5,
            };
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn build_loaded_terrain_native(
    phase: Res<NativeTerrainLoadPhase>,
    mut commands: Commands,
    pending_build: ResMut<PendingTerrainBuild>,
    meshes: ResMut<Assets<Mesh>>,
    materials: ResMut<Assets<ColorMaterial>>,
    progress: ResMut<LoadingProgress>,
) {
    if !matches!(*phase, NativeTerrainLoadPhase::Building) {
        return;
    }
    build_loaded_terrain(
        commands.reborrow(),
        pending_build,
        meshes,
        materials,
        progress,
    );
    commands.insert_resource(NativeTerrainLoadPhase::Done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surrounding_tag_requires_eighty_percent_boundary_coverage() {
        let admin_areas = vec![
            shared::AdminArea {
                id: 1,
                name: "A1".to_owned(),
                country_tag: "A".to_owned(),
                parent_id: None,
                color: None,
            },
            shared::AdminArea {
                id: 2,
                name: "B1".to_owned(),
                country_tag: "B".to_owned(),
                parent_id: None,
                color: None,
            },
        ];
        let admin_map = AdminMap(HashMap::from([(10, 1), (11, 1), (12, 2)]));
        let country_map = CountryMap::default();
        let mostly_a = TerrainAdjacencyData {
            polygons: vec![TerrainPolygonAdjacency {
                adjacent_provinces: vec![10, 11, 12],
            }],
            component_ids: vec![0],
            water_polygons: vec![false],
            borders: vec![
                TerrainProvinceBorder {
                    terrain_index: 0,
                    province_id: 10,
                    chains: vec![vec![[0.0, 0.0], [4.0, 0.0]]],
                },
                TerrainProvinceBorder {
                    terrain_index: 0,
                    province_id: 11,
                    chains: vec![vec![[4.0, 0.0], [8.0, 0.0]]],
                },
                TerrainProvinceBorder {
                    terrain_index: 0,
                    province_id: 12,
                    chains: vec![vec![[8.0, 0.0], [10.0, 0.0]]],
                },
            ],
        };

        assert_eq!(
            terrain_polygon_surrounding_tag(
                0,
                &mostly_a,
                &admin_areas,
                &admin_map,
                &country_map,
                &NonPlayableProvinces::default(),
            ),
            Some(String::from("A"))
        );
        assert_eq!(
            terrain_polygon_surrounding_tag(
                0,
                &TerrainAdjacencyData {
                    polygons: mostly_a.polygons.clone(),
                    component_ids: mostly_a.component_ids.clone(),
                    water_polygons: mostly_a.water_polygons.clone(),
                    borders: vec![
                        TerrainProvinceBorder {
                            terrain_index: 0,
                            province_id: 10,
                            chains: vec![vec![[0.0, 0.0], [3.0, 0.0]]],
                        },
                        TerrainProvinceBorder {
                            terrain_index: 0,
                            province_id: 11,
                            chains: vec![vec![[3.0, 0.0], [6.0, 0.0]]],
                        },
                        TerrainProvinceBorder {
                            terrain_index: 0,
                            province_id: 12,
                            chains: vec![vec![[6.0, 0.0], [10.0, 0.0]]],
                        },
                    ],
                },
                &admin_areas,
                &admin_map,
                &country_map,
                &NonPlayableProvinces::default(),
            ),
            None
        );
    }

    #[test]
    fn water_tiles_keep_original_color() {
        let admin_areas = vec![shared::AdminArea {
            id: 1,
            name: "A1".to_owned(),
            country_tag: "A".to_owned(),
            parent_id: None,
            color: None,
        }];
        let admin_map = AdminMap(HashMap::from([(10, 1)]));
        let country_map = CountryMap::default();
        let terrain_adjacency = TerrainAdjacencyData {
            polygons: vec![TerrainPolygonAdjacency {
                adjacent_provinces: vec![10],
            }],
            component_ids: vec![0],
            water_polygons: vec![true],
            borders: vec![TerrainProvinceBorder {
                terrain_index: 0,
                province_id: 10,
                chains: vec![vec![[0.0, 0.0], [10.0, 0.0]]],
            }],
        };
        let original = [0.039, 0.165, 0.416, 1.0];

        assert_eq!(
            terrain_display_color(
                0,
                original,
                0.35,
                &terrain_adjacency,
                &HashMap::from([("A", [1.0, 0.0, 0.0, 1.0])]),
                &admin_areas,
                &admin_map,
                &country_map,
                &NonPlayableProvinces::default(),
            ),
            original
        );
    }
}
