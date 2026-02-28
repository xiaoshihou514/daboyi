use shared::conv::*;
use shared::map::{MapData, MapProvince};
use shared::*;
use std::collections::HashMap;
use std::fs;

/// Goods each pop class needs per capita per tick.
pub fn pop_needs(class: PopClass) -> Vec<(Good, f32)> {
    match class {
        PopClass::TenantFarmer => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0005),
        ],
        PopClass::Yeoman => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0005),
            (Good::Tools, 0.0003),
        ],
        PopClass::Landlord => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.001),
        ],
        PopClass::Capitalist => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.002),
        ],
        PopClass::PetitBourgeois => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.0008),
            (Good::Tools, 0.0005),
        ],
        PopClass::Clergy => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.0005),
        ],
        PopClass::Bureaucrat => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.0005),
        ],
        PopClass::Nobility => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.003),
        ],
        PopClass::Soldier => vec![
            (Good::Grain, 0.003),
            (Good::Clothing, 0.001),
            (Good::Fuel, 0.0005),
        ],
        PopClass::Intelligentsia => vec![
            (Good::Grain, 0.002),
            (Good::Clothing, 0.001),
            (Good::Luxuries, 0.001),
        ],
    }
}

pub fn default_building_types() -> Vec<BuildingType> {
    vec![
        BuildingType {
            id: "farm".into(),
            name: "Farm".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 1000,
            input: vec![],
            output: vec![(Good::Grain, 5.0)],
        },
        BuildingType {
            id: "yeoman_farm".into(),
            name: "Yeoman Farm".into(),
            worker_class: PopClass::Yeoman,
            workers_per_level: 500,
            input: vec![(Good::Tools, 0.2)],
            output: vec![(Good::Grain, 4.0)],
        },
        BuildingType {
            id: "textile_workshop".into(),
            name: "Textile Workshop".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 200,
            input: vec![(Good::Grain, 0.5)],
            output: vec![(Good::Clothing, 2.0)],
        },
        BuildingType {
            id: "mine".into(),
            name: "Mine".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 500,
            input: vec![(Good::Tools, 0.3)],
            output: vec![(Good::Metal, 2.0)],
        },
        BuildingType {
            id: "charcoal_kiln".into(),
            name: "Charcoal Kiln".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 300,
            input: vec![],
            output: vec![(Good::Fuel, 3.0)],
        },
        BuildingType {
            id: "smithy".into(),
            name: "Smithy".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 200,
            input: vec![(Good::Metal, 1.0), (Good::Fuel, 0.5)],
            output: vec![(Good::Tools, 1.5)],
        },
        BuildingType {
            id: "luxury_workshop".into(),
            name: "Luxury Workshop".into(),
            worker_class: PopClass::PetitBourgeois,
            workers_per_level: 100,
            input: vec![(Good::Metal, 0.5), (Good::Clothing, 0.5)],
            output: vec![(Good::Luxuries, 1.0)],
        },
        BuildingType {
            id: "sawmill".into(),
            name: "Sawmill".into(),
            worker_class: PopClass::TenantFarmer,
            workers_per_level: 400,
            input: vec![(Good::Tools, 0.2)],
            output: vec![(Good::BuildingMaterials, 2.0)],
        },
    ]
}

/// Historical population data for year 1356 — now replaced by terrain-based estimation.
/// Terrain density: population per degree² based on topography × vegetation × climate.
fn terrain_density(mp: &MapProvince) -> f64 {
    // Base density by topography (people per degree² at equator-equivalent).
    let topo_base: f64 = match mp.topography.as_str() {
        "flatland" => 80_000.0,
        "hills" => 40_000.0,
        "plateau" => 25_000.0,
        "wetlands" => 20_000.0,
        "mountains" => 8_000.0,
        _ => 5_000.0,
    };

    // Vegetation multiplier.
    let veg_mult: f64 = match mp.vegetation.as_str() {
        "farmland" => 3.0,
        "grasslands" => 1.5,
        "woods" => 0.8,
        "forest" => 0.5,
        "sparse" => 0.3,
        "jungle" => 0.4,
        "desert" => 0.05,
        _ => 0.5, // NULL/empty
    };

    // Climate multiplier.
    let climate_mult: f64 = match mp.climate.as_str() {
        "subtropical" => 1.8,
        "tropical" => 1.2,
        "mediterranean" => 1.6,
        "oceanic" => 1.3,
        "continental" => 1.0,
        "arid" => 0.2,
        "cold_arid" => 0.15,
        "arctic" => 0.05,
        _ => 0.5,
    };

    (topo_base * veg_mult * climate_mult).max(50.0)
}

