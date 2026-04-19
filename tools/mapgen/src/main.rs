mod geo;

use geo::{parse_gpb_geometry, process_polygons, process_terrain_polygon};
use rusqlite::Connection;
use shared::conv::{f64_to_f32, u64_to_f64};
use shared::map::{MapData, TerrainData};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Path to the EU5toGIS datasets directory.
const DEFAULT_GPKG_DIR: &str = "/home/xiaoshihou/Playground/github/EU5toGIS/datasets";

/// Topography values that indicate water only (go to terrain.bin, not map.bin).
fn is_water(topography: &str) -> bool {
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
    )
}

/// Topography values that indicate water or wasteland (non-playable).
fn is_non_playable(topography: &str) -> bool {
    is_water(topography) || topography.contains("wasteland")
}

/// Pre-defined RGBA color for each non-playable topography type.
fn terrain_color(topography: &str) -> [f32; 4] {
    match topography {
        "deep_ocean" => [0.027, 0.106, 0.314, 1.0], // #071B50
        "ocean" => [0.039, 0.165, 0.416, 1.0],      // #0A2A6A
        "ocean_wasteland" => [0.039, 0.165, 0.416, 1.0], // ocean color
        "coastal_ocean" => [0.051, 0.227, 0.604, 1.0], // #0D3A9A
        "inland_sea" => [0.102, 0.333, 0.722, 1.0], // #1A55B8
        "narrows" => [0.071, 0.282, 0.659, 1.0],    // #1248A8
        "lakes" | "high_lakes" => [0.157, 0.439, 0.816, 1.0], // #2870D0
        "atoll" => [0.102, 0.384, 0.753, 1.0],      // #1A62C0
        "salt_pans" => [0.847, 0.800, 0.667, 1.0],  // #D8CCAA
        "mountain_wasteland" => [0.369, 0.286, 0.224, 1.0], // #5E4939
        "dune_wasteland" => [0.788, 0.659, 0.431, 1.0], // #C9A86E
        "mesa_wasteland" => [0.608, 0.420, 0.278, 1.0], // #9B6B47
        _ if topography.contains("wasteland") => [0.545, 0.482, 0.420, 1.0], // #8B7B6B
        _ => [0.500, 0.500, 0.500, 1.0],
    }
}

/// Parse a hex color string like "#DDA910" → linear RGBA [f32;4].
/// Falls back to white if the string is invalid.
fn parse_hex_color(hex: &str) -> [f32; 4] {
    let s = hex.trim().trim_start_matches('#');
    if s.len() != 6 {
        return [1.0, 1.0, 1.0, 1.0];
    }
    let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(255);
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    ]
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
            "SELECT geom, tag, topography, vegetation, climate, raw_material, natural_harbor_suitability, hex_color \
             FROM locations \
             WHERE topography IS NOT NULL",
        )
        .expect("Failed to prepare locations query");

    let mut all_provinces = Vec::new();
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
            let hex_color_str: Option<String> = row.get(7)?;
            Ok((
                geom,
                tag,
                topography,
                vegetation.unwrap_or_default(),
                climate.unwrap_or_default(),
                raw_material.unwrap_or_default(),
                harbor.unwrap_or(0.0),
                hex_color_str.unwrap_or_default(),
            ))
        })
        .expect("Failed to query locations");

    for row in rows {
        let (geom, tag, topography, vegetation, climate, raw_material, harbor, hex_color_str) =
            row.expect("Failed to read row");
        total_read += 1;

        // Water-only features go to terrain.bin; wasteland stays in map.bin.
        if is_water(&topography) {
            skipped += 1;
            continue;
        }

        let polygons = parse_gpb_geometry(&geom);
        if polygons.is_empty() {
            skipped += 1;
            continue;
        }

        let port_sz = port_sea_zones.get(&tag).cloned();
        let hex_color = parse_hex_color(&hex_color_str);

        if let Some(province) = process_polygons(
            &polygons,
            next_id,
            &tag,
            &topography,
            &vegetation,
            &climate,
            &raw_material,
            f64_to_f32(harbor),
            hex_color,
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

    // Write map.bin.
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let map_data = MapData {
        provinces: all_provinces,
    };
    map_data
        .save(&output_path)
        .expect("Failed to write map.bin");
    let file_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Wrote {} ({:.1} MB)",
        output_path.display(),
        u64_to_f64(file_size) / 1024.0 / 1024.0
    );

    // Pass 2: build terrain.bin from non-playable features (water + wasteland).
    let terrain_path = output_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("terrain.bin");
    print!("Building terrain polygons ... ");
    let conn2 = Connection::open(&locations_path).expect("Failed to open locations.gpkg");
    let mut stmt2 = conn2
        .prepare("SELECT geom, topography FROM locations WHERE topography IS NOT NULL")
        .expect("Failed to prepare terrain query");

    let rows2 = stmt2
        .query_map([], |row| {
            let geom: Vec<u8> = row.get(0)?;
            let topography: String = row.get(1)?;
            Ok((geom, topography))
        })
        .expect("Failed to query terrain");

    let mut terrain_polygons = Vec::new();
    for row in rows2 {
        let (geom, topography) = row.expect("Failed to read terrain row");
        if !is_non_playable(&topography) {
            continue;
        }
        let polygons = parse_gpb_geometry(&geom);
        if polygons.is_empty() {
            continue;
        }
        let color = terrain_color(&topography);
        if let Some(tp) = process_terrain_polygon(&polygons, color) {
            terrain_polygons.push(tp);
        }
    }

    println!("{} terrain polygons", terrain_polygons.len());
    let terrain_data = TerrainData {
        polygons: terrain_polygons,
    };
    terrain_data
        .save(&terrain_path)
        .expect("Failed to write terrain.bin");
    let terrain_size = fs::metadata(&terrain_path).map(|m| m.len()).unwrap_or(0);
    println!(
        "Wrote {} ({:.1} MB)",
        terrain_path.display(),
        u64_to_f64(terrain_size) / 1024.0 / 1024.0
    );
}
