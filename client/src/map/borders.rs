use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::usize_to_u32;

use crate::map::{MapMode, MapResource, MAP_WIDTH};
use crate::net::LatestGameState;
use crate::state::AppState;

/// Border line half-width in world-space degrees.
const BORDER_HALF_W: f32 = 0.10;
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
    // Run only once (after adjacency is populated it's non-empty).
    if !adjacency.0.is_empty() {
        return;
    }
    let Some(map) = map else { return };

    // Quantize f32 coords to i32 grid at 0.01° resolution.
    let quantize = |v: f32| -> i32 { (v * 100.0).round() as i32 };

    // Map from quantized edge (sorted point pair) → province ID.
    // An edge is a segment between two consecutive boundary points.
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
                // Canonical key: sort endpoints so (a→b) == (b→a).
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

    // Deduplicate.
    pairs.sort_unstable();
    pairs.dedup();

    println!("Province adjacency: {} pairs", pairs.len());
    adjacency.0 = pairs;
}

/// Rebuild the border mesh whenever the game state changes or mode changes.
fn rebuild_borders(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    adjacency: Res<ProvinceAdjacency>,
    mode: Res<MapMode>,
    existing: Query<Entity, With<BorderMesh>>,
    mut last_tick: Local<Option<u64>>,
    mut last_mode: Local<Option<MapMode>>,
) {
    let Some(map) = map else { return };
    let Some(gs) = &state.0 else { return };

    // Wait until adjacency is computed.
    if adjacency.0.is_empty() {
        return;
    }

    let mode_changed = Some(*mode) != *last_mode;
    let tick_changed = *last_tick != Some(gs.tick);
    if !mode_changed && !tick_changed {
        return;
    }
    *last_tick = Some(gs.tick);
    *last_mode = Some(*mode);

    // Despawn old border meshes.
    for e in existing.iter() {
        commands.entity(e).despawn();
    }

    // Only show borders in political mode.
    if *mode != MapMode::Political {
        return;
    }

    // Find owner of each province (by index) and whether it's owned wasteland.
    let province_owner: Vec<Option<&str>> = gs
        .provinces
        .iter()
        .map(|p| p.owner.as_deref())
        .collect();
    let is_owned_wasteland = |idx: usize| -> bool {
        if idx >= map.0.provinces.len() { return false; }
        let is_wasteland = map.0.provinces[idx].topography.contains("wasteland");
        let is_owned = idx < gs.provinces.len() && gs.provinces[idx].owner.is_some();
        is_wasteland && is_owned
    };

    // Build border segments: shared edges where adjacent provinces have different owners.
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for &[pid_a, pid_b] in &adjacency.0 {
        let ia = pid_a as usize;
        let ib = pid_b as usize;
        if ia >= province_owner.len() || ib >= province_owner.len() {
            continue;
        }
        // Skip borders involving owned wasteland provinces.
        if is_owned_wasteland(ia) || is_owned_wasteland(ib) {
            continue;
        }
        let owner_a = province_owner[ia];
        let owner_b = province_owner[ib];
        // Draw border if owners differ (including one being None).
        if owner_a == owner_b {
            continue;
        }

        // Find the shared boundary segments between province a and b.
        let shared = shared_segments(&map.0.provinces[ia], &map.0.provinces[ib]);
        for seg in shared {
            emit_quad(&seg, BORDER_HALF_W, &mut positions, &mut colors, &mut indices, 0.8);
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

/// Find boundary segments shared between two provinces (within ε tolerance).
fn shared_segments(
    a: &shared::map::MapProvince,
    b: &shared::map::MapProvince,
) -> Vec<[[f32; 2]; 2]> {
    let quantize = |v: f32| -> i32 { (v * 100.0).round() as i32 };
    let qpoint = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    // Collect all edges from province b as a set of canonical (sorted) point pairs.
    let mut b_edges: std::collections::HashSet<[(i32, i32); 2]> =
        std::collections::HashSet::new();
    for ring in &b.boundary {
        let n = ring.len();
        for i in 0..n {
            let qa = qpoint(ring[i]);
            let qb = qpoint(ring[(i + 1) % n]);
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
            let qa = qpoint(p0);
            let qb = qpoint(p1);
            let key = if qa <= qb { [qa, qb] } else { [qb, qa] };
            if b_edges.contains(&key) {
                result.push([p0, p1]);
            }
        }
    }
    result
}

/// Emit a thick quad for a border segment [p0, p1] with `half_w` width.
fn emit_quad(
    seg: &[[f32; 2]; 2],
    half_w: f32,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    z: f32,
) {
    let [p0, p1] = *seg;
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return;
    }
    // Perpendicular unit vector.
    let nx = -dy / len * half_w;
    let ny = dx / len * half_w;

    let base = usize_to_u32(positions.len());
    positions.push([p0[0] - nx, p0[1] - ny, z]);
    positions.push([p0[0] + nx, p0[1] + ny, z]);
    positions.push([p1[0] + nx, p1[1] + ny, z]);
    positions.push([p1[0] - nx, p1[1] - ny, z]);
    for _ in 0..4 {
        colors.push(BORDER_COLOR);
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}
