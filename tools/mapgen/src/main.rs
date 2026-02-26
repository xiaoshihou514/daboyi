use geo::algorithm::centroid::Centroid;
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString, Polygon};
use geojson::{Feature, GeoJson, Value};
use shared::map::{MapData, MapProvince};
use std::fs;
use std::path::PathBuf;

/// Simplification tolerance in degrees (~100m at equator).
const SIMPLIFY_EPSILON: f64 = 0.001;

/// Simple Mercator projection: lon → x, lat → y (degrees directly).
/// Good enough for 2D game rendering; not geographically accurate at poles.
fn project(lon: f64, lat: f64) -> [f32; 2] {
    [lon as f32, lat as f32]
}

fn parse_ring(coords: &[Vec<f64>]) -> Vec<[f64; 2]> {
    coords
        .iter()
        .map(|c| [c[0], c[1]])
        .collect()
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
    // Flatten all vertices into a single array for earcutr.
    let mut flat_coords: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    // Outer ring (skip closing vertex if it duplicates the first).
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
    let indices: Vec<u32> = indices.iter().map(|&i| i as u32).collect();

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

fn process_feature(feature: &Feature, province_id: u32) -> Option<MapProvince> {
    let props = feature.properties.as_ref()?;
    let geometry = feature.geometry.as_ref()?;

    // Extract GADM fields. The exact field names vary by level; try common ones.
    let gadm_id = props
        .get("GID_4").or_else(|| props.get("GID_3"))
        .or_else(|| props.get("GID_2"))
        .or_else(|| props.get("GID_1"))
        .or_else(|| props.get("GID_0"))
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN")
        .to_string();

    let name = props
        .get("NAME_4").or_else(|| props.get("NAME_3"))
        .or_else(|| props.get("NAME_2"))
        .or_else(|| props.get("NAME_1"))
        .or_else(|| props.get("NAME_0"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let country_code = props
        .get("GID_0")
        .and_then(|v| v.as_str())
        .unwrap_or("UNK")
        .to_string();

    // Parse polygon(s).
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

    // For multi-polygons, pick the largest polygon (most vertices) as the main one.
    // Merge all into a single province.
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

        // Add boundary ring (projected).
        boundary.push(outer_simplified.iter().map(|c| project(c[0], c[1])).collect());

        // Triangulate.
        let base_idx = all_vertices.len() as u32;
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
        gadm_id,
        name,
        country_code,
        boundary,
        vertices: all_vertices,
        indices: all_indices,
        centroid,
    })
}

fn main() {
    let raw_dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "raw_gadm".to_string()),
    );
    let output_path = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "assets/map.bin".to_string()),
    );

    if !raw_dir.is_dir() {
        eprintln!("Error: {} is not a directory. Run tools/download_gadm.py first.", raw_dir.display());
        std::process::exit(1);
    }

    // Collect all .json files sorted by name.
    let mut json_files: Vec<PathBuf> = fs::read_dir(&raw_dir)
        .expect("Failed to read raw_gadm dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "json"))
        .collect();
    json_files.sort();

    println!("Found {} GeoJSON files in {}", json_files.len(), raw_dir.display());

    let mut all_provinces: Vec<MapProvince> = Vec::new();
    let mut next_id: u32 = 0;

    for (file_idx, path) in json_files.iter().enumerate() {
        let filename = path.file_name().unwrap().to_string_lossy();
        print!("[{}/{}] Processing {} ... ", file_idx + 1, json_files.len(), filename);

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                println!("[error] {}", e);
                continue;
            }
        };

        let geojson: GeoJson = match content.parse() {
            Ok(g) => g,
            Err(e) => {
                println!("[error] parse: {}", e);
                continue;
            }
        };

        let features = match geojson {
            GeoJson::FeatureCollection(fc) => fc.features,
            GeoJson::Feature(f) => vec![f],
            _ => {
                println!("[skip] not a FeatureCollection");
                continue;
            }
        };

        let mut count = 0;
        for feature in &features {
            if let Some(province) = process_feature(feature, next_id) {
                all_provinces.push(province);
                next_id += 1;
                count += 1;
            }
        }

        println!("{} provinces", count);
    }

    println!("\nTotal provinces: {}", all_provinces.len());

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
        file_size as f64 / 1024.0 / 1024.0
    );
}
