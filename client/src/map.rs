use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{u32_to_f32, u32_to_usize, usize_to_u32};
use shared::map::{MapData, MapProvince};
use shared::GameState;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::net::LatestGameState;

const MAP_BIN_PATH: &str = "assets/map.bin";

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
    #[default]
    Political,
    Population,
    Production,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MapMode::Political => write!(f, "Political"),
            MapMode::Population => write!(f, "Population"),
            MapMode::Production => write!(f, "Production"),
        }
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

/// Country code → deterministic RGBA color.
fn country_color_rgba(code: &str) -> [f32; 4] {
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    let h = hasher.finish();
    let byte = |shift: u32| f32::from(u8::try_from((h >> shift) & 0xFF).unwrap());
    let r = byte(0) / 255.0 * 0.6 + 0.2;
    let g = byte(8) / 255.0 * 0.6 + 0.2;
    let b = byte(16) / 255.0 * 0.6 + 0.2;
    [r, g, b, 1.0]
}

fn heatmap_rgba(t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let r = (2.0 * t - 0.5).clamp(0.0, 1.0);
    let g = (1.0 - (2.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    let b = (1.0 - 2.0 * t).clamp(0.0, 1.0);
    [r * 0.8 + 0.1, g * 0.8 + 0.1, b * 0.8 + 0.1, 1.0]
}

fn brighten(c: [f32; 4]) -> [f32; 4] {
    [
        (c[0] + 0.25).min(1.0),
        (c[1] + 0.25).min(1.0),
        (c[2] + 0.25).min(1.0),
        c[3],
    ]
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
        let color = country_color_rgba(&mp.tag);
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

    commands.spawn((
        Mesh2d(mesh_handle.clone()),
        MeshMaterial2d(materials.add(ColorMaterial::from_color(Color::WHITE))),
        Transform::default(),
    ));

    commands.insert_resource(ProvinceVertexMap {
        ranges,
        mesh_handle,
    });
    commands.insert_resource(MapResource(map_data));
}

/// Compute the base color for a province given the current map mode.
fn province_base_color(
    pid: usize,
    gs: &GameState,
    map_data: &MapData,
    mode: &MapMode,
    max_pop: u32,
    max_prod: f32,
) -> [f32; 4] {
    if pid < gs.provinces.len() {
        let province = &gs.provinces[pid];
        match mode {
            MapMode::Political => {
                let owner = province.owner.as_deref().unwrap_or("UNK");
                country_color_rgba(owner)
            }
            MapMode::Population => {
                let total: u32 = province.pops.iter().map(|p| p.size).sum();
                heatmap_rgba(u32_to_f32(total) / u32_to_f32(max_pop))
            }
            MapMode::Production => {
                let total: f32 = province.stockpile.values().sum();
                heatmap_rgba(total / max_prod)
            }
        }
    } else {
        country_color_rgba(&map_data.provinces[pid].tag)
    }
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

/// Update vertex colors only when game state, map mode, or selection changes.
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
    let Some(gs) = &state.0 else { return };
    let Some(map) = map else { return };
    let Some(vm) = vertex_map else { return };

    let mode_changed = last.mode != Some(*mode);
    let selection_changed = last.selected != selected.0;
    let tick_changed = last.tick != gs.tick;

    // Economy only runs every 100 ticks; colors only change meaningfully then.
    let economy_boundary =
        tick_changed && (last.tick == 0 || gs.tick / 100 != last.tick / 100);
    let needs_full_recolor = mode_changed || economy_boundary;

    if !needs_full_recolor && !selection_changed {
        if tick_changed {
            last.tick = gs.tick;
        }
        return;
    }

    let Some(mesh) = meshes.get_mut(&vm.mesh_handle) else {
        return;
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
                    let base = province_base_color(
                        pid, gs, &map.0, &*mode, last.max_pop, last.max_prod,
                    );
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
                    let base = province_base_color(
                        pid, gs, &map.0, &*mode, last.max_pop, last.max_prod,
                    );
                    let color = brighten(base);
                    for i in start..(start + count) {
                        colors[i] = color;
                    }
                }
            }
        }
        last.selected = selected.0;
        last.tick = gs.tick;
        return;
    }

    // Full recolor (on economy tick or mode change).
    last.tick = gs.tick;
    last.mode = Some(*mode);
    last.selected = selected.0;

    // Pre-compute heatmap normalization.
    let (max_pop, max_prod) = if *mode != MapMode::Political {
        compute_normalization(gs)
    } else {
        (1, 1.0)
    };
    last.max_pop = max_pop;
    last.max_prod = max_prod;

    // Modify vertex colors in-place to avoid allocation.
    if let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    {
        for (pid, (start, count)) in vm.ranges.iter().enumerate() {
            if *count == 0 {
                continue;
            }
            let is_selected = selected.0 == Some(usize_to_u32(pid));
            let base = province_base_color(pid, gs, &map.0, &*mode, max_pop, max_prod);
            let color = if is_selected { brighten(base) } else { base };
            for i in *start..(*start + *count) {
                colors[i] = color;
            }
        }
    }
}

