use geo::algorithm::area::Area;
use geo::algorithm::bool_ops::BooleanOps;
use geo::algorithm::centroid::Centroid;
use geo::{Coord, LineString, MultiPolygon, Polygon, Rect};
use shared::conv::{f64_to_f32, usize_to_u32};
use shared::map::MapProvince;

/// Area threshold in degree² above which a province is subdivided.
pub const SUBDIVIDE_THRESHOLD: f64 = 1.0;

/// Compute signed area of a ring in degree² (shoelace formula, no lat correction).
fn ring_area_deg2(ring: &[[f64; 2]]) -> f64 {
    if ring.len() < 3 {
        return 0.0;
    }
    let mut area2 = 0.0;
    let n = ring.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area2 += ring[i][0] * ring[j][1];
        area2 -= ring[j][0] * ring[i][1];
    }
    area2.abs() / 2.0
}

/// Convert a MapProvince's boundary (f32) back to f64 rings for geo operations.
fn boundary_to_polygon(boundary: &[Vec<[f32; 2]>]) -> Option<MultiPolygon<f64>> {
    if boundary.is_empty() {
        return None;
    }

    let mut polygons = Vec::new();

    // Each boundary ring may represent a separate polygon part (multipolygon)
    // or the first is outer + rest are holes.
    // In our mapgen, each element of boundary is an outer ring of a polygon part.
    for ring in boundary {
        if ring.len() < 3 {
            continue;
        }
        let coords: Vec<Coord<f64>> = ring
            .iter()
            .map(|p| Coord {
                x: f64::from(p[0]),
                y: f64::from(p[1]),
            })
            .collect();
        let ls = LineString::new(coords);
        polygons.push(Polygon::new(ls, vec![]));
    }

    if polygons.is_empty() {
        None
    } else {
        Some(MultiPolygon::new(polygons))
    }
}

/// Convert a geo Polygon back to our f32 boundary + triangulated mesh format.
fn polygon_to_province_data(
    poly: &Polygon<f64>,
) -> (Vec<Vec<[f32; 2]>>, Vec<[f32; 2]>, Vec<u32>, [f32; 2]) {
    let outer: Vec<[f32; 2]> = poly
        .exterior()
        .coords()
        .map(|c| [f64_to_f32(c.x), f64_to_f32(c.y)])
        .collect();

    let holes_f32: Vec<Vec<[f32; 2]>> = poly
        .interiors()
        .iter()
        .map(|h| {
            h.coords()
                .map(|c| [f64_to_f32(c.x), f64_to_f32(c.y)])
                .collect()
        })
        .collect();

    let boundary = {
        let mut b = vec![outer.clone()];
        b.extend(holes_f32.clone());
        b
    };

    // Triangulate
    let mut flat_coords: Vec<f64> = Vec::new();
    let mut hole_indices: Vec<usize> = Vec::new();

    // Outer ring (skip closing point if duplicated)
    let outer_f64: Vec<[f64; 2]> = poly
        .exterior()
        .coords()
        .map(|c| [c.x, c.y])
        .collect();
    let outer_trimmed = if outer_f64.len() > 1 && outer_f64.first() == outer_f64.last() {
        &outer_f64[..outer_f64.len() - 1]
    } else {
        &outer_f64
    };
    for pt in outer_trimmed {
        flat_coords.push(pt[0]);
        flat_coords.push(pt[1]);
    }

    for interior in poly.interiors() {
        hole_indices.push(flat_coords.len() / 2);
        let hole_f64: Vec<[f64; 2]> = interior.coords().map(|c| [c.x, c.y]).collect();
        let hole_trimmed = if hole_f64.len() > 1 && hole_f64.first() == hole_f64.last() {
            &hole_f64[..hole_f64.len() - 1]
        } else {
            &hole_f64
        };
        for pt in hole_trimmed {
            flat_coords.push(pt[0]);
            flat_coords.push(pt[1]);
        }
    }

    let indices_raw = earcutr::earcut(&flat_coords, &hole_indices, 2).unwrap_or_default();
    let vertices: Vec<[f32; 2]> = flat_coords
        .chunks(2)
        .map(|c| [f64_to_f32(c[0]), f64_to_f32(c[1])])
        .collect();
    let indices: Vec<u32> = indices_raw.iter().map(|&i| usize_to_u32(i)).collect();

    let centroid = poly
        .centroid()
        .map(|c| [f64_to_f32(c.x()), f64_to_f32(c.y())])
        .unwrap_or_else(|| outer.first().copied().unwrap_or([0.0, 0.0]));

    (boundary, vertices, indices, centroid)
}

