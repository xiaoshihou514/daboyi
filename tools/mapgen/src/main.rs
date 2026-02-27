use geo::algorithm::centroid::Centroid;
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString, Polygon};
use rusqlite::Connection;
use shared::conv::{f64_to_f32, u64_to_f64, usize_to_u32};
use shared::map::{MapData, MapProvince};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Simplification tolerance in degrees (~100m at equator).
const SIMPLIFY_EPSILON: f64 = 0.001;

/// Path to the EU5toGIS datasets directory.
const DEFAULT_GPKG_DIR: &str = "/home/xiaoshihou/Playground/github/EU5toGIS/datasets";

/// WGS84 semi-major axis.
const R: f64 = 6_378_137.0;
/// cos(45°)
const COS45: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Inverse Gall Stereographic projection: projected (x, y) → WGS84 (lon, lat) in degrees.
fn gall_stereo_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let lon_rad = x / (R * COS45);
    let lat_rad = 2.0 * (y / (R * (1.0 + COS45))).atan();
    (lon_rad.to_degrees(), lat_rad.to_degrees())
}

/// Project lon/lat to screen coordinates (equirectangular: identity mapping).
fn project(lon: f64, lat: f64) -> [f32; 2] {
    [f64_to_f32(lon), f64_to_f32(lat)]
}

/// Unwrap longitude jumps >180° within a ring to prevent giant antimeridian triangles.
fn normalize_ring_longitudes(ring: &mut Vec<[f64; 2]>) {
    for i in 1..ring.len() {
        let diff = ring[i][0] - ring[i - 1][0];
        if diff > 180.0 {
            ring[i][0] -= 360.0;
        } else if diff < -180.0 {
            ring[i][0] += 360.0;
        }
    }
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
    // Triangulate in the projected (equirectangular = lon/lat) space so that
    // earcut and the rendered triangles are consistent.
    let mut flat_proj: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    let outer_trimmed = if outer.len() > 1 && outer.first() == outer.last() {
        &outer[..outer.len() - 1]
    } else {
        outer
    };
    for pt in outer_trimmed {
        flat_proj.push(pt[0]);
        flat_proj.push(pt[1]);
    }

    for hole in holes {
        hole_indices.push(flat_proj.len() / 2);
        let hole_trimmed = if hole.len() > 1 && hole.first() == hole.last() {
            &hole[..hole.len() - 1]
        } else {
            hole
        };
        for pt in hole_trimmed {
            flat_proj.push(pt[0]);
            flat_proj.push(pt[1]);
        }
    }

    let indices = earcutr::earcut(&flat_proj, &hole_indices, 2).unwrap_or_default();

    let vertices: Vec<[f32; 2]> = flat_proj
        .chunks(2)
        .map(|c| [f64_to_f32(c[0]), f64_to_f32(c[1])])
        .collect();
    let indices: Vec<u32> = indices.iter().map(|&i| usize_to_u32(i)).collect();

    (vertices, indices)
}

/// Compute centroid in WGS84 lon/lat degrees (NOT projected).
fn compute_centroid(outer: &[[f64; 2]]) -> [f32; 2] {
    let ls = coords_to_linestring(outer);
    let poly = Polygon::new(ls, vec![]);
    match poly.centroid() {
        Some(c) => [f64_to_f32(c.x()), f64_to_f32(c.y())],
        None => {
            if let Some(first) = outer.first() {
                [f64_to_f32(first[0]), f64_to_f32(first[1])]
            } else {
                [0.0, 0.0]
            }
        }
    }
}

/// Topography values that indicate water or wasteland (non-playable).
fn is_non_playable(topography: &str) -> bool {
    matches!(
        topography,
        "coastal_ocean"
            | "ocean"
            | "inland_sea"
            | "deep_ocean"
            | "narrows"
            | "lakes"
            | "high_lakes"
            | "atoll"
            | "salt_pans"
    ) || topography.contains("wasteland")
}

