/// Geometry utilities for EU5 GeoPackage → game map conversion.
///
/// Covers:
/// - Gall Stereographic → WGS84 inverse projection
/// - GPB / WKB polygon parsing
/// - Ring normalization (antimeridian unwrapping)
/// - Polygon simplification and triangulation
/// - Centroid computation
use geo::algorithm::centroid::Centroid;
use geo::algorithm::simplify::Simplify;
use geo::{Coord, LineString, Polygon};
use shared::map::{MapProvince, TerrainPolygon};

/// WGS84 semi-major axis.
const R: f64 = 6_378_137.0;
/// cos(45°)
const COS45: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Simplification tolerance in degrees (~100m at equator).
pub const SIMPLIFY_EPSILON: f64 = 0.001;

/// Inverse Gall Stereographic projection: projected (x, y) → WGS84 (lon, lat) in degrees.
pub fn gall_stereo_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let lon_rad = x / (R * COS45);
    let lat_rad = 2.0 * (y / (R * (1.0 + COS45))).atan();
    (lon_rad.to_degrees(), lat_rad.to_degrees())
}

/// Project lon/lat to screen coordinates (equirectangular: identity mapping).
pub fn project(lon: f64, lat: f64) -> [f32; 2] {
    [lon as f32, lat as f32]
}

/// Unwrap longitude jumps >180° within a ring to prevent giant antimeridian triangles.
pub fn normalize_ring_longitudes(ring: &mut Vec<[f64; 2]>) {
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

pub fn simplify_ring(ring: &[[f64; 2]], epsilon: f64) -> Vec<[f64; 2]> {
    let ls = coords_to_linestring(ring);
    let simplified = ls.simplify(&epsilon);
    simplified.into_inner().iter().map(|c| [c.x, c.y]).collect()
}

pub fn triangulate_polygon(
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
) -> (Vec<[f32; 2]>, Vec<u32>) {
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
        .map(|c| [c[0] as f32, c[1] as f32])
        .collect();
    let indices: Vec<u32> = indices.iter().map(|&i| i as u32).collect();

    (vertices, indices)
}

/// Compute centroid in WGS84 lon/lat degrees (NOT projected).
pub fn compute_centroid(outer: &[[f64; 2]]) -> [f32; 2] {
    let ls = coords_to_linestring(outer);
    let poly = Polygon::new(ls, vec![]);
    match poly.centroid() {
        Some(c) => [c.x() as f32, c.y() as f32],
        None => {
            if let Some(first) = outer.first() {
                [first[0] as f32, first[1] as f32]
            } else {
                [0.0, 0.0]
            }
        }
    }
}

// ── WKB parsing ───────────────────────────────────────────────────────────────

pub fn read_u32_wkb(data: &[u8], pos: usize, le: bool) -> u32 {
    let bytes: [u8; 4] = [data[pos], data[pos + 1], data[pos + 2], data[pos + 3]];
    if le {
        u32::from_le_bytes(bytes)
    } else {
        u32::from_be_bytes(bytes)
    }
}

pub fn read_f64_wkb(data: &[u8], pos: usize, le: bool) -> f64 {
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

pub fn parse_wkb_multipolygon(wkb: &[u8], _header_bo: u8, result: &mut Vec<Vec<Vec<[f64; 2]>>>) {
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
                let n_pts_usize = usize::try_from(n_pts).unwrap();
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
            let n_pts_usize = usize::try_from(n_pts).unwrap();
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

/// Returns Vec<Vec<Vec<[f64;2]>>>: polygons → rings → points as [lon, lat].
pub fn parse_gpb_geometry(geom: &[u8]) -> Vec<Vec<Vec<[f64; 2]>>> {
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

// ── High-level polygon builders ───────────────────────────────────────────────

/// Process parsed polygon rings into a MapProvince.
#[allow(clippy::too_many_arguments)]
pub fn process_polygons(
    polygons: &[Vec<Vec<[f64; 2]>>],
    province_id: u32,
    tag: &str,
    topography: &str,
    vegetation: &str,
    climate: &str,
    raw_material: &str,
    harbor_suitability: f32,
    hex_color: [f32; 4],
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
        tag: tag.to_string(),
        name: tag.to_string(),
        topography: topography.to_string(),
        vegetation: vegetation.to_string(),
        climate: climate.to_string(),
        raw_material: raw_material.to_string(),
        harbor_suitability,
        hex_color,
        port_sea_zone,
        boundary,
        vertices: all_vertices,
        indices: all_indices,
        centroid,
    })
}

/// Triangulate a non-playable terrain polygon and return a TerrainPolygon.
pub fn process_terrain_polygon(
    polygons: &[Vec<Vec<[f64; 2]>>],
    color: [f32; 4],
) -> Option<TerrainPolygon> {
    let mut all_vertices: Vec<[f32; 2]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();

    for poly_rings in polygons {
        if poly_rings.is_empty() {
            continue;
        }
        let outer_simplified = simplify_ring(&poly_rings[0], SIMPLIFY_EPSILON);
        let holes_simplified: Vec<Vec<[f64; 2]>> = poly_rings[1..]
            .iter()
            .map(|h| simplify_ring(h, SIMPLIFY_EPSILON))
            .collect();
        let base_idx = all_vertices.len() as u32;
        let (verts, idxs) = triangulate_polygon(&outer_simplified, &holes_simplified);
        all_vertices.extend(verts);
        all_indices.extend(idxs.iter().map(|i| i + base_idx));
    }

    if all_vertices.is_empty() {
        return None;
    }
    Some(TerrainPolygon {
        color,
        vertices: all_vertices,
        indices: all_indices,
    })
}
