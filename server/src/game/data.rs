use shared::conv::*;
use shared::map::{MapData, MapProvince};
use shared::*;
use std::collections::HashMap;

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

/// Historical population data for year 1444, interpolated from HYDE 3.3 (via OWID).
/// Source: OWID indicator 953903 (HYDE 3.3 + Gapminder + UN WPP).
/// Values interpolated between 1400 and 1500 data points: p1400 + (p1500 - p1400) * 44/100.
/// Key: ISO-3166 alpha-3 code (matches shapeGroup in world GeoJSON).
/// Note: TWN population is folded into CHN since Taiwan uses CN ADM3 data.
fn pop_1444_data() -> HashMap<&'static str, u32> {
    HashMap::from([
        ("ABW", 180),
        ("AFG", 1930000),
        ("AGO", 1867440),
        ("AIA", 821),
        ("ALB", 200000),
        ("AND", 1631),
        ("ANT", 1395),
        ("ARE", 31000),
        ("ARG", 288800),
        ("ARM", 193360),
        ("ASM", 650),
        ("ATG", 6700),
        ("AUS", 316668),
        ("AUT", 1580000),
        ("AZE", 412920),
        ("BDI", 612144),
        ("BEL", 998000),
        ("BEN", 2832000),
        ("BFA", 1295139),
        ("BGD", 9277440),
        ("BGR", 660000),
        ("BHR", 20434),
        ("BHS", 4037),
        ("BIH", 351040),
        ("BLR", 495360),
        ("BLZ", 6563),
        ("BOL", 866400),
        ("BRA", 17350133),
        ("BRB", 2900),
        ("BRN", 4448),
        ("BTN", 13320),
        ("BWA", 83334),
        ("CAF", 274098),
        ("CAN", 188800),
        ("CHE", 688000),
        ("CHL", 577600),
        // CHN includes TWN (187076) folded in since Taiwan uses CN ADM3 data.
        ("CHN", 80243364),
        ("CIV", 958280),
        ("CMR", 1281447),
        ("COD", 5475840),
        ("COG", 244423),
        ("COL", 3816000),
        ("COM", 18880),
        ("CRI", 429000),
        ("CUB", 56694),
        ("CYP", 172000),
        ("CZE", 1958400),
        ("DEU", 7600000),
        ("DJI", 20390),
        ("DMA", 1209),
        ("DNK", 488000),
        ("DOM", 177600),
        ("DZA", 1483200),
        ("ECU", 577600),
        ("EGY", 4028000),
        ("ERI", 108420),
        ("ESH", 259),
        ("ESP", 5940000),
        ("EST", 60848),
        ("ETH", 2355000),
        ("FIN", 100000),
        ("FJI", 48048),
        ("FLK", 2),
        ("FRA", 12663053),
        ("FRO", 161),
        ("GAB", 137734),
        ("GBR", 3476636),
        ("GEO", 504160),
        ("GHA", 1595121),
        ("GIN", 990740),
        ("GLP", 45807),
        ("GMB", 3260),
        ("GNB", 77278),
        ("GNQ", 103107),
        ("GRC", 944000),
        ("GRD", 17256),
        ("GRL", 1148),
        ("GTM", 777600),
        ("GUF", 1464),
        ("GUY", 84400),
        ("HKG", 13015),
        ("HND", 969219),
        ("HRV", 526560),
        ("HTI", 162666),
        ("HUN", 1110000),
        ("IDN", 6980000),
        ("IND", 129143200),
        ("IRN", 3860000),
        ("IRQ", 1000000),
        ("ISL", 60000),
        ("ISR", 292649),
        ("ITA", 8444140),
        ("JAM", 206243),
        ("JOR", 76920),
        ("JPN", 10620000),
        ("KAZ", 1261600),
        ("KEN", 1421600),
        ("KGZ", 338800),
        ("KHM", 1444000),
        ("KNA", 1544),
        ("KOR", 2380800),
        ("KWT", 69000),
        ("LAO", 382399),
        ("LBN", 418760),
        ("LBR", 330162),
        ("LBY", 444000),
        ("LCA", 19813),
        ("LIE", 6937),
        ("LKA", 960800),
        ("LSO", 58028),
        ("LTU", 97450),
        ("LUX", 61140),
        ("LVA", 83589),
        ("MAR", 1668000),
        ("MDA", 268320),
        ("MDG", 606666),
        ("MEX", 23373121),
        ("MKD", 175520),
        ("MLI", 1036520),
        ("MLT", 14400),
        ("MMR", 3692000),
        ("MNG", 600000),
        ("MOZ", 921600),
        ("MRT", 247920),
        ("MSR", 726),
        ("MTQ", 64684),
        ("MWI", 337008),
        ("MYS", 374048),
        ("NAM", 128541),
        ("NCL", 16411),
        ("NER", 781680),
        ("NGA", 10440000),
        ("NIC", 309926),
        ("NLD", 830550),
        ("NOR", 288000),
        ("NPL", 1944000),
        ("NZL", 46400),
        ("OMN", 200000),
        ("PAK", 8214400),
        ("PAN", 185825),
        ("PER", 3888000),
        ("PHL", 455096),
        ("PNG", 1094701),
        ("POL", 3720000),
        ("PRI", 3741),
        ("PRK", 1339200),
        ("PRT", 967160),
        ("PRY", 188800),
        ("QAT", 620),
        ("REU", 164),
        ("ROU", 1580000),
        ("RUS", 6612480),
        ("RWA", 565056),
        ("SAU", 1972000),
        ("SDN", 3850666),
        ("SEN", 329281),
        ("SGP", 485),
        ("SJM", 1),
        ("SLB", 1177),
        ("SLE", 6272),
        ("SLV", 87464),
        ("SOM", 734809),
        ("SPM", 292),
        ("STP", 3644),
        ("SUR", 13907),
        ("SVK", 761600),
        ("SVN", 225996),
        ("SWE", 466256),
        ("SWZ", 11894),
        ("SYR", 1025240),
        ("TCA", 13),
        ("TCD", 803760),
        ("TGO", 400403),
        ("THA", 1906836),
        ("TJK", 298800),
        ("TKM", 228800),
        ("TLS", 91080),
        ("TON", 4739),
        ("TTO", 82),
        ("TUN", 800000),
        ("TUR", 5998000),
        ("TZA", 2282000),
        ("UGA", 1421600),
        ("UKR", 2363280),
        ("URY", 22634),
        ("USA", 1804248),
        ("UZB", 1215200),
        ("VCT", 3327),
        ("VEN", 388800),
        ("VIR", 996),
        ("VNM", 1865600),
        ("VUT", 5708),
        ("WSM", 3038),
        ("YEM", 2138000),
        ("ZAF", 487104),
        ("ZMB", 286032),
        ("ZWE", 244800),
    ])
}