/// Camera pan (right-click drag) and zoom (scroll wheel).
fn camera_controls(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut scroll_evts: EventReader<bevy::input::mouse::MouseWheel>,
    mut motion_evts: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera_q: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_q.get_single_mut() else {
        return;
    };

    if mouse_input.pressed(MouseButton::Right) {
        for ev in motion_evts.read() {
            transform.translation.x -= ev.delta.x * projection.scale;
            transform.translation.y += ev.delta.y * projection.scale;
        }
    } else {
        motion_evts.clear();
    }

    for ev in scroll_evts.read() {
        let zoom_factor = 1.0 - ev.y * 0.1;
        projection.scale *= zoom_factor.clamp(0.5, 2.0);
        projection.scale = projection.scale.clamp(0.01, 0.5);
    }
}

/// Point-in-polygon test (ray casting algorithm).
fn point_in_polygon(px: f32, py: f32, ring: &[[f32; 2]]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_province(px: f32, py: f32, mp: &MapProvince) -> bool {
    if mp.boundary.is_empty() {
        return false;
    }
    if !point_in_polygon(px, py, &mp.boundary[0]) {
        return false;
    }
    for hole in mp.boundary.iter().skip(1) {
        if point_in_polygon(px, py, hole) {
            return false;
        }
    }
    true
}

/// Detect left-click on a province. Uses bounding box pre-filter.
/// Iterates in reverse so CN provinces (higher z, later IDs) are checked first.
fn province_click(
    mouse_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Option<Res<MapResource>>,
    mut selected: ResMut<SelectedProvince>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(map) = map else { return };
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) else {
        return;
    };

    let px = world_pos.x;
    let py = world_pos.y;

    // Iterate in reverse so CN provinces (higher z, appended later) take priority.
    for mp in map.0.provinces.iter().rev() {
        if mp.boundary.is_empty() {
            continue;
        }
        // Bounding box pre-filter.
        let ring = &mp.boundary[0];
        let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
        let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
        for pt in ring {
            min_x = min_x.min(pt[0]);
            max_x = max_x.max(pt[0]);
            min_y = min_y.min(pt[1]);
            max_y = max_y.max(pt[1]);
        }
        if px < min_x || px > max_x || py < min_y || py > max_y {
            continue;
        }
        if point_in_province(px, py, mp) {
            selected.0 = Some(mp.id);
            return;
        }
    }
    selected.0 = None;
}

/// Keyboard shortcuts: 1 = Political, 2 = Population, 3 = Production.
fn map_mode_switch(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<MapMode>) {
    if keys.just_pressed(KeyCode::Digit1) {
        *mode = MapMode::Political;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        *mode = MapMode::Population;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        *mode = MapMode::Production;
    }
}
