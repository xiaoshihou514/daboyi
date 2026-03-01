use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::usize_to_u32;

use crate::map::{MapMode, MapResource, MAP_WIDTH};
use crate::net::LatestGameState;
use crate::state::AppState;

const BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// Precomputed adjacency: pairs of province IDs that share an edge.
#[derive(Resource, Default)]
pub struct ProvinceAdjacency(pub Vec<[u32; 2]>);

/// Marker for the border mesh entities.
#[derive(Component)]
pub struct BorderMesh;

pub struct BordersPlugin;

impl Plugin for BordersPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProvinceAdjacency::default())
            .add_systems(
                Update,
                (compute_adjacency, rebuild_borders)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            );
    }
}

/// Compute province adjacency from MapData boundary rings — runs once when MapResource is ready.
pub fn compute_adjacency(
    map: Option<Res<MapResource>>,
    mut adjacency: ResMut<ProvinceAdjacency>,
) {
    if !adjacency.0.is_empty() {
        return;
    }
    let Some(map) = map else { return };

    let quantize = |v: f32| -> i32 { (v * 100.0).round() as i32 };
    let mut edge_map: std::collections::HashMap<[(i32, i32); 2], u32> =
        std::collections::HashMap::new();
    let mut pairs: Vec<[u32; 2]> = Vec::new();

    for province in &map.0.provinces {
        let pid = province.id;
        for ring in &province.boundary {
            let n = ring.len();
            for i in 0..n {
                let a = ring[i];
                let b = ring[(i + 1) % n];
                let qa = (quantize(a[0]), quantize(a[1]));
                let qb = (quantize(b[0]), quantize(b[1]));
                let key = if qa <= qb { [qa, qb] } else { [qb, qa] };
                if let Some(&other_pid) = edge_map.get(&key) {
                    if other_pid != pid {
                        pairs.push([other_pid.min(pid), other_pid.max(pid)]);
                    }
                } else {
                    edge_map.insert(key, pid);
                }
            }
        }
    }

    pairs.sort_unstable();
    pairs.dedup();
    println!("Province adjacency: {} pairs", pairs.len());
    adjacency.0 = pairs;
}

/// Rebuild the border mesh whenever the game state, mode, or zoom level changes.
fn rebuild_borders(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    adjacency: Res<ProvinceAdjacency>,
    mode: Res<MapMode>,
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    existing: Query<Entity, With<BorderMesh>>,
    mut last_tick: Local<Option<u64>>,
    mut last_mode: Local<Option<MapMode>>,
    mut last_cam_scale: Local<f32>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };

    if adjacency.0.is_empty() {
        return;
    }

    let cam_scale = camera_q.get_single().map(|p| p.scale).unwrap_or(0.1);
    let scale_changed = (*last_cam_scale - cam_scale).abs() / cam_scale.max(1e-6) > 0.15;

    let mode_changed = Some(*mode) != *last_mode;
    let tick_changed = *last_tick != Some(gs.tick);
    if !mode_changed && !tick_changed && !scale_changed {
        return;
    }
    *last_tick = Some(gs.tick);
    *last_mode = Some(*mode);
    *last_cam_scale = cam_scale;

    for e in existing.iter() {
        commands.entity(e).despawn();
    }

    if *mode != MapMode::Political {
        return;
    }

    // Border half-width: constant ~1 screen pixel at any zoom level.
    let half_w = cam_scale * 0.8;

    let province_owner: Vec<Option<&str>> = gs
        .provinces
        .iter()
        .map(|p| p.owner.as_deref())
        .collect();
    // Skip border if either province is wasteland (owned or unowned).
    let is_wasteland = |idx: usize| -> bool {
        idx < map.0.provinces.len() && map.0.provinces[idx].topography.contains("wasteland")
    };

    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for &[pid_a, pid_b] in &adjacency.0 {
        let ia = pid_a as usize;
        let ib = pid_b as usize;
        if ia >= province_owner.len() || ib >= province_owner.len() {
            continue;
        }
        if is_wasteland(ia) || is_wasteland(ib) {
            continue;
        }
        if province_owner[ia] == province_owner[ib] {
            continue;
        }

        // Get shared boundary segments in ring order, chain into polylines.
        let segs = shared_segments(&map.0.provinces[ia], &map.0.provinces[ib]);
        for chain in chain_polylines(segs) {
            polyline_to_quads(&chain, half_w, &mut positions, &mut colors, &mut indices, 0.8);
        }
    }

    if positions.is_empty() {
        return;
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

    for &x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, 0.8),
            BorderMesh,
        ));
    }
}

