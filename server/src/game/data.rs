use crate::game::load::{load_country_names, load_eu5_ownership, load_eu5_pops, load_province_names, load_vassals};
use crate::game::params::{
    climate_multiplier, topo_density, veg_multiplier, CLASS_RATIOS, FARM_POP_PER_LEVEL,
    INIT_CLOTHING, INIT_FUEL, INIT_GRAIN, KILN_POP_PER_LEVEL, MIN_PROVINCE_POP,
    RAW_MATERIAL_BONUS,
};
use shared::conv::*;
use shared::map::{MapData, MapProvince};
use shared::*;
use std::collections::HashMap;

pub use crate::game::params::pop_needs;

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

/// Terrain-based population estimate for provinces missing from the pops file.
fn terrain_density(mp: &MapProvince) -> f64 {
    let base = topo_density(&mp.topography);
    let veg = veg_multiplier(&mp.vegetation);
    let cli = climate_multiplier(&mp.climate);
    (base * veg * cli).max(MIN_PROVINCE_POP)
}

/// Approximate polygon area in degree² with cos(lat) correction.
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
    let lat_rad = f64::from(mp.centroid[1]).to_radians();
    (area2.abs() / 2.0) * lat_rad.cos()
}

/// Map EU5 raw_material string to game Good type.
fn raw_material_to_good(material: &str) -> Option<Good> {
    match material {
        "wheat" | "rice" | "millet" | "legumes" | "fruit" | "maize" | "potato" | "livestock" => {
            Some(Good::Grain)
        }
        "cotton" | "wool" | "fiber_crops" | "silk" => Some(Good::Clothing),
        "lumber" | "coal" => Some(Good::Fuel),
        "iron" | "copper" | "tin" | "lead" | "silver" | "goods_gold" | "mercury" | "alum"
        | "saltpeter" => Some(Good::Metal),
        "gems" | "ivory" | "saffron" | "incense" | "pepper" | "cloves" | "cocoa" | "tea"
        | "coffee" | "sugar" | "pearls" | "amber" | "dyes" | "wine" => Some(Good::Luxuries),
        "clay" | "stone" | "marble" | "sand" => Some(Good::BuildingMaterials),
        "fur" | "wild_game" | "fish" | "beeswax" | "salt" | "horses" | "elephants" | "olives"
        | "tobacco" | "chili" | "medicaments" => Some(Good::Tools),
        _ => None,
    }
}

/// Generate a full game world from EU5 map data.
pub fn generate_world(map_data: &MapData) -> GameState {
    let building_types = default_building_types();

    let eu5_pops = load_eu5_pops();
    let province_names = load_province_names();
    let country_names = load_country_names();

    let province_pops: Vec<u32> = map_data
        .provinces
        .iter()
        .map(|mp| {
            if let Some(&pop) = eu5_pops.get(&mp.tag) {
                pop
            } else {
                f64_to_u32((province_area(mp) * terrain_density(mp)).max(MIN_PROVINCE_POP))
            }
        })
        .collect();

    let eu5_ownership = load_eu5_ownership();
    let province_owners: Vec<Option<String>> = map_data
        .provinces
        .iter()
        .map(|mp| eu5_ownership.get(&mp.tag).cloned())
        .collect();

    // Collect unique country tags → create Country entities.
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

    let provinces: Vec<Province> = map_data
        .provinces
        .iter()
        .enumerate()
        .map(|(idx, mp)| {
            let province_pop = province_pops[idx];
            let pops = CLASS_RATIOS
                .iter()
                .map(|(class, ratio)| Pop {
                    class: *class,
                    size: f64_to_u32(u32_to_f64(province_pop) * ratio).max(1),
                    needs_satisfaction: 1.0,
                })
                .collect();

            let farm_level = (province_pop / FARM_POP_PER_LEVEL).max(1);
            let kiln_level = (province_pop / KILN_POP_PER_LEVEL).max(1);

            let mut stockpile = HashMap::from([
                (Good::Grain, INIT_GRAIN),
                (Good::Clothing, INIT_CLOTHING),
                (Good::Fuel, INIT_FUEL),
            ]);
            if let Some(good) = raw_material_to_good(&mp.raw_material) {
                *stockpile.entry(good).or_insert(0.0) += RAW_MATERIAL_BONUS;
            }

            Province {
                id: mp.id,
                name: province_names.get(&mp.tag).cloned().unwrap_or_else(|| mp.name.clone()),
                owner: province_owners[idx].clone(),
                pops,
                buildings: vec![
                    Building { type_id: "farm".into(), level: farm_level },
                    Building { type_id: "charcoal_kiln".into(), level: kiln_level },
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

    let vassals = load_vassals();
    println!("Vassal relationships: {}", vassals.len());

    GameState {
        tick: 0,
        date: GameDate::default(),
        countries,
        provinces,
        building_types,
        vassals,
    }
}
