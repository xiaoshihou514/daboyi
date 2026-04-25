use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::mesh::{
    Indices, MeshVertexAttribute, MeshVertexBufferLayoutRef, PrimitiveTopology,
};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderRef, ShaderType, SpecializedMeshPipelineError,
    VertexFormat,
};
use shared::map::{CachedBorder, ProvinceAdjacencyCache, ADJACENCY_CACHE_VERSION};
use std::collections::HashMap;

use crate::editor::{
    province_country_tag, visible_admin_id_for_province, ActiveAdmin, ActiveCountry, AdminAreas,
    AdminMap, CountryMap, NonPlayableProvinces,
};
use crate::map::{BorderVersion, MapMode, MapResource, MAP_WIDTH};
use crate::memory::MemoryMonitor;
use crate::state::AppState;
use crate::terrain::{
    terrain_polygon_is_water, terrain_polygon_surrounding_tag, TerrainAdjacencyData,
};
use crate::ui::{LoadingProgress, LoadingStage};
use bevy::log::info;
use bevy::sprite::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin};

use std::collections::HashSet;

const ADJACENCY_BIN_PATH: &str = "assets/province_adjacency.bin";
const BORDER_CHUNK_WIDTH: f32 = 30.0;
const BORDER_CHUNK_HEIGHT: f32 = 15.0;
const BORDER_CHUNK_COLS: u32 = 12;

const ATTRIBUTE_BORDER_OFFSET: MeshVertexAttribute =
    MeshVertexAttribute::new("BorderOffset", 983_541_201, VertexFormat::Float32x2);
const ATTRIBUTE_BORDER_TIER: MeshVertexAttribute =
    MeshVertexAttribute::new("BorderTier", 983_541_202, VertexFormat::Float32);

/// Precomputed border chains keyed by adjacent province pairs.
#[derive(Resource, Default)]
pub struct ProvinceAdjacency(pub Vec<CachedBorder>);

/// Marker for the border mesh entities.
#[derive(Component)]
pub struct BorderMesh;

#[allow(dead_code)] // Used in shader code
#[derive(Clone, Copy, ShaderType)]
struct BorderMaterialParams {
    proj_scale: f32,
    _padding: Vec3,
}