/// Parse GeoPackage Binary (GPB) geometry → list of WGS84 polygon rings.
/// Returns Vec<Vec<Vec<[f64;2]>>>: polygons → rings → points as [lon, lat].
fn parse_gpb_geometry(geom: &[u8]) -> Vec<Vec<Vec<[f64; 2]>>> {
    let mut result = Vec::new();
    if geom.len() < 8 {
        return result;
    }

    // GP header: "GP" magic (2), version (1), flags (1), srs_id (4)
    if geom[0] != b'G' || geom[1] != b'P' {
        return result;
    }
    let flags = geom[3];
    let byte_order_flag = flags & 0x01; // 0 = big-endian, 1 = little-endian
    let envelope_type = (flags >> 1) & 0x07;

    let envelope_size: usize = match envelope_type {
        0 => 0,
        1 => 32, // 4 doubles (minx, maxx, miny, maxy)
        2 => 48, // 6 doubles
        3 => 48, // 6 doubles
        4 => 64, // 8 doubles
        _ => return result,
    };

    let wkb_start = 8 + envelope_size;
    if wkb_start >= geom.len() {
        return result;
    }

    let wkb = &geom[wkb_start..];
    parse_wkb_multipolygon(wkb, byte_order_flag, &mut result);
    result
}

fn read_u32_wkb(data: &[u8], pos: usize, le: bool) -> u32 {
    let bytes: [u8; 4] = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
    if le {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    }
}

fn read_f64_wkb(data: &[u8], pos: usize, le: bool) -> f64 {
    let bytes: [u8; 8] = [
        data[pos],
        data[pos + 1],
        data[pos + 2],
        data[pos + 3],
        data[pos + 4],
        data[pos + 5],
        data[pos + 6],
        data[pos + 7],
    ];
    if le {
        f64::from_le_bytes(bytes)
    } else {
        f64::from_be_bytes(bytes)
    }
}

fn parse_wkb_multipolygon(wkb: &[u8], _header_bo: u8, result: &mut Vec<Vec<Vec<[f64; 2]>>>) {
    if wkb.len() < 5 {
        return;
    }
    let le = wkb[0] == 1;
    let wkb_type = read_u32_wkb(wkb, 1, le);

    if wkb_type == 6 {
        // MultiPolygon
        let n_polys = read_u32_wkb(wkb, 5, le);
        let mut pos: usize = 9;
        for _ in 0..n_polys {
            if pos + 5 > wkb.len() {
                break;
            }
            let poly_le = wkb[pos] == 1;
            pos += 5; // skip byte_order + type (3 = Polygon)
            if pos + 4 > wkb.len() {
                break;
            }
            let n_rings = read_u32_wkb(wkb, pos, poly_le);
            pos += 4;

            let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
            for _ in 0..n_rings {
                if pos + 4 > wkb.len() {
                    break;
                }
                let n_pts = read_u32_wkb(wkb, pos, poly_le);
                pos += 4;
                let n_pts_usize = u32_to_usize(n_pts);
                let mut points: Vec<[f64; 2]> = Vec::with_capacity(n_pts_usize);
                for _ in 0..n_pts {
                    if pos + 16 > wkb.len() {
                        break;
                    }
                    let x = read_f64_wkb(wkb, pos, poly_le);
                    let y = read_f64_wkb(wkb, pos + 8, poly_le);
                    pos += 16;
                    let (lon, lat) = gall_stereo_to_wgs84(x, y);
                    points.push([lon, lat]);
                }
                normalize_ring_longitudes(&mut points);
                rings.push(points);
            }
            result.push(rings);
        }
    } else if wkb_type == 3 {
        // Single Polygon
        let n_rings = read_u32_wkb(wkb, 5, le);
        let mut pos: usize = 9;
        let mut rings: Vec<Vec<[f64; 2]>> = Vec::new();
        for _ in 0..n_rings {
            if pos + 4 > wkb.len() {
                break;
            }
            let n_pts = read_u32_wkb(wkb, pos, le);
            pos += 4;
            let n_pts_usize = u32_to_usize(n_pts);
            let mut points: Vec<[f64; 2]> = Vec::with_capacity(n_pts_usize);
            for _ in 0..n_pts {
                if pos + 16 > wkb.len() {
                    break;
                }
                let x = read_f64_wkb(wkb, pos, le);
                let y = read_f64_wkb(wkb, pos + 8, le);
                pos += 16;
                let (lon, lat) = gall_stereo_to_wgs84(x, y);
                points.push([lon, lat]);
            }
            normalize_ring_longitudes(&mut points);
            rings.push(points);
        }
        result.push(rings);
    }
}

fn u32_to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap()
}

