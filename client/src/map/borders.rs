use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::{f32_to_i32, u32_to_f32, u32_to_usize, usize_to_u32};
use std::collections::HashMap;

use crate::editor::{AdminMap, CountryMap};
use crate::map::{BorderVersion, MapMode, MapResource, MAP_WIDTH};
use crate::state::AppState;

const BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
/// Target border half-width in screen pixels (constant visual size regardless of zoom).
const BORDER_HALF_PIXELS: f32 = 1.5;

#[derive(Clone)]
pub(crate) struct CachedBorder {
    provinces: [u32; 2],
    chains: Vec<Vec<[f32; 2]>>,
}

/// Precomputed border chains keyed by adjacent province pairs.
#[derive(Resource, Default)]
pub struct ProvinceAdjacency(pub Vec<CachedBorder>);

/// Marker for the border mesh entities.
#[derive(Component)]
pub struct BorderMesh;

#[derive(Resource, Default)]
struct BorderAssets {
    mesh: Option<Handle<Mesh>>,
    material: Option<Handle<ColorMaterial>>,
}

#[derive(Resource, Default)]
struct BorderScratch {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OwnerKey {
    Country(u32),
    Admin(u32),
}

pub struct BordersPlugin;

impl Plugin for BordersPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProvinceAdjacency::default())
            .insert_resource(BorderAssets::default())
            .insert_resource(BorderScratch::default())
            .add_systems(
                Update,
                (compute_adjacency, rebuild_borders)
                    .chain()
                    .run_if(in_state(AppState::Editing)),
            );
    }
}

/// Compute province adjacency and shared border chains once from province boundaries.
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

    // Pass 1: compute merged chains for every adjacent pair.
    let mut pair_data: Vec<([u32; 2], Vec<Vec<[f32; 2]>>)> = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let ia = u32_to_usize(pair[0]);
        let ib = u32_to_usize(pair[1]);
        if ia >= map.0.provinces.len() || ib >= map.0.provinces.len() {
            continue;
        }
        let raw_chains =
            chain_polylines(shared_segments(&map.0.provinces[ia], &map.0.provinces[ib]));
        if raw_chains.is_empty() {
            continue;
        }
        let merged = merge_chains(raw_chains);
        pair_data.push((pair, merged));
    }

    // Pass 2: globally weld chain endpoints within 0.05° to their centroid.
    // This fixes junction gaps at T/X intersections where GIS data stores
    // slightly different float values for the same geographic junction vertex
    // depending on which neighbor is on the other side.
    weld_endpoints_global(&mut pair_data);

    // Pass 3: apply Chaikin smoothing and build CachedBorder entries.
    let mut cached_borders = Vec::with_capacity(pair_data.len());
    for (pair, chains) in pair_data {
        let smoothed: Vec<Vec<[f32; 2]>> = chains
            .iter()
            .map(|c| chaikin_smooth(&chaikin_smooth(c)))
            .collect();
        cached_borders.push(CachedBorder {
            provinces: pair,
            chains: smoothed,
        });
    }

    println!("Province adjacency: {} pairs", cached_borders.len());
    adjacency.0 = cached_borders;
}