impl Default for BorderMaterialParams {
    fn default() -> Self {
        Self {
            proj_scale: 0.05,
            _padding: Vec3::ZERO,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
struct BorderMaterial {
    #[uniform(0)]
    params: BorderMaterialParams,
}

impl Material2d for BorderMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/border_material.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/border_material.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let vertex_layout = layout.0.get_layout(&[
            Mesh::ATTRIBUTE_POSITION.at_shader_location(0),
            ATTRIBUTE_BORDER_OFFSET.at_shader_location(1),
            ATTRIBUTE_BORDER_TIER.at_shader_location(2),
        ])?;
        descriptor.vertex.buffers = vec![vertex_layout];
        Ok(())
    }
}

#[derive(Resource, Default)]
struct BorderAssets {
    meshes: HashMap<u16, Handle<Mesh>>,
    material: Option<Handle<BorderMaterial>>,
}

#[derive(Resource, Default)]
struct BorderScratch {
    positions: Vec<[f32; 3]>,
    offsets: Vec<[f32; 2]>,
    tiers: Vec<f32>,
    indices: Vec<u32>,
}

/// 边界变化跟踪资源
#[derive(Resource, Default)]
pub struct BorderChanges {
    /// 发生变化的省份ID
    pub changed_provinces: HashSet<u32>,
}

#[derive(Default)]
struct BorderChunkIndex {
    province_chunks: Vec<Vec<u16>>,
    adjacency_by_chunk: HashMap<u16, Vec<usize>>,
    terrain_by_chunk: HashMap<u16, Vec<usize>>,
    all_chunks: Vec<u16>,
}

/// 边界数据资源，用于存储分块索引与运行时状态
#[derive(Resource, Default)]
pub struct BorderData {
    /// 边界计算是否正在进行中
    pub is_computing: bool,
    chunk_index: Option<BorderChunkIndex>,
}

#[derive(Component, Clone, Copy)]
struct BorderChunk(u16);

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerKey {
    Country(u32),
    Admin(u32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BorderTier {
    Country,
    Admin,
    Province,
}

pub struct BordersPlugin;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BorderAdjacencyPass;

impl Plugin for BordersPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<BorderMaterial>::default())
            .insert_resource(ProvinceAdjacency::default())
            .insert_resource(BorderAssets::default())
            .insert_resource(BorderScratch::default())
            .insert_resource(BorderData::default())
            .insert_resource(BorderChanges::default())
            .add_systems(
                Update,
                (
                    compute_adjacency.in_set(BorderAdjacencyPass),
                    rebuild_borders,
                    update_border_material_params,
                )
                    .chain()
                    .run_if(in_state(AppState::Editing)),
            )
            .add_systems(
                Update,
                (
                    compute_adjacency.in_set(BorderAdjacencyPass),
                    rebuild_borders,
                    update_border_loading_progress,
                )
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

#[derive(SystemParam)]
struct BorderInputs<'w, 's> {
    border_version: Res<'w, BorderVersion>,
    active_admin: Res<'w, ActiveAdmin>,
    active_country: Res<'w, ActiveCountry>,
    admin_areas: Res<'w, AdminAreas>,
    country_map: Res<'w, CountryMap>,
    admin_assignments: Res<'w, AdminMap>,
    non_playable_provinces: Res<'w, NonPlayableProvinces>,
    adjacency: Res<'w, ProvinceAdjacency>,
    terrain_adjacency: Res<'w, TerrainAdjacencyData>,
    mode: Res<'w, MapMode>,
    border_assets: ResMut<'w, BorderAssets>,
    scratch: ResMut<'w, BorderScratch>,
    border_changes: ResMut<'w, BorderChanges>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
struct BorderState<'w, 's> {
    last_mode: Local<'s, Option<MapMode>>,
    last_border_version: Local<'s, u64>,
    last_active_admin: Local<'s, Option<u32>>,
    last_active_country: Local<'s, Option<String>>,
    _marker: std::marker::PhantomData<&'w ()>,
}

/// Compute province adjacency from pre-computed cache file.
pub fn compute_adjacency(map: Option<Res<MapResource>>, mut adjacency: ResMut<ProvinceAdjacency>) {
    if !adjacency.0.is_empty() {
        return;
    }
    let Some(map) = map else { return };
    let province_count = map.0.provinces.len() as u32;

    match ProvinceAdjacencyCache::load(ADJACENCY_BIN_PATH) {
        Ok(cache) if cache.version == ADJACENCY_CACHE_VERSION && cache.province_count == province_count => {
            bevy::log::info!(
                target: "daboyi::startup",
                "Loaded province adjacency cache: {} pairs",
                cache.borders.len()
            );
            adjacency.0 = cache.borders;
            MemoryMonitor::log_estimated_allocation(
                "Province adjacency cache",
                cached_borders_bytes(&adjacency.0),
                0,
                "cached shared-border chains retained on the CPU",
            );
        }
        _ => {
            bevy::log::error!(
                target: "daboyi::startup",
                "Failed to load {ADJACENCY_BIN_PATH}: run mapgen to generate it"
            );
        }
    }
}

/// Rebuild the border mesh whenever political ownership semantics change.
fn rebuild_borders(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BorderMaterial>>,
    mut border_inputs: BorderInputs,
    map: Option<Res<MapResource>>,
    existing: Query<(Entity, &BorderChunk), With<BorderMesh>>,
    mut border_state: BorderState,
    mut border_data: ResMut<BorderData>,
) {
    let Some(map) = map else { return };

    if border_inputs.adjacency.0.is_empty() {
        return;
    }

    // Check if we actually need to rebuild borders
    let mode_changed = Some(*border_inputs.mode) != *border_state.last_mode;
    let border_changed = *border_state.last_border_version != border_inputs.border_version.0;
    let active_admin_changed = *border_state.last_active_admin != border_inputs.active_admin.0;
    let active_country_changed =
        *border_state.last_active_country != border_inputs.active_country.0;

    // Early return if no changes
    if !mode_changed && !border_changed && !active_admin_changed && !active_country_changed {
        return;
    }

    MemoryMonitor::log_memory_usage("Before border rebuild");
    MemoryMonitor::log_detailed_memory_usage("Before border rebuild");
    *border_state.last_mode = Some(*border_inputs.mode);
    *border_state.last_border_version = border_inputs.border_version.0;
    *border_state.last_active_admin = border_inputs.active_admin.0;
    *border_state.last_active_country = border_inputs.active_country.0.clone();

    let existing_chunk_entities = group_chunk_entities(&existing);

    // Early-return for non-political mode BEFORE touching the mesh
    if *border_inputs.mode != MapMode::Map {
        despawn_border_entities(&mut commands, &existing);
        border_inputs.border_changes.changed_provinces.clear();
        return;
    }

    // If already computing, skip to avoid duplicate tasks
    if border_data.is_computing {
        return;
    }

    border_data.is_computing = true;
    let chunk_index = border_data.chunk_index.get_or_insert_with(|| {
        build_border_chunk_index(
            &map.0,
            &border_inputs.adjacency.0,
            &border_inputs.terrain_adjacency,
        )
    });

    let full_rebuild = mode_changed
        || active_admin_changed
        || active_country_changed
        || existing_chunk_entities.is_empty()
        || border_inputs.border_changes.changed_provinces.is_empty();
    let dirty_chunks = if full_rebuild {
        chunk_index.all_chunks.clone()
    } else {
        dirty_chunks_from_provinces(chunk_index, &border_inputs.border_changes.changed_provinces)
    };

    if dirty_chunks.is_empty() {
        border_data.is_computing = false;
        border_inputs.border_changes.changed_provinces.clear();
        return;
    }

    info!(
        target: "daboyi::paint::memory",
        "border rebuild scope: full_rebuild={} dirty_chunks={} changed_provinces={}",
        full_rebuild,
        dirty_chunks.len(),
        border_inputs.border_changes.changed_provinces.len(),
    );

    let mut country_keys: HashMap<String, u32> = HashMap::new();
    let mut next_country_key = 0_u32;
    let mut intern_country_key = |tag: &str| {
        country_keys.entry(tag.to_owned()).or_insert_with(|| {
            let key = next_country_key;
            next_country_key = next_country_key.saturating_add(1);
            key
        });
    };
    for tag in border_inputs.country_map.0.values() {
        intern_country_key(tag);
    }
    for area in &border_inputs.admin_areas.0 {
        intern_country_key(area.country_tag.as_str());
    }

    let is_wasteland = |idx: usize| -> bool {
        idx < map.0.provinces.len() && map.0.provinces[idx].topography.contains("wasteland")
    };
    let province_owner = |idx: usize| -> Option<OwnerKey> {
        if idx >= map.0.provinces.len() {
            return None;
        }
        let prov_id = map.0.provinces[idx].id;
        if let Some(area_id) = visible_admin_id_for_province(
            border_inputs.active_country.0.as_deref(),
            border_inputs.active_admin.0,
            &border_inputs.admin_areas.0,
            &border_inputs.admin_assignments,
            &border_inputs.country_map,
            prov_id,
        ) {
            return Some(OwnerKey::Admin(area_id));
        }
        province_country_tag(
            &border_inputs.admin_areas.0,
            &border_inputs.admin_assignments,
            &border_inputs.country_map,
            prov_id,
        )
        .and_then(|tag| country_keys.get(tag).copied())
        .map(OwnerKey::Country)
    };

    let admin_areas = &border_inputs.admin_areas.0;
    let admin_assignments = &border_inputs.admin_assignments;
    let country_map = &border_inputs.country_map;
    let terrain_adjacency = &border_inputs.terrain_adjacency;
    let material_handle = border_inputs
        .border_assets
        .material
        .get_or_insert_with(|| materials.add(BorderMaterial::default()))
        .clone();

    let mut total_uploaded_bytes = 0usize;
    for chunk_id in dirty_chunks {
        let chunk_entities = existing_chunk_entities.get(&chunk_id);
        let BorderScratch {
            positions,
            offsets,
            tiers,
            indices,
        } = &mut *border_inputs.scratch;
        positions.clear();
        offsets.clear();
        tiers.clear();
        indices.clear();

        build_chunk_geometry(
            chunk_id,
            chunk_index,
            &border_inputs.adjacency.0,
            terrain_adjacency,
            &map.0,
            admin_areas,
            admin_assignments,
            country_map,
            &border_inputs.non_playable_provinces,
            is_wasteland,
            &province_owner,
            positions,
            offsets,
            tiers,
            indices,
        );

        let chunk_bytes = positions
            .len()
            .saturating_mul(std::mem::size_of::<[f32; 3]>())
            .saturating_add(
                offsets
                    .len()
                    .saturating_mul(std::mem::size_of::<[f32; 2]>()),
            )
            .saturating_add(tiers.len().saturating_mul(std::mem::size_of::<f32>()))
            .saturating_add(indices.len().saturating_mul(std::mem::size_of::<u32>()));
        total_uploaded_bytes = total_uploaded_bytes.saturating_add(chunk_bytes);

        info!(
            target: "daboyi::paint::memory",
            "border chunk rebuild: chunk={} vertices={} indices={} approx_mib={:.1}",
            chunk_id,
            positions.len(),
            indices.len(),
            MemoryMonitor::bytes_to_mib(chunk_bytes),
        );

        upsert_chunk_mesh(
            chunk_id,
            positions,
            offsets,
            tiers,
            indices,
            &mut commands,
            &mut meshes,
            &mut border_inputs.border_assets,
            &material_handle,
            chunk_entities,
        );
    }

    MemoryMonitor::log_estimated_allocation(
        "Border scratch buffers",
        border_scratch_capacity_bytes(
            &border_inputs.scratch.positions,
            &border_inputs.scratch.offsets,
            &border_inputs.scratch.tiers,
            &border_inputs.scratch.indices,
        ),
        0,
        "persistent Vec capacities reused between chunk rebuilds",
    );
    border_inputs.border_changes.changed_provinces.clear();
    border_data.is_computing = false;
    info!(
        target: "daboyi::paint::memory",
        "border rebuild complete: dirty_chunks={} uploaded_mib={:.1}",
        border_inputs.border_assets.meshes.len(),
        MemoryMonitor::bytes_to_mib(total_uploaded_bytes),
    );
    MemoryMonitor::log_memory_usage("After border rebuild");
}

fn update_border_loading_progress(
    map: Option<Res<MapResource>>,
    terrain_adjacency: Res<TerrainAdjacencyData>,
    adjacency: Res<ProvinceAdjacency>,
    border_data: Res<BorderData>,
    existing: Query<Entity, With<BorderMesh>>,
    mut progress: ResMut<LoadingProgress>,
) {
    if map.is_none() || terrain_adjacency.polygons.is_empty() {
        return;
    }
    if adjacency.0.is_empty() || border_data.is_computing || existing.is_empty() {
        progress.borders = LoadingStage::Working {
            label: "正在构建边界网格".to_string(),
            progress: 0.6,
        };
        return;
    }
    progress.borders = LoadingStage::Ready;
}

fn province_border_tier(
    province_a: u32,
    province_b: u32,
    admin_areas: &[shared::AdminArea],
    admin_assignments: &AdminMap,
    country_map: &CountryMap,
    owner_a: Option<OwnerKey>,
    owner_b: Option<OwnerKey>,
) -> Option<BorderTier> {
    let country_a = province_country_tag(admin_areas, admin_assignments, country_map, province_a);
    let country_b = province_country_tag(admin_areas, admin_assignments, country_map, province_b);
    if country_a != country_b {
        return Some(BorderTier::Country);
    }
    if owner_a != owner_b {
        return Some(BorderTier::Admin);
    }
    Some(BorderTier::Province)
}

fn collect_country_junction_rims(
    junction_rims: &mut HashMap<(i32, i32), ([f32; 2], Vec<[f32; 2]>)>,
    chain: &[[f32; 2]],
) {
    if let Some(span) = endpoint_cap_span(chain, 1.0, true) {
        let entry = junction_rims
            .entry(border_qpt(chain[0]))
            .or_insert((chain[0], Vec::new()));
        entry.1.extend_from_slice(&span);
    }
    let last = *chain.last().unwrap();
    if let Some(span) = endpoint_cap_span(chain, 1.0, false) {
        let entry = junction_rims
            .entry(border_qpt(last))
            .or_insert((last, Vec::new()));
        entry.1.extend_from_slice(&span);
    }
}

fn terrain_border_should_render(terrain_owner: Option<&str>, province_tag: &str) -> bool {
    terrain_owner != Some(province_tag)
}

fn border_tier_id(border_tier: BorderTier) -> f32 {
    match border_tier {
        BorderTier::Country => 0.0,
        BorderTier::Admin => 1.0,
        BorderTier::Province => 2.0,
    }
}

fn border_tier_z(border_tier: BorderTier) -> f32 {
    match border_tier {
        BorderTier::Country => 0.84,
        BorderTier::Admin => 0.83,
        BorderTier::Province => 0.82,
    }
}

fn despawn_border_entities(
    commands: &mut Commands,
    existing: &Query<(Entity, &BorderChunk), With<BorderMesh>>,
) {
    for (entity, _) in existing.iter() {
        commands.entity(entity).despawn();
    }
}

fn group_chunk_entities(
    existing: &Query<(Entity, &BorderChunk), With<BorderMesh>>,
) -> HashMap<u16, Vec<Entity>> {
    let mut grouped = HashMap::new();
    for (entity, chunk) in existing.iter() {
        grouped.entry(chunk.0).or_insert_with(Vec::new).push(entity);
    }
    grouped
}

fn build_border_chunk_index(
    map: &shared::map::MapData,
    adjacency: &[CachedBorder],
    terrain_adjacency: &TerrainAdjacencyData,
) -> BorderChunkIndex {
    let mut province_chunks = vec![Vec::new(); map.provinces.len()];
    for province in &map.provinces {
        let entry = &mut province_chunks[province.id as usize];
        for point in province
            .boundary
            .iter()
            .flat_map(|ring| ring.iter().copied())
            .chain(std::iter::once(province.centroid))
        {
            let chunk = chunk_id_for_point(point);
            if !entry.contains(&chunk) {
                entry.push(chunk);
            }
        }
    }

    let mut adjacency_by_chunk: HashMap<u16, Vec<usize>> = HashMap::new();
    for (index, border) in adjacency.iter().enumerate() {
        for chunk in chunks_for_chains(&border.chains) {
            adjacency_by_chunk
                .entry(chunk)
                .or_insert_with(Vec::new)
                .push(index);
        }
    }

    let mut terrain_by_chunk: HashMap<u16, Vec<usize>> = HashMap::new();
    for (index, border) in terrain_adjacency.borders.iter().enumerate() {
        for chunk in chunks_for_chains(&border.chains) {
            terrain_by_chunk
                .entry(chunk)
                .or_insert_with(Vec::new)
                .push(index);
        }
    }

    let mut all_chunks: Vec<u16> = adjacency_by_chunk
        .keys()
        .chain(terrain_by_chunk.keys())
        .copied()
        .collect();
    all_chunks.sort_unstable();
    all_chunks.dedup();

    BorderChunkIndex {
        province_chunks,
        adjacency_by_chunk,
        terrain_by_chunk,
        all_chunks,
    }
}

fn dirty_chunks_from_provinces(
    chunk_index: &BorderChunkIndex,
    changed_provinces: &HashSet<u32>,
) -> Vec<u16> {
    let mut dirty = Vec::new();
    for &province_id in changed_provinces {
        let Some(chunks) = chunk_index.province_chunks.get(province_id as usize) else {
            continue;
        };
        for &chunk in chunks {
            if !dirty.contains(&chunk) {
                dirty.push(chunk);
            }
        }
    }
    dirty
}

fn chunk_id_for_point(point: [f32; 2]) -> u16 {
    let x = point[0].clamp(0.0, MAP_WIDTH - f32::EPSILON);
    let y = (point[1] + 90.0).clamp(0.0, 180.0 - f32::EPSILON);
    let col = (x / BORDER_CHUNK_WIDTH).floor() as u16;
    let row = (y / BORDER_CHUNK_HEIGHT).floor() as u16;
    let chunk_cols = BORDER_CHUNK_COLS as u16;
    row.saturating_mul(chunk_cols) + col.min(chunk_cols - 1)
}

fn chunks_for_chains(chains: &[Vec<[f32; 2]>]) -> Vec<u16> {
    let mut chunks = Vec::new();
    for &point in chains.iter().flat_map(|chain| chain.iter()) {
        let chunk = chunk_id_for_point(point);
        if !chunks.contains(&chunk) {
            chunks.push(chunk);
        }
    }
    chunks
}

#[allow(clippy::too_many_arguments)]
fn build_chunk_geometry(
    chunk_id: u16,
    chunk_index: &BorderChunkIndex,
    adjacency: &[CachedBorder],
    terrain_adjacency: &TerrainAdjacencyData,
    map: &shared::map::MapData,
    admin_areas: &[shared::AdminArea],
    admin_assignments: &AdminMap,
    country_map: &CountryMap,
    non_playable_provinces: &NonPlayableProvinces,
    is_wasteland: impl Fn(usize) -> bool,
    province_owner: &impl Fn(usize) -> Option<OwnerKey>,
    positions: &mut Vec<[f32; 3]>,
    offsets: &mut Vec<[f32; 2]>,
    tiers: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    let mut junction_rims: HashMap<(i32, i32), ([f32; 2], Vec<[f32; 2]>)> = HashMap::new();

    if let Some(border_indexes) = chunk_index.adjacency_by_chunk.get(&chunk_id) {
        for &border_index in border_indexes {
            let border = &adjacency[border_index];
            let ia = border.provinces[0] as usize;
            let ib = border.provinces[1] as usize;
            if is_wasteland(ia) || is_wasteland(ib) {
                continue;
            }
            let Some(border_tier) = province_border_tier(
                map.provinces[ia].id,
                map.provinces[ib].id,
                admin_areas,
                admin_assignments,
                country_map,
                province_owner(ia),
                province_owner(ib),
            ) else {
                continue;
            };

            for chain in &border.chains {
                if chain.len() < 2 || !chain_touches_chunk(chain, chunk_id) {
                    continue;
                }
                polyline_to_border_strip(chain, positions, offsets, tiers, indices, border_tier);
                if border_tier == BorderTier::Country {
                    collect_country_junction_rims(&mut junction_rims, chain);
                }
            }
        }
    }

    let mut terrain_owner_cache: HashMap<u32, Option<String>> = HashMap::new();
    if let Some(border_indexes) = chunk_index.terrain_by_chunk.get(&chunk_id) {
        for &border_index in border_indexes {
            let terrain_border = &terrain_adjacency.borders[border_index];
            if terrain_polygon_is_water(terrain_border.terrain_index, terrain_adjacency) {
                continue;
            }
            let Some(province_tag) = province_country_tag(
                admin_areas,
                admin_assignments,
                country_map,
                terrain_border.province_id,
            ) else {
                continue;
            };
            if non_playable_provinces
                .0
                .contains(&terrain_border.province_id)
            {
                continue;
            }
            let terrain_owner = terrain_owner_cache
                .entry(terrain_border.terrain_index)
                .or_insert_with(|| {
                    terrain_polygon_surrounding_tag(
                        terrain_border.terrain_index,
                        terrain_adjacency,
                        admin_areas,
                        admin_assignments,
                        country_map,
                        non_playable_provinces,
                    )
                });
            if !terrain_border_should_render(terrain_owner.as_deref(), province_tag) {
                continue;
            }
            for chain in &terrain_border.chains {
                if chain.len() < 2 || !chain_touches_chunk(chain, chunk_id) {
                    continue;
                }
                polyline_to_border_strip(
                    chain,
                    positions,
                    offsets,
                    tiers,
                    indices,
                    BorderTier::Country,
                );
                collect_country_junction_rims(&mut junction_rims, chain);
            }
        }
    }

    for (_, (center, rim_points)) in junction_rims {
        if rim_points.len() >= 6 {
            add_junction_fill(center, &rim_points, positions, offsets, tiers, indices);
        }
    }
}

fn chain_touches_chunk(chain: &[[f32; 2]], chunk_id: u16) -> bool {
    chain
        .iter()
        .copied()
        .any(|point| chunk_id_for_point(point) == chunk_id)
}

#[allow(clippy::too_many_arguments)]
fn upsert_chunk_mesh(
    chunk_id: u16,
    positions: &mut Vec<[f32; 3]>,
    offsets: &mut Vec<[f32; 2]>,
    tiers: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    border_assets: &mut BorderAssets,
    material_handle: &Handle<BorderMaterial>,
    existing_entities: Option<&Vec<Entity>>,
) {
    if positions.is_empty() {
        if let Some(entities) = existing_entities {
            for &entity in entities {
                commands.entity(entity).despawn();
            }
        }
        border_assets.meshes.remove(&chunk_id);
        return;
    }

    let mesh_handle = border_assets
        .meshes
        .entry(chunk_id)
        .or_insert_with(|| {
            meshes.add(Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            ))
        })
        .clone();

    if let Some(mesh) = meshes.get_mut(&mesh_handle) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, std::mem::take(positions));
        mesh.insert_attribute(ATTRIBUTE_BORDER_OFFSET, std::mem::take(offsets));
        mesh.insert_attribute(ATTRIBUTE_BORDER_TIER, std::mem::take(tiers));
        mesh.insert_indices(Indices::U32(std::mem::take(indices)));
    }

    if existing_entities.is_none() {
        for &x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
            commands.spawn((
                Mesh2d(mesh_handle.clone()),
                MeshMaterial2d(material_handle.clone()),
                Transform::from_xyz(x_off, 0.0, 0.8),
                BorderMesh,
                BorderChunk(chunk_id),
            ));
        }
    }
}

