mod color;
mod interact;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{u32_to_f32, u32_to_usize, usize_to_u32};
use shared::map::MapData;
use shared::GameState;

use crate::net::LatestGameState;
use color::{brighten, heatmap_rgba, owner_color_rgba, terrain_province_color};
use interact::{camera_controls, map_mode_switch, province_click};

pub const MAP_BIN_PATH: &str = "assets/map.bin";
/// Equal Earth x-range width: longitude ±180° maps exactly to x ∈ [-180, 180].
pub const MAP_WIDTH: f32 = 360.0;

pub struct MapPlugin;

/// Loaded map geometry, available as a Bevy resource.
#[derive(Resource)]
pub struct MapResource(pub MapData);

/// Currently selected province.
#[derive(Resource, Default)]
pub struct SelectedProvince(pub Option<u32>);

/// Map coloring mode, switchable with keys 1/2/3/4/5.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MapMode {
    #[default]
    Province,
    Population,
    Production,
    Terrain,
    Political,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let key = match self {
            MapMode::Province => "map_province",
            MapMode::Population => "map_population",
            MapMode::Production => "map_production",
            MapMode::Terrain => "map_terrain",
            MapMode::Political => "map_political",
        };
        write!(f, "{}", rust_i18n::t!(key))
    }
}

/// Maps province_id → (start_vertex_index, vertex_count) in the merged mesh.
#[derive(Resource)]
struct ProvinceVertexMap {
    ranges: Vec<(usize, usize)>,
    mesh_handle: Handle<Mesh>,
}

/// Tracks the last state we colored for, to avoid redundant recoloring.
#[derive(Resource, Default)]
struct LastColorState {
    tick: u64,
    mode: Option<MapMode>,
    selected: Option<u32>,
    /// Cached normalization values (only change on economy ticks).
    max_pop: u32,
    max_prod: f32,
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SelectedProvince::default())
            .insert_resource(MapMode::default())
            .insert_resource(LastColorState::default())
            .add_systems(Startup, load_map)
            .add_systems(
                Update,
                (
                    color_provinces,
                    camera_controls,
                    province_click,
                    map_mode_switch,
                ),
            );
    }
}

/// Build a single merged mesh for ALL provinces (all at z=0.0, no overlaps after mapgen filtering).
fn load_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let map_data = match MapData::load(MAP_BIN_PATH) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to load {MAP_BIN_PATH}: {e}");
            eprintln!("Map will not be rendered. Run mapgen first.");
            return;
        }
    };

    println!("Loaded map: {} provinces", map_data.provinces.len());

    let mut all_positions: Vec<[f32; 3]> = Vec::new();
    let mut all_normals: Vec<[f32; 3]> = Vec::new();
    let mut all_uvs: Vec<[f32; 2]> = Vec::new();
    let mut all_colors: Vec<[f32; 4]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for mp in &map_data.provinces {
        if mp.vertices.is_empty() || mp.indices.is_empty() {
            ranges.push((0, 0));
            continue;
        }

        let start = all_positions.len();
        let color = mp.hex_color;
        let base_idx = usize_to_u32(all_positions.len());

        for v in &mp.vertices {
            all_positions.push([v[0], v[1], 0.0]);
            all_normals.push([0.0, 0.0, 1.0]);
            all_uvs.push(*v);
            all_colors.push(color);
        }
        for idx in &mp.indices {
            all_indices.push(idx + base_idx);
        }

        ranges.push((start, mp.vertices.len()));
    }

    let total_verts = all_positions.len();
    println!(
        "Map mesh: {} vertices, {} triangles",
        total_verts,
        all_indices.len() / 3
    );

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, all_positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, all_normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, all_uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, all_colors);
    mesh.insert_indices(Indices::U32(all_indices));

    let mesh_handle = meshes.add(mesh);

    // Spawn 3 side-by-side copies sharing the same mesh asset for seamless wrapping.
    // All 3 instances reflect color changes automatically when we mutate the shared mesh.
    let material = materials.add(ColorMaterial::from_color(Color::WHITE));
    for &x_offset in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
        commands.spawn((
            Mesh2d(mesh_handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_offset, 0.0, 0.0),
        ));
    }

    commands.insert_resource(ProvinceVertexMap {
        ranges,
        mesh_handle,
    });
    commands.insert_resource(MapResource(map_data));
}

/// Compute heatmap normalization values from game state.
fn compute_normalization(gs: &GameState) -> (u32, f32) {
    let mut mp = 0u32;
    let mut mprod = 0.0f32;
    for p in &gs.provinces {
        let total_pop: u32 = p.pops.iter().map(|pop| pop.size).sum();
        mp = mp.max(total_pop);
        let total_prod: f32 = p.stockpile.values().sum();
        mprod = mprod.max(total_prod);
    }
    (mp.max(1), mprod.max(1.0))
}