/// Rebuild the border mesh whenever political ownership semantics change.
fn rebuild_borders(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    border_version: Res<BorderVersion>,
    country_map: Res<CountryMap>,
    admin_assignments: Res<AdminMap>,
    map: Option<Res<MapResource>>,
    adjacency: Res<ProvinceAdjacency>,
    mode: Res<MapMode>,
    camera_q: Query<&OrthographicProjection, With<Camera2d>>,
    existing: Query<Entity, With<BorderMesh>>,
    mut border_assets: ResMut<BorderAssets>,
    mut scratch: ResMut<BorderScratch>,
    mut last_mode: Local<Option<MapMode>>,
    mut last_border_version: Local<u64>,
    mut last_scale: Local<f32>,
) {
    let Some(map) = map else { return };

    if adjacency.0.is_empty() {
        return;
    }

    // Camera projection scale (world units per pixel).
    let proj_scale = camera_q.get_single().map(|p| p.scale).unwrap_or(0.05);
    let scale_changed = *last_scale == 0.0
        || ((*last_scale - proj_scale).abs() / *last_scale > 0.05);

    let mode_changed = Some(*mode) != *last_mode;
    let border_changed = *last_border_version != border_version.0;
    if !mode_changed && !border_changed && !scale_changed {
        return;
    }
    *last_mode = Some(*mode);
    *last_border_version = border_version.0;
    *last_scale = proj_scale;

    // Early-return for non-political mode BEFORE touching the mesh, so
    // recycle_mesh_buffers never strips it and leaves zero vertices for the GPU allocator.
    if *mode != MapMode::Political {
        despawn_border_entities(&mut commands, &existing);
        return;
    }

    if let Some(mesh_handle) = border_assets.mesh.clone() {
        if let Some(mesh) = meshes.get_mut(&mesh_handle) {
            recycle_mesh_buffers(mesh, &mut scratch);
        } else {
            scratch.positions.clear();
            scratch.colors.clear();
            scratch.indices.clear();
        }
    } else {
        scratch.positions.clear();
        scratch.colors.clear();
        scratch.indices.clear();
    }

    let mut country_keys: HashMap<&str, u32> = HashMap::new();
    let mut next_country_key = 0_u32;
    for tag in country_map.0.values() {
        country_keys.entry(tag.as_str()).or_insert_with(|| {
            let key = next_country_key;
            next_country_key = next_country_key.saturating_add(1);
            key
        });
    }

    let is_wasteland = |idx: usize| -> bool {
        idx < map.0.provinces.len() && map.0.provinces[idx].topography.contains("wasteland")
    };
    let province_owner = |idx: usize| -> Option<OwnerKey> {
        if idx < map.0.provinces.len() {
            let prov_id = map.0.provinces[idx].id;
            if let Some(&area_id) = admin_assignments.0.get(&prov_id) {
                return Some(OwnerKey::Admin(area_id));
            }
            return country_map
                .0
                .get(&prov_id)
                .and_then(|tag| country_keys.get(tag.as_str()).copied())
                .map(OwnerKey::Country);
        } else {
            None
        }
    };

    let BorderScratch {
        positions,
        colors,
        indices,
    } = &mut *scratch;

    for border in &adjacency.0 {
        let ia = u32_to_usize(border.provinces[0]);
        let ib = u32_to_usize(border.provinces[1]);
        if is_wasteland(ia) || is_wasteland(ib) {
            continue;
        }
        if province_owner(ia) == province_owner(ib) {
            continue;
        }

        for chain in &border.chains {
            let hw = BORDER_HALF_PIXELS * proj_scale;
            polyline_to_quads(chain, hw, positions, colors, indices, 0.8);
            // Disc caps seal junction voids where 3+ borders converge.
            // Radius = 2 * hw covers the uncovered corner at 90° junctions
            // (corner is at hw * √2 ≈ 1.414 * hw from center; 2 * hw gives margin).
            let disc_r = hw * 2.0;
            add_endpoint_disc(chain[0], disc_r, positions, colors, indices, 0.8);
            add_endpoint_disc(*chain.last().unwrap(), disc_r, positions, colors, indices, 0.8);
        }
    }

    if positions.is_empty() {
        despawn_border_entities(&mut commands, &existing);
        // Restore minimal valid geometry so the GPU allocator doesn't divide by zero
        // when it processes this mesh handle (entities are still alive this frame
        // until deferred despawns are flushed).
        if let Some(mesh_handle) = border_assets.mesh.clone() {
            if let Some(mesh) = meshes.get_mut(&mesh_handle) {
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    vec![[0.0f32, 0.0, 0.0]; 3],
                );
                mesh.insert_attribute(
                    Mesh::ATTRIBUTE_COLOR,
                    vec![[0.0f32, 0.0, 0.0, 0.0]; 3],
                );
                mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
            }
        }
        return;
    }    let mesh_handle = border_assets.mesh.get_or_insert_with(|| {
        meshes.add(Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        ))
    }).clone();
    let material_handle = border_assets
        .material
        .get_or_insert_with(|| materials.add(ColorMaterial::default()))
        .clone();

    if let Some(mesh) = meshes.get_mut(&mesh_handle) {
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            std::mem::take(positions),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, std::mem::take(colors));
        mesh.insert_indices(Indices::U32(std::mem::take(indices)));
    }

    if existing.iter().next().is_none() {
        for &x_off in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
            commands.spawn((
                Mesh2d(mesh_handle.clone()),
                MeshMaterial2d(material_handle.clone()),
                Transform::from_xyz(x_off, 0.0, 0.8),
                BorderMesh,
            ));
        }
    }
}

fn despawn_border_entities(commands: &mut Commands, existing: &Query<Entity, With<BorderMesh>>) {
    for entity in existing.iter() {
        commands.entity(entity).despawn();
    }
}

fn recycle_mesh_buffers(mesh: &mut Mesh, scratch: &mut BorderScratch) {
    scratch.positions = match mesh.remove_attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(values)) => values,
        _ => Vec::new(),
    };
    scratch.colors = match mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(values)) => values,
        _ => Vec::new(),
    };
    scratch.indices = match mesh.remove_indices() {
        Some(Indices::U32(values)) => values,
        _ => Vec::new(),
    };
    scratch.positions.clear();
    scratch.colors.clear();
    scratch.indices.clear();
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

/// Apply one iteration of Chaikin's curve subdivision algorithm to smooth a polyline.
/// Preserves the endpoints; interior points are replaced by pairs at 1/4 and 3/4 of each segment.
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

