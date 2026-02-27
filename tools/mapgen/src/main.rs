use geo::algorithm::centroid::Centroid;
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString, Polygon};
use geojson::{Feature, GeoJson, Value};
use shared::conv::{f64_to_f32, u64_to_f64, usize_to_u32};
use shared::map::{MapData, MapProvince};
use std::fs;
use std::path::PathBuf;

/// Simplification tolerance in degrees (~100m at equator).
const SIMPLIFY_EPSILON: f64 = 0.001;

/// Default path to the shared geojson directory.
const DEFAULT_GEOJSON_DIR: &str = "/home/xiaoshihou/Playground/shared/geojson";

fn project(lon: f64, lat: f64) -> [f32; 2] {
    [f64_to_f32(lon), f64_to_f32(lat)]
}

fn parse_ring(coords: &[Vec<f64>]) -> Vec<[f64; 2]> {
    coords.iter().map(|c| [c[0], c[1]]).collect()
}

fn coords_to_linestring(coords: &[[f64; 2]]) -> LineString<f64> {
    LineString::from(
        coords
            .iter()
            .map(|c| Coord { x: c[0], y: c[1] })
            .collect::<Vec<_>>(),
    )
}

fn simplify_ring(ring: &[[f64; 2]], epsilon: f64) -> Vec<[f64; 2]> {
    let ls = coords_to_linestring(ring);
    let simplified = ls.simplify(&epsilon);
    simplified
        .into_inner()
        .iter()
        .map(|c| [c.x, c.y])
        .collect()
}

fn triangulate_polygon(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
) -> (Vec<[f32; 2]>, Vec<u32>) {
    let mut flat_coords: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    let outer_trimmed = if outer.len() > 1 && outer.first() == outer.last() {
        &outer[..outer.len() - 1]
    } else {
        outer
    };
    for pt in outer_trimmed {
        flat_coords.push(pt[0]);
        flat_coords.push(pt[1]);
    }

    for hole in holes {
        hole_indices.push(flat_coords.len() / 2);
        let hole_trimmed = if hole.len() > 1 && hole.first() == hole.last() {
            &hole[..hole.len() - 1]
        } else {
            hole
        };
        for pt in hole_trimmed {
            flat_coords.push(pt[0]);
            flat_coords.push(pt[1]);
        }
    }

    let indices = earcutr::earcut(&flat_coords, &hole_indices, 2).unwrap_or_default();

    let vertices: Vec<[f32; 2]> = flat_coords
        .chunks(2)
        .map(|c| project(c[0], c[1]))
        .collect();
    let indices: Vec<u32> = indices.iter().map(|&i| usize_to_u32(i)).collect();

    (vertices, indices)
}

fn compute_centroid(outer: &[[f64; 2]]) -> [f32; 2] {
    let ls = coords_to_linestring(outer);
    let poly = Polygon::new(ls, vec![]);
    match poly.centroid() {
        Some(c) => project(c.x(), c.y()),
        None => {
            if let Some(first) = outer.first() {
                project(first[0], first[1])
            } else {
                [0.0, 0.0]
            }
        }
    }
}

/// Extract province metadata from a GeoJSON feature.
/// Supports two schemas:
///   - CN files: { "name": "...", "gb": "..." }
///   - World files: { "shapeName": "...", "shapeID": "...", "shapeGroup": "...", "shapeType": "..." }
struct FeatureMeta {
    id: String,
    name: String,
    country_code: String,
}

fn extract_meta(props: &serde_json::Map<String, serde_json::Value>) -> FeatureMeta {
    // CN schema
    if let Some(name) = props.get("name").and_then(|v| v.as_str()) {
        let gb = props
            .get("gb")
            .and_then(|v| v.as_str())
            .unwrap_or("UNKNOWN");
        return FeatureMeta {
            id: format!("CN_{}", gb),
            name: name.to_string(),
            country_code: "CHN".to_string(),
        };
    }

    // World schema
    let shape_name = props
        .get("shapeName")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let shape_id = props
        .get("shapeID")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");
    let shape_group = props
        .get("shapeGroup")
        .and_then(|v| v.as_str())
        .unwrap_or("UNK");

    FeatureMeta {
        id: shape_id.to_string(),
        name: shape_name.to_string(),
        country_code: shape_group.to_string(),
    }
}