/// Skips expensive full recolor on non-economy ticks (economy runs every 100 ticks).
fn color_provinces(
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    vertex_map: Option<Res<ProvinceVertexMap>>,
    mode: Res<MapMode>,
    selected: Res<SelectedProvince>,
    mut last: ResMut<LastColorState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(map) = map else { return };
    let Some(vm) = vertex_map else { return };

    let mode_changed = last.mode != Some(*mode);
    let selection_changed = last.selected != selected.0;

    // Modes that don't need game state can render immediately.
    let gs_independent = matches!(*mode, MapMode::Province | MapMode::Terrain);

    let (tick_changed, economy_boundary) = if let Some(gs) = &state.0 {
        let tc = last.tick != gs.tick;
        let eb = tc && (last.tick == 0 || gs.tick / 100 != last.tick / 100);
        (tc, eb)
    } else {
        (false, false)
    };

    let needs_full_recolor = mode_changed || economy_boundary || (gs_independent && mode_changed);

    if !needs_full_recolor && !selection_changed {
        if tick_changed {
            if let Some(gs) = &state.0 {
                last.tick = gs.tick;
            }
        }
        return;
    }

    // For game-state-dependent modes we need a game state.
    if !gs_independent && state.0.is_none() {
        last.mode = Some(*mode);
        return;
    }

    // Pre-compute normalization and update last BEFORE the closure borrows last.
    let (max_pop, max_prod) = if let Some(gs) = &state.0 {
        if !matches!(*mode, MapMode::Province | MapMode::Terrain | MapMode::Political) {
            compute_normalization(gs)
        } else {
            (last.max_pop.max(1), last.max_prod.max(1.0))
        }
    } else {
        (last.max_pop.max(1), last.max_prod.max(1.0))
    };

    let Some(mesh) = meshes.get_mut(&vm.mesh_handle) else {
        return;
    };

    // Helper closure: pure read of state, map, mode, max_pop/max_prod (no last borrow).
    let base_color = |pid: usize| -> [f32; 4] {
        match *mode {
            MapMode::Province => map.0.provinces[pid].hex_color,
            MapMode::Terrain => terrain_province_color(&map.0.provinces[pid].topography),
            MapMode::Political => {
                // Wasteland provinces always show terrain color, not owner/unclaimed.
                let topo = &map.0.provinces[pid].topography;
                if topo.contains("wasteland") {
                    return terrain_province_color(topo);
                }
                if let Some(gs) = &state.0 {
                    if pid < gs.provinces.len() {
                        if let Some(owner) = gs.provinces[pid].owner.as_deref() {
                            return owner_color_rgba(owner);
                        }
                        // Unclaimed: neutral gray
                        return [0.55, 0.55, 0.55, 1.0];
                    }
                }
                map.0.provinces[pid].hex_color
            }
            MapMode::Population => {
                if let Some(gs) = &state.0 {
                    if pid < gs.provinces.len() {
                        let total: u32 = gs.provinces[pid].pops.iter().map(|p| p.size).sum();
                        return heatmap_rgba(u32_to_f32(total) / u32_to_f32(max_pop));
                    }
                }
                map.0.provinces[pid].hex_color
            }
            MapMode::Production => {
                if let Some(gs) = &state.0 {
                    if pid < gs.provinces.len() {
                        let total: f32 = gs.provinces[pid].stockpile.values().sum();
                        return heatmap_rgba(total / max_prod);
                    }
                }
                map.0.provinces[pid].hex_color
            }
        }
    };

    // Targeted selection-only update (cheap: ~60 vertices instead of millions).
    if !needs_full_recolor && selection_changed {
        if let Some(VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        {
            // Restore old selection to base color.
            if let Some(old_id) = last.selected {
                let pid = u32_to_usize(old_id);
                if pid < vm.ranges.len() {
                    let (start, count) = vm.ranges[pid];
                    let base = base_color(pid);
                    for i in start..(start + count) {
                        colors[i] = base;
                    }
                }
            }
            // Highlight new selection.
            if let Some(new_id) = selected.0 {
                let pid = u32_to_usize(new_id);
                if pid < vm.ranges.len() {
                    let (start, count) = vm.ranges[pid];
                    let col = brighten(base_color(pid));
                    for i in start..(start + count) {
                        colors[i] = col;
                    }
                }
            }
        }
        last.selected = selected.0;
        if let Some(gs) = &state.0 {
            last.tick = gs.tick;
        }
        return;
    }

    // Full recolor.
    if let Some(gs) = &state.0 {
        last.tick = gs.tick;
    }
    last.max_pop = max_pop;
    last.max_prod = max_prod;
    last.mode = Some(*mode);
    last.selected = selected.0;

    if let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    {
        for (pid, (start, count)) in vm.ranges.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            let is_selected = selected.0 == Some(usize_to_u32(pid));
            let base = base_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            for i in *start..(*start + *count) {
                colors[i] = color;
            }
        }
    }
}