/// Approximate polygon area in degree² with cos(lat) correction for equirectangular coords.
fn province_area(mp: &MapProvince) -> f64 {
    if mp.boundary.is_empty() {
        return 0.0;
    }
    let ring = &mp.boundary[0];
    if ring.len() < 3 {
        return 0.0;
    }
    let mut area2 = 0.0f64;
    let n = ring.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area2 += f64::from(ring[i][0]) * f64::from(ring[j][1]);
        area2 -= f64::from(ring[j][0]) * f64::from(ring[i][1]);
    }
    // Boundary is in equirectangular (lon/lat) coords; 1° longitude is shorter at higher
    // latitudes, so correct with cos(lat) using the province centroid latitude.
    let lat_rad = f64::from(mp.centroid[1]).to_radians();
    (area2.abs() / 2.0) * lat_rad.cos()
}

/// Map EU5 raw_material string to game Good type.
fn raw_material_to_good(material: &str) -> Option<Good> {
    match material {
        // Grain
        "wheat" | "rice" | "millet" | "legumes" | "fruit" | "maize" | "potato" | "livestock" => {
            Some(Good::Grain)
        }
        // Clothing
        "cotton" | "wool" | "fiber_crops" | "silk" => Some(Good::Clothing),
        // Fuel
        "lumber" | "coal" => Some(Good::Fuel),
        // Metal
        "iron" | "copper" | "tin" | "lead" | "silver" | "goods_gold" | "mercury" | "alum"
        | "saltpeter" => Some(Good::Metal),
        // Luxuries
        "gems" | "ivory" | "saffron" | "incense" | "pepper" | "cloves" | "cocoa" | "tea"
        | "coffee" | "sugar" | "pearls" | "amber" | "dyes" | "wine" => Some(Good::Luxuries),
        // Building Materials
        "clay" | "stone" | "marble" | "sand" => Some(Good::BuildingMaterials),
        // Tools (versatile trade goods)
        "fur" | "wild_game" | "fish" | "beeswax" | "salt" | "horses" | "elephants" | "olives"
        | "tobacco" | "chili" | "medicaments" => Some(Good::Tools),
        _ => None,
    }
}

/// Medieval (1356) class distribution ratios.
/// Overwhelmingly agricultural; small merchant/artisan class.
const CLASS_RATIOS: &[(PopClass, f64)] = &[
    (PopClass::TenantFarmer, 0.50),
    (PopClass::Yeoman, 0.25),
    (PopClass::PetitBourgeois, 0.06),
    (PopClass::Soldier, 0.04),
    (PopClass::Landlord, 0.04),
    (PopClass::Clergy, 0.04),
    (PopClass::Bureaucrat, 0.03),
    (PopClass::Intelligentsia, 0.02),
    (PopClass::Nobility, 0.015),
    (PopClass::Capitalist, 0.005),
];


/// Path to the EU5-save-derived province ownership TSV (location_tag → owner_tag).
const OWNERSHIP_TSV: &str = "assets/ownership.tsv";

/// Path to the EU5 population totals file (location_name → total_population in thousands).
const POPS_TSV: &str = "assets/pops.tsv";

/// Path to Chinese province names (location_tag → chinese_name).
const PROVINCE_NAMES_TSV: &str = "assets/province_names.tsv";

/// Path to Chinese country names (country_tag → chinese_name).
const COUNTRY_NAMES_TSV: &str = "assets/country_names.tsv";

/// Load Chinese province names.
fn load_province_names() -> HashMap<String, String> {
    load_names_tsv(PROVINCE_NAMES_TSV, "province")
}

/// Load Chinese country names.
fn load_country_names() -> HashMap<String, String> {
    load_names_tsv(COUNTRY_NAMES_TSV, "country")
}

/// Generic two-column TSV loader: col0 = key, col1 = value, skip header.
fn load_names_tsv(path: &str, kind: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {path}: {e}. {kind} names will fall back to tag.");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(2, '\t');
        if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
            let key = key.trim();
            let val = val.trim();
            if !key.is_empty() && !val.is_empty() {
                map.insert(key.to_string(), val.to_string());
            }
        }
    }
    println!("Loaded {} {kind} names from {path}", map.len());
    map
}

/// Load province population data from the EU5 pops file.
/// Returns a HashMap from location_tag → population (as integer headcount, thousands × 1000).
fn load_eu5_pops() -> HashMap<String, u32> {
    let mut map = HashMap::new();
    let content = match fs::read_to_string(POPS_TSV) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {POPS_TSV}: {e}. Population will fall back to terrain estimates.");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(3, '\t');
        if let (Some(loc), Some(pop_str)) = (parts.next(), parts.next()) {
            let loc = loc.trim();
            let pop_str = pop_str.trim();
            if !loc.is_empty() {
                if let Ok(pop_thousands) = pop_str.parse::<f64>() {
                    // File units are thousands; convert to headcount, floor at 10.
                    let headcount = ((pop_thousands * 1000.0).round() as u32).max(10);
                    map.insert(loc.to_string(), headcount);
                }
            }
        }
    }
    println!("Loaded {} EU5 province populations from {}", map.len(), POPS_TSV);
    map
}