fn border_scratch_capacity_bytes(
    positions: &Vec<[f32; 3]>,
    offsets: &Vec<[f32; 2]>,
    tiers: &Vec<f32>,
    indices: &Vec<u32>,
) -> usize {
    positions
        .capacity()
        .saturating_mul(std::mem::size_of::<[f32; 3]>())
        .saturating_add(
            offsets
                .capacity()
                .saturating_mul(std::mem::size_of::<[f32; 2]>()),
        )
        .saturating_add(tiers.capacity().saturating_mul(std::mem::size_of::<f32>()))
        .saturating_add(
            indices
                .capacity()
                .saturating_mul(std::mem::size_of::<u32>()),
        )
}

fn cached_borders_bytes(borders: &[CachedBorder]) -> usize {
    borders.iter().fold(
        borders
            .len()
            .saturating_mul(std::mem::size_of::<CachedBorder>()),
        |acc, border| {
            acc.saturating_add(
                border
                    .chains
                    .len()
                    .saturating_mul(std::mem::size_of::<Vec<[f32; 2]>>()),
            )
            .saturating_add(
                border
                    .chains
                    .iter()
                    .map(|chain| {
                        chain
                            .capacity()
                            .saturating_mul(std::mem::size_of::<[f32; 2]>())
                    })
                    .sum::<usize>(),
            )
        },
    )
}

