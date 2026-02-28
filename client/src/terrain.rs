use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::usize_to_u32;
use shared::map::{RiverData, TerrainData};

const TERRAIN_BIN_PATH: &str = "assets/terrain.bin";
const RIVERS_BIN_PATH: &str = "assets/rivers.bin";
/// Three copies of the 360°-wide world for seamless horizontal wrapping.
const WORLD_OFFSETS: [f32; 3] = [-360.0, 0.0, 360.0];

/// River width in world-space degrees per width class.
const RIVER_WIDTHS: [f32; 3] = [0.10, 0.18, 0.30];
/// River RGBA color.
const RIVER_COLOR: [f32; 4] = [0.18, 0.47, 0.75, 0.85];

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_terrain, spawn_rivers));
    }
}

/// Build a single merged mesh from all terrain polygons and spawn three copies.
fn load_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let terrain = match TerrainData::load(TERRAIN_BIN_PATH) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to load {TERRAIN_BIN_PATH}: {e}");
            eprintln!("Terrain will not be rendered. Run mapgen first.");
            return;
        }
    };

    // Count total vertices/indices up front for pre-allocation.
    let total_verts: usize = terrain.polygons.iter().map(|p| p.vertices.len()).sum();
    let total_idxs: usize = terrain.polygons.iter().map(|p| p.indices.len()).sum();

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(total_verts);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(total_verts);
    let mut indices: Vec<u32> = Vec::with_capacity(total_idxs);

    for poly in &terrain.polygons {
        let base = usize_to_u32(positions.len());
        for &[x, y] in &poly.vertices {
            positions.push([x, y, 0.0]);
            colors.push(poly.color);
        }
        for &i in &poly.indices {
            indices.push(i + base);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    let handle = meshes.add(mesh);
    let material = materials.add(ColorMaterial::default());

    for &x_off in &WORLD_OFFSETS {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, -1.0),
        ));
    }

    eprintln!(
        "Terrain: {} polygons, {} vertices",
        terrain.polygons.len(),
        total_verts,
    );
}

/// Build a quad-strip mesh for a single river polyline.
/// Each segment becomes a rectangle (4 verts, 2 tris).
fn polyline_to_quads(
    points: &[[f32; 2]],
    half_w: f32,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    z: f32,
) {
    if points.len() < 2 {
        return;
    }
    for seg in points.windows(2) {
        let [x0, y0] = seg[0];
        let [x1, y1] = seg[1];
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            continue;
        }
        // Perpendicular unit vector
        let px = -dy / len * half_w;
        let py = dx / len * half_w;

        let base = usize_to_u32(positions.len());
        positions.push([x0 - px, y0 - py, z]);
        positions.push([x0 + px, y0 + py, z]);
        positions.push([x1 + px, y1 + py, z]);
        positions.push([x1 - px, y1 - py, z]);
        for _ in 0..4 {
            colors.push(RIVER_COLOR);
        }
        // Two triangles: (0,1,2) and (0,2,3)
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Load rivers.bin and build a single triangle mesh for all river segments.
fn spawn_rivers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let river_data = match RiverData::load(RIVERS_BIN_PATH) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to load {RIVERS_BIN_PATH}: {e}");
            eprintln!("Rivers will not be rendered. Run tools/extract_rivers_vector.py first.");
            return;
        }
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for river in &river_data.rivers {
        let half_w = RIVER_WIDTHS[river.width_class as usize] / 2.0;
        let pts: Vec<[f32; 2]> = river.points.iter().map(|&p| p).collect();
        polyline_to_quads(&pts, half_w, &mut positions, &mut colors, &mut indices, 0.5);
    }

    if positions.is_empty() {
        eprintln!("rivers.bin contained no renderable river segments");
        return;
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions.clone());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors.clone());
    mesh.insert_indices(Indices::U32(indices.clone()));
    let handle = meshes.add(mesh);
    let material = materials.add(ColorMaterial::default());

    for &x_off in &WORLD_OFFSETS {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, 0.5),
        ));
    }

    eprintln!(
        "Rivers: {} polylines, {} quads",
        river_data.rivers.len(),
        indices.len() / 6,
    );
}
