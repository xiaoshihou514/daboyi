use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{u32_to_usize, usize_to_u32};
use shared::map::{RiverData, TerrainData};
use std::collections::HashMap;

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

    let base = usize_to_u32(positions.len());
    for i in point_start..=point_end {
        positions.push([left[i][0], left[i][1], z]);
        positions.push([right[i][0], right[i][1], z]);
        colors.push(RIVER_COLOR);
        colors.push(RIVER_COLOR);
    }

    let point_count = point_end - point_start + 1;
    for i in 0..(point_count - 1) {
        let local = usize_to_u32(i);
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

    let base = usize_to_u32(positions.len());
    positions.push([center[0], center[1], z]);
    colors.push(RIVER_COLOR);
    for point in &polygon {
        positions.push([point[0], point[1], z]);
        colors.push(RIVER_COLOR);
    }
    let poly_len = usize_to_u32(polygon.len());
    for k in 0..poly_len {
        indices.push(base);
        indices.push(base + 1 + k);
        indices.push(base + 1 + ((k + 1) % poly_len));
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
    let mut junction_rims: HashMap<u32, Vec<[f32; 2]>> = HashMap::new();

    for edge in &river_data.edges {
        let half_w = RIVER_WIDTHS[u32_to_usize(u32::from(edge.width_class))] / 2.0;
        let raw: Vec<[f32; 2]> = edge.points.iter().map(|&p| p).collect();
        let pts = chaikin_smooth(&chaikin_smooth(&raw));
        if pts.len() < 2 {
            continue;
        }
        let Some((left, right)) = compute_strip_sides(&pts, half_w) else {
            continue;
        };

        if left.len() >= 4 {
            emit_quad_strip_range(
                &left,
                &right,
                1,
                left.len() - 2,
                &mut positions,
                &mut colors,
                &mut indices,
                0.5,
            );
        }

        if let Some(merge_points) = endpoint_merge_points(&left, &right, true) {
            let entry = junction_rims.entry(edge.start_node).or_default();
            entry.extend_from_slice(&merge_points);
        }
        if let Some(merge_points) = endpoint_merge_points(&left, &right, false) {
            let entry = junction_rims.entry(edge.end_node).or_default();
            entry.extend_from_slice(&merge_points);
        }
    }

    for (node_id, mut rim_points) in junction_rims {
        rim_points.push(river_data.nodes[u32_to_usize(node_id)].position);
        if rim_points.len() >= 4 {
            add_junction_fill(&rim_points, &mut positions, &mut colors, &mut indices, 0.5);
        }
    }

    if positions.is_empty() {
        eprintln!("rivers.bin contained no renderable river segments");
        return;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
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
        "Rivers: {} edges, {} quads",
        river_data.edges.len(),
        indices.len() / 6,
    );
}
