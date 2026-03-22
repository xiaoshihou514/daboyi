mod color;
pub mod borders;
mod interact;

use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{u32_to_usize, usize_to_u32};
use shared::map::MapData;
use std::collections::HashMap;

use crate::editor::{ActiveCountry, EditorCountries, MapColoring};
use crate::state::AppState;
use color::{brighten, owner_color_rgba, terrain_province_color};
use interact::{camera_controls, map_mode_switch, province_click};
pub use borders::BordersPlugin;

pub const MAP_BIN_PATH: &str = "assets/map.bin";
const COUNTRY_COLORS_TSV: &str = "assets/country_colors.tsv";
/// Equal Earth x-range width: longitude ±180° maps exactly to x ∈ [-180, 180].
pub const MAP_WIDTH: f32 = 360.0;

/// EU5 country tag → RGBA color loaded from country_colors.tsv (for Province seed data).
#[derive(Resource, Default)]
pub struct CountryColors(pub HashMap<String, [f32; 4]>);

pub struct MapPlugin;

/// Loaded map geometry, available as a Bevy resource.
#[derive(Resource)]
pub struct MapResource(pub MapData);

/// Currently selected province.
#[derive(Resource, Default)]
pub struct SelectedProvince(pub Option<u32>);

/// Map coloring mode, switchable with keys 1/2/3.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MapMode {
    Province,
    Terrain,
    #[default]
    Political,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let key = match self {
            MapMode::Province => "map_province",
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

/// Tracks the last coloring state to avoid redundant recoloring.
#[derive(Resource, Default)]
struct LastColorState {
    mode: Option<MapMode>,
    selected: Option<u32>,
    /// Counter incremented whenever MapColoring changes (detected via version bump).
    coloring_version: u64,
}

/// Incremented every time MapColoring is modified, so color_provinces can detect changes.
#[derive(Resource, Default)]
pub struct ColoringVersion(pub u64);

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SelectedProvince::default())
            .insert_resource(MapMode::default())
            .insert_resource(LastColorState::default())
            .insert_resource(CountryColors::default())
            .insert_resource(ColoringVersion::default())
            .add_systems(Startup, load_map)
            .add_systems(Update, color_provinces.run_if(in_state(AppState::Editing)))
            .add_systems(
                Update,
                (camera_controls, province_click, map_mode_switch)
                    .run_if(in_state(AppState::Editing)),
            );
    }
}

/// Build a single merged mesh for ALL provinces.
fn load_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let map_data = match MapData::load(MAP_BIN_PATH) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to load {MAP_BIN_PATH}: {e}");
            eprintln!("Map will not be rendered. Run mapgen first.");
            next_state.set(AppState::Editing);
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

    println!(
        "Map mesh: {} vertices, {} triangles",
        all_positions.len(),
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

    // Load EU5 country colors (used when seeding from EU5 ownership data).
    let mut color_map: HashMap<String, [f32; 4]> = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(COUNTRY_COLORS_TSV) {
        for line in content.lines().skip(1) {
            let mut parts = line.splitn(4, '\t');
            if let (Some(tag), Some(r), Some(g), Some(b)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            {
                if let (Ok(r), Ok(g), Ok(b)) = (r.parse::<u8>(), g.parse::<u8>(), b.parse::<u8>()) {
                    color_map.insert(
                        tag.to_string(),
                        [
                            f32::from(r) / 255.0,
                            f32::from(g) / 255.0,
                            f32::from(b) / 255.0,
                            1.0,
                        ],
                    );
                }
            }
        }
        println!("Loaded {} country colors from {COUNTRY_COLORS_TSV}", color_map.len());
    }
    commands.insert_resource(CountryColors(color_map));

    next_state.set(AppState::Editing);
}

/// Recolor the province mesh based on current mode and coloring assignments.
fn color_provinces(
    map: Option<Res<MapResource>>,
    vertex_map: Option<Res<ProvinceVertexMap>>,
    mode: Res<MapMode>,
    selected: Res<SelectedProvince>,
    coloring: Res<MapColoring>,
    countries: Res<EditorCountries>,
    coloring_version: Res<ColoringVersion>,
    mut last: ResMut<LastColorState>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Some(map) = map else { return };
    let Some(vm) = vertex_map else { return };

    let mode_changed = last.mode != Some(*mode);
    let selection_changed = last.selected != selected.0;
    let coloring_changed = last.coloring_version != coloring_version.0;

    let needs_full_recolor = mode_changed || coloring_changed;

    if !needs_full_recolor && !selection_changed {
        return;
    }

    // Build a quick lookup: tag → color for editor countries.
    let country_color_lookup: HashMap<&str, [f32; 4]> = countries
        .0
        .iter()
        .map(|c| (c.tag.as_str(), c.color))
        .collect();

    let base_color = |pid: usize| -> [f32; 4] {
        match *mode {
            MapMode::Province => map.0.provinces[pid].hex_color,
            MapMode::Terrain => terrain_province_color(&map.0.provinces[pid].topography),
            MapMode::Political => {
                let topo = &map.0.provinces[pid].topography;
                let prov_id = map.0.provinces[pid].id;

                if let Some(tag) = coloring.assignments.get(&prov_id) {
                    // Province is assigned to a country.
                    let country_color = country_color_lookup
                        .get(tag.as_str())
                        .copied()
                        .unwrap_or_else(|| owner_color_rgba(tag));

                    if topo.contains("wasteland") {
                        // Blend 50/50 with terrain color for wasteland.
                        let wc = terrain_province_color(topo);
                        return [
                            (wc[0] + country_color[0]) * 0.5,
                            (wc[1] + country_color[1]) * 0.5,
                            (wc[2] + country_color[2]) * 0.5,
                            1.0,
                        ];
                    }
                    return country_color;
                }

                // Unassigned: wasteland shows terrain, others neutral gray.
                if topo.contains("wasteland") {
                    terrain_province_color(topo)
                } else {
                    [0.55, 0.55, 0.55, 1.0]
                }
            }
        }
    };

    let Some(mesh) = meshes.get_mut(&vm.mesh_handle) else {
        return;
    };

    // Targeted selection-only update (cheap).
    if !needs_full_recolor && selection_changed {
        if let Some(VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
        {
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
        return;
    }

    // Full recolor.
    last.mode = Some(*mode);
    last.selected = selected.0;
    last.coloring_version = coloring_version.0;

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
