use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{u32_to_f32, u32_to_usize, usize_to_u32};
use shared::map::{MapData, MapProvince};
use shared::GameState;

use crate::net::LatestGameState;

const MAP_BIN_PATH: &str = "assets/map.bin";
/// Equal Earth x-range width: longitude ±180° maps exactly to x ∈ [-180, 180].
const MAP_WIDTH: f32 = 360.0;

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
    Political,
    Population,
    Production,
    Terrain,
    Owner,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MapMode::Political => write!(f, "Political"),
            MapMode::Population => write!(f, "Population"),
            MapMode::Production => write!(f, "Production"),
            MapMode::Terrain => write!(f, "Terrain"),
            MapMode::Owner => write!(f, "Owner"),
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

/// Deterministic RGBA color for a country owner tag.
/// Uses FNV-1a hash — stable across runs unlike DefaultHasher.
fn owner_color_rgba(tag: &str) -> [f32; 4] {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    let mut h = FNV_OFFSET;
    for byte in tag.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }
    // Map hash bytes to visually distinct mid-range colors (avoid very dark/light).
    let r = f32::from(u8::try_from((h >> 0) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    let g = f32::from(u8::try_from((h >> 8) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    let b = f32::from(u8::try_from((h >> 16) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    [r, g, b, 1.0]
}

fn heatmap_rgba(t: f32) -> [f32; 4] {    let t = t.clamp(0.0, 1.0);
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

/// Terrain mode: color by province topography.
fn terrain_province_color(topography: &str) -> [f32; 4] {
    match topography {
        "mountains" => [0.420, 0.357, 0.306, 1.0],  // dark muted brown
        "hills" => [0.608, 0.545, 0.447, 1.0],        // tan
        "plateau" => [0.690, 0.627, 0.502, 1.0],      // light tan
        "wetlands" => [0.420, 0.545, 0.420, 1.0],     // muted teal-green
        "desert" | "sparse_desert" | "dunes" => [0.784, 0.659, 0.431, 1.0], // sandy
        "flatland" | "farmland" => [0.545, 0.667, 0.482, 1.0],              // green
        _ => [0.604, 0.604, 0.545, 1.0],
    }
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
    let gs_independent = matches!(*mode, MapMode::Political | MapMode::Terrain);

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
        return;
    }

    // Pre-compute normalization and update last BEFORE the closure borrows last.
    let (max_pop, max_prod) = if let Some(gs) = &state.0 {
        if !matches!(*mode, MapMode::Political | MapMode::Terrain | MapMode::Owner) {
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
            MapMode::Political => map.0.provinces[pid].hex_color,
            MapMode::Terrain => terrain_province_color(&map.0.provinces[pid].topography),
            MapMode::Owner => {
                if let Some(gs) = &state.0 {
                    if pid < gs.provinces.len() {
                        return owner_color_rgba(gs.provinces[pid].owner.as_deref().unwrap_or("UNK"));
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
        // Wrap x so the camera stays in [-180, 180) — map copies cover the rest.
        let half = MAP_WIDTH / 2.0;
        if transform.translation.x >= half {
            transform.translation.x -= MAP_WIDTH;
        } else if transform.translation.x < -half {
            transform.translation.x += MAP_WIDTH;
        }
        // Clamp y to equirectangular pole-to-pole range (±90°).
        transform.translation.y = transform.translation.y.clamp(-90.0, 90.0);
    } else {
        motion_evts.clear();
    }

    for ev in scroll_evts.read() {
        let zoom_factor = 1.0 - ev.y * 0.1;
        projection.scale *= zoom_factor.clamp(0.5, 2.0);
        // Max scale 0.1 ensures the visible width ≤ MAP_WIDTH on screens up to ~3600px.
        projection.scale = projection.scale.clamp(0.01, 0.1);
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

    let py = world_pos.y;
    // Wrap x into map coordinate range [-180, 180) to hit-test any copy.
    let px = world_pos.x - (((world_pos.x + 180.0) / MAP_WIDTH).floor() * MAP_WIDTH);

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

/// Keyboard shortcuts: 1 = Political, 2 = Population, 3 = Production, 4 = Terrain, 5 = Owner.
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
    if keys.just_pressed(KeyCode::Digit4) {
        *mode = MapMode::Terrain;
    }
    if keys.just_pressed(KeyCode::Digit5) {
        *mode = MapMode::Owner;
    }
}