/// Approximate polygon area in degree² (shoelace formula with cos(lat) correction).
fn province_area(mp: &MapProvince) -> f64 {
    if mp.boundary.is_empty() {
        return 0.0;
    }
    let ring = &mp.boundary[0];
    if ring.len() < 3 {
        return 0.0;
    }
    // Shoelace formula
    let mut area2 = 0.0f64;
    let n = ring.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area2 += f64::from(ring[i][0]) * f64::from(ring[j][1]);
        area2 -= f64::from(ring[j][0]) * f64::from(ring[i][1]);
    }
    let raw = area2.abs() / 2.0;
    // Correct for latitude: multiply by cos(centroid_lat) to get a more realistic area proxy.
    let lat = f64::from(mp.centroid[1]);
    raw * lat.to_radians().cos()
}

/// Medieval (1444) class distribution ratios.
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

/// Default population density for countries not in OWID data (people per degree²).
/// Roughly ~5 people/km² medieval average → ~60,000 per degree² at mid-latitudes.
const FALLBACK_POP_DENSITY: f64 = 60_000.0;

/// Generate a full game world from map data using OWID/HYDE historical population for 1444.
/// Population is distributed proportionally by province polygon area within each country.
pub fn generate_world(map_data: &MapData) -> GameState {
    let building_types = default_building_types();
    let pop_data = pop_1444_data();

    // Step 1: compute total polygon area per country.
    let mut country_total_area: HashMap<String, f64> = HashMap::new();
    let mut province_areas: Vec<f64> = Vec::with_capacity(map_data.provinces.len());

    for mp in &map_data.provinces {
        let area = province_area(mp);
        province_areas.push(area);
        *country_total_area
            .entry(mp.country_code.clone())
            .or_insert(0.0) += area;
    }

    // Step 2: create provinces with proportional population.
    let provinces: Vec<Province> = map_data
        .provinces
        .iter()
        .enumerate()
        .map(|(idx, mp)| {
            let country = &mp.country_code;
            let total_area = country_total_area.get(country).copied().unwrap_or(1.0);
            let prov_area = province_areas[idx];

            // Country total population (OWID data or area-based fallback).
            let country_pop = pop_data
                .get(country.as_str())
                .map(|&p| u32_to_f64(p))
                .unwrap_or_else(|| total_area * FALLBACK_POP_DENSITY);

            // Province share of country population, proportional to area.
            let share = if total_area > 0.0 {
                prov_area / total_area
            } else {
                1.0
            };
            let province_pop = f64_to_u32((country_pop * share).max(10.0));

            // Distribute into pop classes by medieval ratios.
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

            // Building levels scale with total population.
            let farm_level = (province_pop / 1500).max(1);
            let kiln_level = (province_pop / 5000).max(1);

            Province {
                id: mp.id,
                name: mp.name.clone(),
                owner: Some(mp.country_code.clone()),
                pops,
                buildings: vec![
                    Building { type_id: "farm".into(), level: farm_level },
                    Building { type_id: "charcoal_kiln".into(), level: kiln_level },
                ],
                stockpile: HashMap::from([
                    (Good::Grain, 20.0),
                    (Good::Clothing, 5.0),
                    (Good::Fuel, 3.0),
                ]),
            }
        })
        .collect();

    // Log population totals for verification.
    let total: u64 = provinces
        .iter()
        .map(|p| p.pops.iter().map(|pop| u64::from(pop.size)).sum::<u64>())
        .sum();
    println!("World population (1444): {total}");

    GameState {
        tick: 0,
        date: GameDate::default(),
        provinces,
        building_types,
    }
}