fn process_feature(feature: &Feature, province_id: u32) -> Option<MapProvince> {
    let props = feature.properties.as_ref()?;
    let geometry = feature.geometry.as_ref()?;

    let meta = extract_meta(props);

    let polygons: Vec<Vec<Vec<[f64; 2]>>> = match &geometry.value {
        Value::Polygon(rings) => {
            vec![rings.iter().map(|r| parse_ring(r)).collect()]
        }
        Value::MultiPolygon(multi) => multi
            .iter()
            .map(|rings| rings.iter().map(|r| parse_ring(r)).collect())
            .collect(),
        _ => return None,
    };

    let mut all_vertices: Vec<[f32; 2]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut boundary: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut best_outer: Option<Vec<[f64; 2]>> = None;
    let mut best_len = 0;

    for poly_rings in &polygons {
        if poly_rings.is_empty() {
            continue;
        }

        let outer_simplified = simplify_ring(&poly_rings[0], SIMPLIFY_EPSILON);
        let holes_simplified: Vec<Vec<[f64; 2]>> = poly_rings[1..]
            .iter()
            .map(|h| simplify_ring(h, SIMPLIFY_EPSILON))
            .collect();

        if outer_simplified.len() > best_len {
            best_len = outer_simplified.len();
            best_outer = Some(outer_simplified.clone());
        }

        boundary.push(
            outer_simplified
                .iter()
                .map(|c| project(c[0], c[1]))
                .collect(),
        );

        let base_idx = usize_to_u32(all_vertices.len());
        let (verts, idxs) = triangulate_polygon(&outer_simplified, &holes_simplified);
        all_vertices.extend(verts);
        all_indices.extend(idxs.iter().map(|i| i + base_idx));
    }

    if all_vertices.is_empty() {
        return None;
    }

    let centroid = match best_outer {
        Some(ref outer) => compute_centroid(outer),
        None => [0.0, 0.0],
    };

    Some(MapProvince {
        id: province_id,
        gadm_id: meta.id,
        name: meta.name,
        country_code: meta.country_code,
        boundary,
        vertices: all_vertices,
        indices: all_indices,
        centroid,
    })
}

fn load_geojson(path: &PathBuf) -> Vec<Feature> {
    let content = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("  [error] reading {}: {}", path.display(), e);
        String::new()
    });
    if content.is_empty() {
        return vec![];
    }
    let geojson: GeoJson = match content.parse() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("  [error] parsing {}: {}", path.display(), e);
            return vec![];
        }
    };
    match geojson {
        GeoJson::FeatureCollection(fc) => fc.features,
        GeoJson::Feature(f) => vec![f],
        _ => vec![],
    }
}

fn main() {
    let geojson_dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| DEFAULT_GEOJSON_DIR.to_string()),
    );
    let output_path = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "assets/map.bin".to_string()),
    );

    if !geojson_dir.is_dir() {
        eprintln!(
            "Error: {} is not a directory.\nExpected prepackaged geojson at {}",
            geojson_dir.display(),
            DEFAULT_GEOJSON_DIR
        );
        std::process::exit(1);
    }

    let cn_path = geojson_dir.join("cn_adm3.geojson");
    let world_path = geojson_dir.join("world_adm2.geojson");

    if !cn_path.exists() || !world_path.exists() {
        eprintln!(
            "Error: expected cn_adm3.geojson and world_adm2.geojson in {}",
            geojson_dir.display()
        );
        std::process::exit(1);
    }

    let mut all_provinces: Vec<MapProvince> = Vec::new();
    let mut next_id: u32 = 0;

    // China ADM3 (prioritized, higher detail)
    print!("Processing cn_adm3.geojson ... ");
    let cn_features = load_geojson(&cn_path);
    let mut cn_count = 0;
    for feature in &cn_features {
        if let Some(province) = process_feature(feature, next_id) {
            all_provinces.push(province);
            next_id += 1;
            cn_count += 1;
        }
    }
    println!("{} provinces", cn_count);

    // World ADM2 (excluding China and disputed overlaps)
    print!("Processing world_adm2.geojson ... ");
    let world_features = load_geojson(&world_path);
    let mut world_count = 0;
    // IND shapeIDs that overlap with CN data (disputed territories)
    const DISPUTED_IND_SHAPE_IDS: &[&str] = &[
        "76128533B18668117085772", // Tawang (overlaps CN 错那/隆子 — 藏南)
        "76128533B57183548666997", // Leh(Ladakh) (includes Aksai Chin)
    ];
    for feature in &world_features {
        if let Some(props) = &feature.properties {
            let group = props
                .get("shapeGroup")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            // Skip China (already in cn_adm3) and Taiwan (covered by CN data)
            if group == "CHN" || group == "TWN" {
                continue;
            }
            // Skip disputed IND districts that overlap with CN boundaries
            if group == "IND" {
                if let Some(sid) = props.get("shapeID").and_then(|v| v.as_str()) {
                    if DISPUTED_IND_SHAPE_IDS.contains(&sid) {
                        continue;
                    }
                }
            }
        }
        if let Some(province) = process_feature(feature, next_id) {
            all_provinces.push(province);
            next_id += 1;
            world_count += 1;
        }
    }
    println!("{} provinces", world_count);

    println!(
        "\nTotal provinces: {} (China: {}, World: {})",
        all_provinces.len(),
        cn_count,
        world_count
    );

    // Write output.
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).ok();
    }

    let map_data = MapData {
        provinces: all_provinces,
    };
    map_data.save(&output_path).expect("Failed to write map.bin");

    let file_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Wrote {} ({:.1} MB)",
        output_path.display(),
        u64_to_f64(file_size) / 1024.0 / 1024.0
    );
}