/// Return segments from province `a`'s rings that are shared with province `b`.
/// Returned in ring-traversal order so consecutive entries form connected chains.
fn shared_segments(
    a: &shared::map::MapProvince,
    b: &shared::map::MapProvince,
) -> Vec<[[f32; 2]; 2]> {
    let quantize = |v: f32| -> i32 { (v * 100.0).round() as i32 };
    let qpt = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    let mut b_edges: std::collections::HashSet<[(i32, i32); 2]> =
        std::collections::HashSet::new();
    for ring in &b.boundary {
        let n = ring.len();
        for i in 0..n {
            let qa = qpt(ring[i]);
            let qb = qpt(ring[(i + 1) % n]);
            let key = if qa <= qb { [qa, qb] } else { [qb, qa] };
            b_edges.insert(key);
        }
    }

    let mut result = Vec::new();
    for ring in &a.boundary {
        let n = ring.len();
        for i in 0..n {
            let p0 = ring[i];
            let p1 = ring[(i + 1) % n];
            let qa = qpt(p0);
            let qb = qpt(p1);
            let key = if qa <= qb { [qa, qb] } else { [qb, qa] };
            if b_edges.contains(&key) {
                result.push([p0, p1]);
            }
        }
    }
    result
}

/// Group ring-ordered segments into connected polyline chains.
/// A new chain starts whenever consecutive segments are not connected end-to-start.
fn chain_polylines(segments: Vec<[[f32; 2]; 2]>) -> Vec<Vec<[f32; 2]>> {
    if segments.is_empty() {
        return vec![];
    }
    let pts_eq = |a: [f32; 2], b: [f32; 2]| {
        (a[0] - b[0]).abs() < 1e-5 && (a[1] - b[1]).abs() < 1e-5
    };
    let mut chains: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = vec![segments[0][0], segments[0][1]];

    for seg in segments.iter().skip(1) {
        let last = *current.last().unwrap();
        if pts_eq(last, seg[0]) {
            current.push(seg[1]);
        } else {
            chains.push(current);
            current = vec![seg[0], seg[1]];
        }
    }
    chains.push(current);
    chains
}

/// Build a continuous quad-strip mesh for a polyline with miter joins at interior points.
/// Ported from terrain.rs (same algorithm used for rivers).
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

    let perp = |dx: f32, dy: f32| -> (f32, f32) {
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 { (0.0, 0.0) } else { (-dy / len, dx / len) }
    };

    let mut left: Vec<[f32; 2]> = Vec::with_capacity(n);
    let mut right: Vec<[f32; 2]> = Vec::with_capacity(n);

    for i in 0..n {
        let [x, y] = points[i];
        let offset = if i == 0 {
            let [x1, y1] = points[1];
            let (px, py) = perp(x1 - x, y1 - y);
            (px * half_w, py * half_w)
        } else if i == n - 1 {
            let [xp, yp] = points[n - 2];
            let (px, py) = perp(x - xp, y - yp);
            (px * half_w, py * half_w)
        } else {
            let [xp, yp] = points[i - 1];
            let [xn, yn] = points[i + 1];
            let (p0x, p0y) = perp(x - xp, y - yp);
            let (p1x, p1y) = perp(xn - x, yn - y);
            let mx = p0x + p1x;
            let my = p0y + p1y;
            let mlen = (mx * mx + my * my).sqrt();
            if mlen < 1e-9 {
                (p0x * half_w, p0y * half_w)
            } else {
                let mux = mx / mlen;
                let muy = my / mlen;
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

    let base = usize_to_u32(positions.len());
    for i in 0..n {
        positions.push([left[i][0], left[i][1], z]);
        positions.push([right[i][0], right[i][1], z]);
        colors.push(BORDER_COLOR);
        colors.push(BORDER_COLOR);
    }
    for i in 0..(n as u32 - 1) {
        let l0 = base + i * 2;
        let r0 = l0 + 1;
        let l1 = l0 + 2;
        let r1 = l0 + 3;
        indices.extend_from_slice(&[l0, r0, r1, l0, r1, l1]);
    }
}