fn border_quantize(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

fn border_qpt(p: [f32; 2]) -> (i32, i32) {
    (border_quantize(p[0]), border_quantize(p[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_keys_store_distinct_tags() {
        let mut keys: HashMap<String, u32> = HashMap::new();
        let mut next_key = 0_u32;
        for tag in ["A", "A", "B"] {
            keys.entry(tag.to_owned()).or_insert_with(|| {
                let key = next_key;
                next_key += 1;
                key
            });
        }
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn owned_terrain_skips_same_tag_borders() {
        assert!(!terrain_border_should_render(Some("A"), "A"));
        assert!(terrain_border_should_render(Some("A"), "B"));
        assert!(terrain_border_should_render(None, "A"));
    }
}

/// Build a continuous quad-strip topology for a polyline, storing center positions plus
/// canonical offset vectors so zoom-driven width changes can happen in the shader.
fn polyline_to_border_strip(
    points: &[[f32; 2]],
    positions: &mut Vec<[f32; 3]>,
    offsets: &mut Vec<[f32; 2]>,
    tiers: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    border_tier: BorderTier,
) {
    if points.len() < 2 {
        return;
    }
    let n = points.len();
    let z = border_tier_z(border_tier);
    let tier_id = border_tier_id(border_tier);

    let perp = |dx: f32, dy: f32| -> (f32, f32) {
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            (0.0, 0.0)
        } else {
            (-dy / len, dx / len)
        }
    };

    let mut left: Vec<[f32; 2]> = Vec::with_capacity(n);
    let mut right: Vec<[f32; 2]> = Vec::with_capacity(n);

    for i in 0..n {
        let [x, y] = points[i];
        let offset = if i == 0 {
            let [x1, y1] = points[1];
            let (px, py) = perp(x1 - x, y1 - y);
            (px, py)
        } else if i == n - 1 {
            let [xp, yp] = points[n - 2];
            let (px, py) = perp(x - xp, y - yp);
            (px, py)
        } else {
            let [xp, yp] = points[i - 1];
            let [xn, yn] = points[i + 1];
            let (p0x, p0y) = perp(x - xp, y - yp);
            let (p1x, p1y) = perp(xn - x, yn - y);
            let mx = p0x + p1x;
            let my = p0y + p1y;
            let mlen = (mx * mx + my * my).sqrt();
            if mlen < 1e-9 {
                (p0x, p0y)
            } else {
                let mux = mx / mlen;
                let muy = my / mlen;
                let dot = mux * p0x + muy * p0y;
                let scale = if dot.abs() < 1e-6 {
                    1.0
                } else {
                    (1.0 / dot).min(4.0)
                };
                (mux * scale, muy * scale)
            }
        };
        left.push([-offset.0, -offset.1]);
        right.push([offset.0, offset.1]);
    }

    let base = positions.len() as u32;
    for i in 0..n {
        positions.push([points[i][0], points[i][1], z]);
        positions.push([points[i][0], points[i][1], z]);
        offsets.push(left[i]);
        offsets.push(right[i]);
        tiers.push(tier_id);
        tiers.push(tier_id);
    }
    for i in 0..(n as u32 - 1) {
        let l0 = base + i * 2;
        let r0 = l0 + 1;
        let l1 = l0 + 2;
        let r1 = l0 + 3;
        indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
    }
}

fn endpoint_cap_span(points: &[[f32; 2]], half_w: f32, at_start: bool) -> Option<[[f32; 2]; 2]> {
    if points.len() < 2 {
        return None;
    }
    let (center, other) = if at_start {
        (points[0], points[1])
    } else {
        (points[points.len() - 1], points[points.len() - 2])
    };
    let dx = other[0] - center[0];
    let dy = other[1] - center[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return None;
    }
    let px = -dy / len * half_w;
    let py = dx / len * half_w;
    Some([
        [center[0] - px, center[1] - py],
        [center[0] + px, center[1] + py],
    ])
}

fn add_junction_fill(
    center: [f32; 2],
    rim_points: &[[f32; 2]],
    positions: &mut Vec<[f32; 3]>,
    offsets: &mut Vec<[f32; 2]>,
    tiers: &mut Vec<f32>,
    indices: &mut Vec<u32>,
) {
    if rim_points.len() < 3 {
        return;
    }

    let mut sorted = rim_points.to_vec();
    sorted.sort_by(|lhs, rhs| {
        let angle_l = (lhs[1] - center[1]).atan2(lhs[0] - center[0]);
        let angle_r = (rhs[1] - center[1]).atan2(rhs[0] - center[0]);
        angle_l
            .partial_cmp(&angle_r)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut polygon: Vec<[f32; 2]> = Vec::with_capacity(sorted.len());
    for point in sorted {
        let is_duplicate = polygon.iter().any(|existing| {
            (existing[0] - point[0]).abs() < 1e-5 && (existing[1] - point[1]).abs() < 1e-5
        });
        if !is_duplicate {
            polygon.push(point);
        }
    }
    if polygon.len() < 3 {
        return;
    }

    let base = positions.len() as u32;
    positions.push([center[0], center[1], border_tier_z(BorderTier::Country)]);
    offsets.push([0.0, 0.0]);
    tiers.push(border_tier_id(BorderTier::Country));
    for point in &polygon {
        positions.push([center[0], center[1], border_tier_z(BorderTier::Country)]);
        offsets.push([point[0] - center[0], point[1] - center[1]]);
        tiers.push(border_tier_id(BorderTier::Country));
    }
    let poly_len = polygon.len() as u32;
    for k in 0..poly_len {
        indices.push(base);
        indices.push(base + 1 + k);
        indices.push(base + 1 + ((k + 1) % poly_len));
    }
}

fn update_border_material_params(
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    border_assets: Res<BorderAssets>,
    mut materials: ResMut<Assets<BorderMaterial>>,
    mut last_proj_scale: Local<Option<f32>>,
) {
    let Some(material_handle) = border_assets.material.as_ref() else {
        return;
    };
    let proj_scale = camera_q
        .get_single()
        .map(|projection| projection.scale)
        .unwrap_or(0.05);
    let changed = last_proj_scale
        .map(|last| (last - proj_scale).abs() > 0.0005)
        .unwrap_or(true);
    if !changed {
        return;
    }
    let Some(material) = materials.get_mut(material_handle) else {
        return;
    };
    material.params.proj_scale = proj_scale;
    *last_proj_scale = Some(proj_scale);
}