/// Process parsed polygon rings into a MapProvince.
fn process_polygons(
    polygons: &[Vec<Vec<[f64; 2]>>],
    province_id: u32,
    tag: &str,
    topography: &str,
    vegetation: &str,
    climate: &str,
    raw_material: &str,
    harbor_suitability: f32,
    port_sea_zone: Option<String>,
) -> Option<MapProvince> {
    let mut all_vertices: Vec<[f32; 2]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    let mut boundary: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut best_outer: Option<Vec<[f64; 2]>> = None;
    let mut best_len = 0;

    for poly_rings in polygons {
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
        tag: tag.to_string(),
        name: tag.to_string(),
        topography: topography.to_string(),
        vegetation: vegetation.to_string(),
        climate: climate.to_string(),
        raw_material: raw_material.to_string(),
        harbor_suitability,
        port_sea_zone,
        boundary,
        vertices: all_vertices,
        indices: all_indices,
        centroid,
    })
}

fn main() {
    let gpkg_dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| DEFAULT_GPKG_DIR.to_string()),
    );
    let output_path = PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "assets/map.bin".to_string()),
    );

    let locations_path = gpkg_dir.join("locations.gpkg");
    let ports_path = gpkg_dir.join("ports.gpkg");

    if !locations_path.exists() {
        eprintln!(
            "Error: {} not found.\nExpected EU5toGIS datasets at {}",
            locations_path.display(),
            DEFAULT_GPKG_DIR
        );
        std::process::exit(1);
    }

    // Load port → sea zone mapping from ports.gpkg.
    let mut port_sea_zones: HashMap<String, String> = HashMap::new();
    if ports_path.exists() {
        print!("Loading ports.gpkg ... ");
        let conn = Connection::open(&ports_path).expect("Failed to open ports.gpkg");
        let mut stmt = conn
            .prepare("SELECT tag, SeaZone FROM ports")
            .expect("Failed to query ports");
        let rows = stmt
            .query_map([], |row| {
                let tag: String = row.get(0)?;
                let sea_zone: String = row.get(1)?;
                Ok((tag, sea_zone))
            })
            .expect("Failed to iterate ports");
        for row in rows {
            if let Ok((tag, sz)) = row {
                port_sea_zones.insert(tag, sz);
            }
        }
        println!("{} ports loaded", port_sea_zones.len());
    } else {
        println!("Warning: ports.gpkg not found, skipping port data");
    }

    // Read locations.gpkg and build provinces.
    print!("Reading locations.gpkg ... ");
    let conn = Connection::open(&locations_path).expect("Failed to open locations.gpkg");
    let mut stmt = conn
        .prepare(
            "SELECT geom, tag, topography, vegetation, climate, raw_material, natural_harbor_suitability \
             FROM locations \
             WHERE topography IS NOT NULL",
        )
        .expect("Failed to prepare locations query");

    let mut all_provinces: Vec<MapProvince> = Vec::new();
    let mut next_id: u32 = 0;
    let mut skipped = 0u32;
    let mut total_read = 0u32;

    let rows = stmt
        .query_map([], |row| {
            let geom: Vec<u8> = row.get(0)?;
            let tag: String = row.get(1)?;
            let topography: String = row.get(2)?;
            let vegetation: Option<String> = row.get(3)?;
            let climate: Option<String> = row.get(4)?;
            let raw_material: Option<String> = row.get(5)?;
            let harbor: Option<f64> = row.get(6)?;
            Ok((
                geom,
                tag,
                topography,
                vegetation.unwrap_or_default(),
                climate.unwrap_or_default(),
                raw_material.unwrap_or_default(),
                harbor.unwrap_or(0.0),
            ))
        })
        .expect("Failed to query locations");

    for row in rows {
        let (geom, tag, topography, vegetation, climate, raw_material, harbor) =
            row.expect("Failed to read row");
        total_read += 1;

        // Filter non-playable (water + wasteland).
        if is_non_playable(&topography) {
            skipped += 1;
            continue;
        }

        let polygons = parse_gpb_geometry(&geom);
        if polygons.is_empty() {
            skipped += 1;
            continue;
        }

        let port_sz = port_sea_zones.get(&tag).cloned();

        if let Some(province) = process_polygons(
            &polygons,
            next_id,
            &tag,
            &topography,
            &vegetation,
            &climate,
            &raw_material,
            f64_to_f32(harbor),
            port_sz,
        ) {
            all_provinces.push(province);
            next_id += 1;
        } else {
            skipped += 1;
        }
    }
    println!(
        "{} playable provinces (read {}, skipped {})",
        all_provinces.len(),
        total_read,
        skipped
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
