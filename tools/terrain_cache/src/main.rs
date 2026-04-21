use serde::{Deserialize, Serialize};
use shared::conv::{f32_to_i32, u32_to_usize, usize_to_u32};
use shared::map::{MapData, TerrainData, TerrainPolygon};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

const DEFAULT_MAP_PATH: &str = "assets/map.bin";
const DEFAULT_TERRAIN_PATH: &str = "assets/terrain.bin";
const DEFAULT_OUTPUT_PATH: &str = "assets/terrain_adjacency.bin";
const TERRAIN_ADJACENCY_CACHE_VERSION: u32 = 3;

#[derive(Clone, Default, Serialize, Deserialize)]
struct TerrainPolygonAdjacency {
    adjacent_provinces: Vec<u32>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TerrainProvinceBorder {
    terrain_index: u32,
    province_id: u32,
    chains: Vec<Vec<[f32; 2]>>,
}

#[derive(Serialize, Deserialize)]
struct TerrainAdjacencyCache {
    version: u32,
    province_count: u32,
    terrain_polygon_count: u32,
    polygons: Vec<TerrainPolygonAdjacency>,
    borders: Vec<TerrainProvinceBorder>,
    component_ids: Vec<u32>,
    water_polygons: Vec<bool>,
}

fn main() -> io::Result<()> {
    let map_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| DEFAULT_MAP_PATH.to_string()),
    );
    let terrain_path = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| DEFAULT_TERRAIN_PATH.to_string()),
    );
    let output_path = PathBuf::from(
        std::env::args()
            .nth(3)
            .unwrap_or_else(|| DEFAULT_OUTPUT_PATH.to_string()),
    );

    let map = MapData::load(&map_path)?;
    let terrain = TerrainData::load(&terrain_path)?;

    let province_boundaries: Vec<(u32, Vec<Vec<[f32; 2]>>)> = map
        .provinces
        .iter()
        .map(|province| (province.id, province.boundary.clone()))
        .collect();
    let terrain_boundaries: Vec<Vec<[[f32; 2]; 2]>> = terrain
        .polygons
        .iter()
        .map(terrain_polygon_boundary_segments)
        .collect();
    let terrain_is_water: Vec<bool> = terrain
        .polygons
        .iter()
        .map(|polygon| {
            polygon.color == [0.027, 0.106, 0.314, 1.0]
                || polygon.color == [0.039, 0.165, 0.416, 1.0]
                || polygon.color == [0.051, 0.227, 0.604, 1.0]
                || polygon.color == [0.102, 0.333, 0.722, 1.0]
                || polygon.color == [0.071, 0.282, 0.659, 1.0]
                || polygon.color == [0.157, 0.439, 0.816, 1.0]
                || polygon.color == [0.102, 0.384, 0.753, 1.0]
                || polygon.color == [0.847, 0.800, 0.667, 1.0]
        })
        .collect();

    let (polygons, borders, component_ids, water_polygons) =
        build_terrain_adjacency(&province_boundaries, &terrain_boundaries, &terrain_is_water);
    let cache = TerrainAdjacencyCache {
        version: TERRAIN_ADJACENCY_CACHE_VERSION,
        province_count: usize_to_u32(map.provinces.len()),
        terrain_polygon_count: usize_to_u32(terrain.polygons.len()),
        polygons,
        borders,
        component_ids,
        water_polygons,
    };

    let bytes = bincode::serialize(&cache)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output_path, bytes)?;
    println!(
        "Wrote {} ({} polygons, {} borders)",
        output_path.display(),
        cache.polygons.len(),
        cache.borders.len()
    );
    Ok(())
}

fn build_terrain_adjacency(
    province_boundaries: &[(u32, Vec<Vec<[f32; 2]>>)],
    terrain_boundaries: &[Vec<[[f32; 2]; 2]>],
    terrain_is_water: &[bool],
) -> (
    Vec<TerrainPolygonAdjacency>,
    Vec<TerrainProvinceBorder>,
    Vec<u32>,
    Vec<bool>,
) {
    let mut province_edges: HashMap<[(i32, i32); 2], Vec<u32>> = HashMap::new();
    for (province_id, rings) in province_boundaries {
        for ring in rings {
            let ring_len = ring.len();
            for index in 0..ring_len {
                let segment = [ring[index], ring[(index + 1) % ring_len]];
                province_edges
                    .entry(quantized_segment_key(segment))
                    .or_default()
                    .push(*province_id);
            }
        }
    }

    let mut terrain_pair_segments: HashMap<(u32, u32), Vec<[[f32; 2]; 2]>> = HashMap::new();
    let mut terrain_edges: HashMap<[(i32, i32); 2], Vec<u32>> = HashMap::new();
    for (terrain_index, segments) in terrain_boundaries.iter().enumerate() {
        let terrain_index = usize_to_u32(terrain_index);
        for &segment in segments {
            terrain_edges
                .entry(quantized_segment_key(segment))
                .or_default()
                .push(terrain_index);
            if let Some(province_ids) = province_edges.get(&quantized_segment_key(segment)) {
                for province_id in province_ids {
                    terrain_pair_segments
                        .entry((terrain_index, *province_id))
                        .or_default()
                        .push(segment);
                }
            }
        }
    }

    let mut polygons = vec![TerrainPolygonAdjacency::default(); terrain_boundaries.len()];
    let mut borders = Vec::new();
    for ((terrain_index, province_id), segments) in terrain_pair_segments {
        let chains = merge_unordered_segments(segments);
        if chains.is_empty() {
            continue;
        }
        polygons[u32_to_usize(terrain_index)]
            .adjacent_provinces
            .push(province_id);
        borders.push(TerrainProvinceBorder {
            terrain_index,
            province_id,
            chains,
        });
    }

    for polygon in &mut polygons {
        polygon.adjacent_provinces.sort_unstable();
        polygon.adjacent_provinces.dedup();
    }

    (
        polygons,
        borders,
        terrain_component_ids(terrain_boundaries.len(), terrain_edges, terrain_is_water),
        terrain_is_water.to_vec(),
    )
}