/// Subdivide a province if its area exceeds the threshold.
/// Returns a Vec of sub-provinces (or the original province if no subdivision needed).
/// The `base_id` is the starting ID for new sub-provinces.
pub fn subdivide_province(mp: &MapProvince, base_id: u32) -> Vec<MapProvince> {
    // Compute raw area in deg² from boundary.
    let area = if !mp.boundary.is_empty() && mp.boundary[0].len() >= 3 {
        let ring: Vec<[f64; 2]> = mp.boundary[0]
            .iter()
            .map(|p| [f64::from(p[0]), f64::from(p[1])])
            .collect();
        ring_area_deg2(&ring)
    } else {
        0.0
    };

    if area <= SUBDIVIDE_THRESHOLD {
        // No subdivision needed — return with updated ID.
        let mut clone = mp.clone();
        clone.id = base_id;
        return vec![clone];
    }

    let Some(multi) = boundary_to_polygon(&mp.boundary) else {
        let mut clone = mp.clone();
        clone.id = base_id;
        return vec![clone];
    };

    // Compute bounding box of the multipolygon.
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for poly in multi.iter() {
        for coord in poly.exterior().coords() {
            min_x = min_x.min(coord.x);
            max_x = max_x.max(coord.x);
            min_y = min_y.min(coord.y);
            max_y = max_y.max(coord.y);
        }
    }

    let width = max_x - min_x;
    let height = max_y - min_y;

    // Determine grid dimensions.
    let n_cells = (area / SUBDIVIDE_THRESHOLD).ceil().max(2.0);
    let aspect = if height > 0.0 { width / height } else { 1.0 };
    let cols = (n_cells.sqrt() * aspect.sqrt()).ceil().max(2.0) as usize;
    let rows = (n_cells / cols as f64).ceil().max(1.0) as usize;

    let cell_w = width / cols as f64;
    let cell_h = height / rows as f64;

    let mut results = Vec::new();
    let mut next_id = base_id;

    for row in 0..rows {
        for col in 0..cols {
            let x0 = min_x + col as f64 * cell_w;
            let y0 = min_y + row as f64 * cell_h;
            // Add small epsilon overlap to avoid gaps.
            let x1 = if col == cols - 1 {
                max_x + 0.001
            } else {
                x0 + cell_w + 0.0001
            };
            let y1 = if row == rows - 1 {
                max_y + 0.001
            } else {
                y0 + cell_h + 0.0001
            };

            let cell = Rect::new(Coord { x: x0, y: y0 }, Coord { x: x1, y: y1 })
                .to_polygon();

            // Clip the province polygon to this grid cell.
            let clipped = multi.intersection(&MultiPolygon::new(vec![cell]));

            if clipped.0.is_empty() {
                continue;
            }

            // Check if the clipped result has meaningful area.
            let clipped_area = clipped.unsigned_area();
            if clipped_area < 1e-8 {
                continue;
            }

            // Merge all polygons in the clipped result into one MapProvince.
            let mut all_boundary: Vec<Vec<[f32; 2]>> = Vec::new();
            let mut all_vertices: Vec<[f32; 2]> = Vec::new();
            let mut all_indices: Vec<u32> = Vec::new();

            let mut best_area = 0.0;
            let mut centroid = [0.0f32, 0.0f32];

            for poly in clipped.iter() {
                let (bnd, verts, idxs, cent) = polygon_to_province_data(poly);
                if verts.is_empty() {
                    continue;
                }

                let base_idx = usize_to_u32(all_vertices.len());
                all_boundary.extend(bnd);
                all_vertices.extend(verts);
                all_indices.extend(idxs.iter().map(|&i| i + base_idx));

                let a = poly.unsigned_area();
                if a > best_area {
                    best_area = a;
                    centroid = cent;
                }
            }

            if all_vertices.is_empty() {
                continue;
            }

            let name = if rows * cols > 1 {
                format!("{} {}-{}", mp.name, row, col)
            } else {
                mp.name.clone()
            };

            results.push(MapProvince {
                id: next_id,
                gadm_id: format!("{}_{}-{}", mp.gadm_id, row, col),
                name,
                country_code: mp.country_code.clone(),
                boundary: all_boundary,
                vertices: all_vertices,
                indices: all_indices,
                centroid,
            });
            next_id += 1;
        }
    }

    if results.is_empty() {
        // Fallback: return original province.
        let mut clone = mp.clone();
        clone.id = base_id;
        vec![clone]
    } else {
        results
    }
}