/// Greedily join chains that share an endpoint (using the same quantization as the rest of
/// the border code) so that disconnected sub-chains of the same border pair become one
/// continuous polyline. Without this, Chaikin smoothing widens each sub-chain independently
/// and leaves visible gaps at junction points.
fn merge_chains(mut chains: Vec<Vec<[f32; 2]>>) -> Vec<Vec<[f32; 2]>> {
    // Use 0.1° (≈11 km) precision so small GIS axis-aligned endpoint gaps are bridged.
    let quantize = |v: f32| -> i32 { f32_to_i32((v * 10.0).round()) };
    let qpt = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    'restart: loop {
        let n = chains.len();
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let qi_first = qpt(chains[i][0]);
                let qi_last = qpt(*chains[i].last().unwrap());
                let qj_first = qpt(chains[j][0]);
                let qj_last = qpt(*chains[j].last().unwrap());

                // i-end → j-start: append j[1..] to i
                if qi_last == qj_first {
                    let tail = chains[j][1..].to_vec();
                    chains[i].extend(tail);
                    chains.remove(j);
                    continue 'restart;
                }
                // i-end → j-end: append reverse(j)[1..] to i
                if qi_last == qj_last {
                    let mut j_rev = chains[j].clone();
                    j_rev.reverse();
                    chains[i].extend_from_slice(&j_rev[1..]);
                    chains.remove(j);
                    continue 'restart;
                }
                // j-end → i-start: prepend j[..j.len()-1] before i
                if qi_first == qj_last {
                    let prefix_len = chains[j].len() - 1;
                    let prefix = chains[j][..prefix_len].to_vec();
                    let tail = std::mem::take(&mut chains[i]);
                    let mut new_chain = prefix;
                    new_chain.extend(tail);
                    chains[i] = new_chain;
                    chains.remove(j);
                    continue 'restart;
                }
                // j-start → i-start (reversed j ends at j-first = i-first):
                // prepend reverse(j) before i[1..]
                if qi_first == qj_first {
                    let mut j_rev = chains[j].clone();
                    j_rev.reverse();
                    let tail = std::mem::take(&mut chains[i]);
                    j_rev.extend_from_slice(&tail[1..]);
                    chains[i] = j_rev;
                    chains.remove(j);
                    continue 'restart;
                }
            }
        }
        break;
    }
    chains
}

/// Snap all chain endpoints within 0.1° of each other to their centroid.
/// Called on pre-Chaikin chains so Chaikin starts from the exact welded positions.
fn weld_endpoints_global(pair_data: &mut Vec<([u32; 2], Vec<Vec<[f32; 2]>>)>) {
    // 0.1° buckets: any two endpoints within ~0.05° of each other share a bucket.
    // Using the same resolution as merge_chains so GIS precision mismatches are
    // reliably merged across province-pair boundaries.
    let quantize = |v: f32| -> i32 { f32_to_i32((v * 10.0).round()) };
    let qpt = |p: [f32; 2]| -> (i32, i32) { (quantize(p[0]), quantize(p[1])) };

    let mut bucket_sum: HashMap<(i32, i32), ([f32; 2], u32)> = HashMap::new();
    for (_, chains) in pair_data.iter() {
        for chain in chains {
            let n = chain.len();
            if n < 2 {
                continue;
            }
            for &pt in &[chain[0], chain[n - 1]] {
                let q = qpt(pt);
                let e = bucket_sum.entry(q).or_insert(([0.0, 0.0], 0));
                e.0[0] += pt[0];
                e.0[1] += pt[1];
                e.1 += 1;
            }
        }
    }
    let centroids: HashMap<(i32, i32), [f32; 2]> = bucket_sum
        .into_iter()
        .map(|(k, (sum, n))| (k, [sum[0] / u32_to_f32(n), sum[1] / u32_to_f32(n)]))
        .collect();

    for (_, chains) in pair_data.iter_mut() {
        for chain in chains.iter_mut() {
            let n = chain.len();
            if n < 2 {
                continue;
            }
            if let Some(&c) = centroids.get(&qpt(chain[0])) {
                chain[0] = c;
            }
            if let Some(&c) = centroids.get(&qpt(chain[n - 1])) {
                chain[n - 1] = c;
            }
        }
    }
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

/// Add a filled 8-segment disc at `center` with radius `r`. Used as endpoint caps so that
/// junction vertices between different province-pair chains are visually sealed.
fn add_endpoint_disc(
    center: [f32; 2],
    r: f32,
    positions: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    z: f32,
) {
    const SEGS: u32 = 8;
    let cx = center[0];
    let cy = center[1];
    let base = usize_to_u32(positions.len());

    // Centre vertex
    positions.push([cx, cy, z]);
    colors.push(BORDER_COLOR);

    for k in 0..SEGS {
        let angle = std::f32::consts::TAU * u32_to_f32(k) / u32_to_f32(SEGS);
        positions.push([cx + r * angle.cos(), cy + r * angle.sin(), z]);
        colors.push(BORDER_COLOR);
    }

    // Fan triangles: centre + rim[k] + rim[(k+1) % SEGS]
    for k in 0..SEGS {
        indices.push(base); // centre
        indices.push(base + 1 + k);
        indices.push(base + 1 + (k + 1) % SEGS);
    }
}