fn terrain_component_ids(
    polygon_count: usize,
    terrain_edges: HashMap<[(i32, i32); 2], Vec<u32>>,
    terrain_is_water: &[bool],
) -> Vec<u32> {
    let mut adjacency: Vec<Vec<u32>> = vec![Vec::new(); polygon_count];
    for polygons in terrain_edges.into_values() {
        if polygons.len() < 2 {
            continue;
        }
        for left_index in 0..polygons.len() {
            for right_index in left_index + 1..polygons.len() {
                let left = polygons[left_index];
                let right = polygons[right_index];
                if terrain_is_water[u32_to_usize(left)] != terrain_is_water[u32_to_usize(right)] {
                    continue;
                }
                adjacency[u32_to_usize(left)].push(right);
                adjacency[u32_to_usize(right)].push(left);
            }
        }
    }

    let mut component_ids = vec![u32::MAX; polygon_count];
    let mut next_component = 0_u32;
    for polygon_index in 0..polygon_count {
        if component_ids[polygon_index] != u32::MAX {
            continue;
        }
        let mut stack = vec![usize_to_u32(polygon_index)];
        component_ids[polygon_index] = next_component;
        while let Some(current) = stack.pop() {
            for &neighbor in &adjacency[u32_to_usize(current)] {
                let neighbor_index = u32_to_usize(neighbor);
                if component_ids[neighbor_index] != u32::MAX {
                    continue;
                }
                component_ids[neighbor_index] = next_component;
                stack.push(neighbor);
            }
        }
        next_component = next_component.saturating_add(1);
    }
    component_ids
}

fn terrain_polygon_boundary_segments(poly: &TerrainPolygon) -> Vec<[[f32; 2]; 2]> {
    let mut edge_counts: HashMap<(u32, u32), u32> = HashMap::new();
    let mut edge_points: HashMap<(u32, u32), [[f32; 2]; 2]> = HashMap::new();

    for triangle in poly.indices.chunks_exact(3) {
        for &(start, end) in &[
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            *edge_counts.entry(key).or_insert(0) += 1;
            edge_points.entry(key).or_insert([
                poly.vertices[u32_to_usize(start)],
                poly.vertices[u32_to_usize(end)],
            ]);
        }
    }

    edge_counts
        .into_iter()
        .filter_map(|(key, count)| {
            if count == 1 {
                edge_points.get(&key).copied()
            } else {
                None
            }
        })
        .collect()
}

fn quantized_segment_key(segment: [[f32; 2]; 2]) -> [(i32, i32); 2] {
    let start = quantized_point(segment[0]);
    let end = quantized_point(segment[1]);
    if start <= end {
        [start, end]
    } else {
        [end, start]
    }
}

fn quantized_point(point: [f32; 2]) -> (i32, i32) {
    (
        f32_to_i32((point[0] * 100.0).round()),
        f32_to_i32((point[1] * 100.0).round()),
    )
}

fn merge_unordered_segments(segments: Vec<[[f32; 2]; 2]>) -> Vec<Vec<[f32; 2]>> {
    let mut chains: Vec<Vec<[f32; 2]>> = segments
        .into_iter()
        .map(|segment| vec![segment[0], segment[1]])
        .collect();

    'restart: loop {
        let chain_count = chains.len();
        for left_index in 0..chain_count {
            for right_index in 0..chain_count {
                if left_index == right_index {
                    continue;
                }
                let left_first = quantized_point(chains[left_index][0]);
                let left_last = quantized_point(*chains[left_index].last().unwrap());
                let right_first = quantized_point(chains[right_index][0]);
                let right_last = quantized_point(*chains[right_index].last().unwrap());

                if left_last == right_first {
                    let tail = chains[right_index][1..].to_vec();
                    chains[left_index].extend(tail);
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_last == right_last {
                    let mut reversed = chains[right_index].clone();
                    reversed.reverse();
                    chains[left_index].extend_from_slice(&reversed[1..]);
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_first == right_last {
                    let prefix = chains[right_index][..chains[right_index].len() - 1].to_vec();
                    let tail = std::mem::take(&mut chains[left_index]);
                    let mut new_chain = prefix;
                    new_chain.extend(tail);
                    chains[left_index] = new_chain;
                    chains.remove(right_index);
                    continue 'restart;
                }
                if left_first == right_first {
                    let mut reversed = chains[right_index].clone();
                    reversed.reverse();
                    let tail = std::mem::take(&mut chains[left_index]);
                    reversed.extend_from_slice(&tail[1..]);
                    chains[left_index] = reversed;
                    chains.remove(right_index);
                    continue 'restart;
                }
            }
        }
        break;
    }

    chains
}