/// Load province ownership from the EU5-save-derived TSV.
/// Returns a HashMap from location_tag (e.g. "stockholm") → owner_tag (e.g. "SWE").
fn load_eu5_ownership() -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = match fs::read_to_string(OWNERSHIP_TSV) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {OWNERSHIP_TSV}: {e}. Province ownership will fall back to historical boundaries.");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(2, '\t');
        if let (Some(loc), Some(owner)) = (parts.next(), parts.next()) {
            let loc = loc.trim();
            let owner = owner.trim();
            if !loc.is_empty() && !owner.is_empty() {
                map.insert(loc.to_string(), owner.to_string());
            }
        }
    }
    println!("Loaded {} EU5 province ownerships from {}", map.len(), OWNERSHIP_TSV);
    map
}

/// Generate a full game world from EU5 map data using real population data and GIS production metadata.
pub fn generate_world(map_data: &MapData) -> GameState {
    let building_types = default_building_types();

    // Step 1: load real population data from EU5 pops file.
    let eu5_pops = load_eu5_pops();
    // Step 1b: load Chinese name tables.
    let province_names = load_province_names();
    let country_names = load_country_names();

    let province_pops: Vec<u32> = map_data
        .provinces
        .iter()
        .map(|mp| {
            if let Some(&pop) = eu5_pops.get(&mp.tag) {
                pop
            } else {
                // Terrain-based fallback for provinces missing from the pop file.
                let raw = province_area(mp) * terrain_density(mp);
                // Use the average scaling: file covers ~394M across 20k provinces.
                // Rough global scale: ~20_000 people per degree² unit.
                f64_to_u32(raw.max(10.0))
            }
        })
        .collect();

    // Step 2: load EU5 ownership exclusively.
    let eu5_ownership = load_eu5_ownership();
    let mut province_owners: Vec<Option<String>> = Vec::with_capacity(map_data.provinces.len());

    for mp in &map_data.provinces {
        province_owners.push(eu5_ownership.get(&mp.tag).cloned());
    }

    // Step 3: collect all unique country tags and create Country entities.
    let mut active_tags: HashMap<String, u32> = HashMap::new();
    for (idx, owner) in province_owners.iter().enumerate() {
        if let Some(tag) = owner {
            active_tags
                .entry(tag.clone())
                .or_insert(u32::try_from(idx).unwrap());
        }
    }

    let mut countries: Vec<Country> = active_tags
        .iter()
        .map(|(tag, &first_prov)| Country {
            name: country_names.get(tag).cloned().unwrap_or_else(|| tag.clone()),
            tag: tag.clone(),
            capital_province: first_prov,
        })
        .collect();
    countries.sort_by(|a, b| a.tag.cmp(&b.tag));

    println!("Created {} countries", countries.len());

    // Step 4: create provinces with real population + GIS raw_material production.
    let provinces: Vec<Province> = map_data
        .provinces
        .iter()
        .enumerate()
        .map(|(idx, mp)| {
            let province_pop = province_pops[idx];

            let pops: Vec<Pop> = CLASS_RATIOS
                .iter()
                .map(|(class, ratio)| {
                    let size = f64_to_u32(u32_to_f64(province_pop) * ratio);
                    Pop {
                        class: *class,
                        size: size.max(1),
                        needs_satisfaction: 1.0,
                    }
                })
                .collect();

            let farm_level = (province_pop / 1500).max(1);
            let kiln_level = (province_pop / 5000).max(1);

            // Initial stockpile: base goods + bonus from raw_material.
            let mut stockpile = HashMap::from([
                (Good::Grain, 20.0),
                (Good::Clothing, 5.0),
                (Good::Fuel, 3.0),
            ]);
            if let Some(good) = raw_material_to_good(&mp.raw_material) {
                *stockpile.entry(good).or_insert(0.0) += 15.0;
            }

            Province {
                id: mp.id,
                name: province_names.get(&mp.tag).cloned().unwrap_or_else(|| mp.name.clone()),
                owner: province_owners[idx].clone(),
                pops,
                buildings: vec![
                    Building {
                        type_id: "farm".into(),
                        level: farm_level,
                    },
                    Building {
                        type_id: "charcoal_kiln".into(),
                        level: kiln_level,
                    },
                ],
                stockpile,
            }
        })
        .collect();

    let total: u64 = provinces
        .iter()
        .map(|p| p.pops.iter().map(|pop| u64::from(pop.size)).sum::<u64>())
        .sum();
    println!("World population (1356): {total}");

    GameState {
        tick: 0,
        date: GameDate::default(),
        countries,
        provinces,
        building_types,
    }
}
