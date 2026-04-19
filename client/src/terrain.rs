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
            Transform::from_xyz(x_off, 0.0, 1.0),
        ));
    }

    eprintln!(
        "Terrain: {} polygons, {} vertices",
        terrain.polygons.len(),
        total_verts,
    );
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

/// Build a continuous quad-strip mesh for a river polyline with miter joins.
/// Interior vertices share miter-bisected offsets — no gaps between segments.
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

    // Emit the shared quad-strip.
    let base = usize_to_u32(positions.len());
    for i in 0..n {
        positions.push([left[i][0], left[i][1], z]);
        positions.push([right[i][0], right[i][1], z]);
        colors.push(RIVER_COLOR);
        colors.push(RIVER_COLOR);
    }
    // 2 vertices per point. Segment i→i+1 uses verts (2i, 2i+1, 2i+2, 2i+3).
    for i in 0..(n as u32 - 1) {
        let l0 = base + i * 2;
        let r0 = l0 + 1;
        let l1 = l0 + 2;
        let r1 = l0 + 3;
        indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
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
        let raw: Vec<[f32; 2]> = river.points.iter().map(|&p| p).collect();
        let pts = chaikin_smooth(&chaikin_smooth(&raw));
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
