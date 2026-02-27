use geo::algorithm::contains::Contains;
use geo::{Coord, LineString, MultiPolygon, Polygon};
use geojson::{GeoJson, Value};
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

/// Target world population for 1356 (~500M estimated).
const TARGET_WORLD_POP: f64 = 500_000_000.0;

/// Path to the historical boundary GeoJSON asset.
const HISTORICAL_GEOJSON: &str = "assets/world_1400.geojson";

// ── Chinese warlord faction definitions (circa 1356) ─────────────────────────

/// Chinese warlord factions and their display names.
const WARLORD_DEFS: &[(&str, &str)] = &[
    ("YUA", "元朝"),
    ("HSG", "韩宋"),
    ("ZYZ", "朱元璋"),
    ("CYL", "陈友谅"),
    ("ZSC", "张士诚"),
    ("FGZ", "方国珍"),
    ("XIA", "明夏"),
];

/// Returns true if this province centroid falls within greater China (including Mongolia/Tibet).
fn is_china_region(lon: f64, lat: f64) -> bool {
    // Broad China bbox: lon 73–135, lat 18–54
    lon > 73.0 && lon < 135.0 && lat > 18.0 && lat < 54.0
        // Exclude Southeast Asia (Vietnam, Laos, etc.)
        && !(lon > 100.0 && lat < 23.0 && lon < 110.0)
        // Exclude Korea
        && !(lon > 124.0 && lat > 33.0 && lat < 44.0 && lon < 132.0)
        // Exclude Japan
        && !(lon > 128.0 && lat > 30.0)
}

/// Assign a Chinese province (by lat/lon) to a warlord faction.
fn chinese_warlord_tag(lon: f64, lat: f64) -> &'static str {
    // Taiwan — indigenous peoples
    if lon > 119.5 && lon < 122.5 && lat > 21.5 && lat < 25.5 {
        return "UNC";
    }

    // Mongolia/outer regions → Yuan
    if lat > 42.0 || lon < 80.0 || lon > 125.0 {
        return "YUA";
    }

    // Ming Yuzhen (Sichuan basin, Chongqing, Guizhou, Yunnan)
    if lon < 108.0 && lat < 34.0 && lat > 22.0 {
        return "XIA";
    }

    // Han Song / Red Turban North (Henan ~32-36N, 110-116E)
    if lat > 32.0 && lat < 36.5 && lon > 110.0 && lon < 116.5 {
        return "HSG";
    }

    // Zhu Yuanzhang (southern Anhui + Nanjing region, 30-33N, 116-119E)
    if lat > 29.5 && lat < 33.0 && lon > 115.5 && lon < 119.5 {
        return "ZYZ";
    }

    // Zhang Shicheng (Shanghai/Jiangsu coast, 30-33N, 119-122E)
    if lat > 30.0 && lat < 33.5 && lon > 119.0 && lon < 122.0 {
        return "ZSC";
    }

    // Fang Guozhen (Zhejiang coast, 27-30N, 119-122E)
    if lat > 27.0 && lat < 30.5 && lon > 119.0 && lon < 122.5 {
        return "FGZ";
    }

    // Chen Youliang (Hubei/Hunan/Jiangxi, 24-32N, 108-117E)
    if lat > 24.0 && lat < 32.0 && lon > 108.0 && lon < 117.0 {
        return "CYL";
    }

    // Everything else in China → Yuan
    "YUA"
}

// ── Historical boundary mapping (world_1400 polity name → game tag) ──────────

/// Map historical polity name from world_1400.geojson → (tag, display_name).
fn country_tag_for_name(name: &str) -> (&'static str, &'static str) {
    match name {
        // Major European powers
        "France" => ("FRA", "法兰西王国"),
        "English territory" => ("ENG", "英格兰王国"),
        "Castile" => ("CAS", "卡斯蒂利亚王国"),
        "Aragón" => ("ARA", "阿拉贡王国"),
        "Portugal" => ("PRT", "葡萄牙王国"),
        "Holy Roman Empire" => ("HRE", "神圣罗马帝国"),
        "Scotland" => ("SCO", "苏格兰王国"),
        "Navarre" => ("NAV", "纳瓦拉王国"),
        "Granada" => ("GRA", "格拉纳达酋长国"),
        "Britany" => ("BRI", "布列塔尼公国"),
        "Corsica" => ("COR", "科西嘉"),
        "Sardinia" => ("SAR", "撒丁岛"),
        "Sicily" => ("SIC", "西西里王国"),
        "Venice" => ("VNC", "威尼斯共和国"),
        "Papal States" => ("PAP", "教皇国"),

        // Eastern Europe
        "Poland-Lithuania" => ("PLC", "波兰-立陶宛"),
        "Kalmar Union" => ("KAL", "卡尔马联盟"),
        "Novgorod" => ("NOV", "诺夫哥罗德共和国"),
        "Kingdom of Hungary" => ("HUN", "匈牙利王国"),
        "Bosnia" => ("BOS", "波斯尼亚王国"),
        "Moldova" => ("MOL", "摩尔达维亚"),
        "Principality of Wallachia" => ("WAL", "瓦拉几亚公国"),
        "Teutonic Knights" => ("TEU", "条顿骑士团"),
        "Bulgar Khanate" => ("BUL", "保加利亚"),
        "Byzantine Empire" => ("BYZ", "拜占庭帝国"),
        "Georgia" => ("GEO", "格鲁吉亚王国"),
        "Cyprus" => ("CYR", "塞浦路斯王国"),

        // Islamic world
        "Ottoman Empire" => ("OTT", "奥斯曼帝国"),
        "Mamluke Sultanate" => ("MAM", "马穆鲁克苏丹国"),
        "Timurid Empire" => ("TIM", "帖木儿帝国"),
        "Seljuk Caliphate" => ("SEL", "塞尔柱苏丹国"),
        "Hafsid Caliphate" => ("HAF", "哈夫斯王朝"),
        "Morocco" => ("MOR", "摩洛哥"),
        "Beylik of Aydin" => ("AYD", "艾登侯国"),
        "Hadramaut" => ("HAD", "哈德拉毛"),
        "Muscat" => ("MUS", "马斯喀特"),
        "Yemen" => ("YMN", "也门"),

        // Mongol successor states
        "Blue Horde" => ("GLD", "金帐汗国"),
        "White Horde" => ("WHT", "白帐汗国"),
        "Chagatai Khanate" => ("CHA", "察合台汗国"),

        // South/Southeast Asia
        "Sultanate of Delhi" => ("DEL", "德里苏丹国"),
        "Chola state" => ("CHO", "朱罗国"),
        "Pandya state" => ("PAN", "潘地亚国"),
        "Orissa" => ("ORI", "奥里萨"),
        "Kashmir and Ladakh" => ("KAS", "克什米尔"),
        "Sinhalese kingdom" => ("SIN", "僧伽罗王国"),
        "Ayutthaya" => ("AYU", "阿瑜陀耶"),
        "Sukhothai" => ("SUK", "素可泰"),
        "Pagan" => ("PAG", "蒲甘"),
        "Khmer Empire" => ("KHR", "高棉帝国"),
        "Champa" => ("CMP", "占城"),
        "Srivijaya Empire" => ("SRI", "三佛齐"),
        "Kediri" => ("KED", "谏义里"),
        "Aceh" => ("ACE", "亚齐"),

        // East Asia (excluding China — handled by warlord system)
        "Shogun Japan (Kamakura)" => ("JAP", "日本幕府"),
        "Đại Việt" => ("DAV", "大越"),
        "Hainan" => ("HAN", "海南"),
        "Tibet" => ("TIB", "西藏"),
        "Chūzan" => ("RYU", "琉球中山"),
        "Hokuzan" => ("HOK", "琉球北山"),
        "Nanzan" => ("NAN", "琉球南山"),
        "minor Hindu and Buddhist kingdoms" => ("HBK", "印度诸邦"),

        // Africa
        "Mali" => ("MLI", "马里帝国"),
        "Ethiopia" => ("ETI", "埃塞俄比亚"),
        "Shoa" => ("SHO", "绍阿"),
        "Alwa" => ("ALW", "阿尔瓦"),
        "Makkura" => ("MAK", "马库里亚"),
        "Benin" => ("BNI", "贝宁帝国"),
        "Bornu-Kanem" => ("BKN", "博尔努-加涅姆"),
        "Great Zimbabwe" => ("ZIM", "大津巴布韦"),
        "Expansionist Kingdom of Merina" => ("MER", "梅里纳王国"),
        "Madagascar" => ("MDG", "马达加斯加"),

        // Americas
        "Chimú Empire" => ("CHI", "奇穆帝国"),
        "Mixtec Empire" => ("MIX", "米斯特克"),
        "Zapotec Empire" => ("ZAP", "萨波特克"),

        // Skip Great Khanate (China handled by warlord system)
        "Great Khanate" => ("_SKIP_", ""),

        // Everything else gets a generic tag
        _ => ("", ""),
    }
}

/// Auto-generate a 3-character tag from a name (for unmapped polities).
fn auto_tag(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ')
        .collect();
    let words: Vec<&str> = clean.split_whitespace().collect();
    if words.len() >= 3 {
        format!(
            "{}{}{}",
            words[0].chars().next().unwrap_or('X'),
            words[1].chars().next().unwrap_or('X'),
            words[2].chars().next().unwrap_or('X'),
        )
        .to_uppercase()
    } else if words.len() == 2 {
        let w1 = words[0];
        format!(
            "{}{}{}",
            w1.chars().next().unwrap_or('X'),
            w1.chars().nth(1).unwrap_or('X'),
            words[1].chars().next().unwrap_or('X'),
        )
        .to_uppercase()
    } else {
        let w = words.first().copied().unwrap_or("UNK");
        w.chars().take(3).collect::<String>().to_uppercase()
    }
}

// ── Historical boundary loading ──────────────────────────────────────────────

struct HistoricalPolity {
    tag: String,
    name: String,
    geometry: MultiPolygon<f64>,
}

fn parse_geojson_geometry(value: &Value) -> Option<MultiPolygon<f64>> {
    match value {
        Value::Polygon(rings) => {
            let outer: Vec<Coord<f64>> = rings
                .first()?
                .iter()
                .map(|c| Coord { x: c[0], y: c[1] })
                .collect();
            let holes: Vec<LineString<f64>> = rings[1..]
                .iter()
                .map(|ring| {
                    LineString::new(ring.iter().map(|c| Coord { x: c[0], y: c[1] }).collect())
                })
                .collect();
            Some(MultiPolygon::new(vec![Polygon::new(
                LineString::new(outer),
                holes,
            )]))
        }
        Value::MultiPolygon(multi) => {
            let polys: Vec<Polygon<f64>> = multi
                .iter()
                .filter_map(|rings| {
                    let outer: Vec<Coord<f64>> = rings
                        .first()?
                        .iter()
                        .map(|c| Coord { x: c[0], y: c[1] })
                        .collect();
                    let holes: Vec<LineString<f64>> = rings[1..]
                        .iter()
                        .map(|ring| {
                            LineString::new(
                                ring.iter().map(|c| Coord { x: c[0], y: c[1] }).collect(),
                            )
                        })
                        .collect();
                    Some(Polygon::new(LineString::new(outer), holes))
                })
                .collect();
            if polys.is_empty() {
                None
            } else {
                Some(MultiPolygon::new(polys))
            }
        }
        _ => None,
    }
}

fn load_historical_polities() -> Vec<HistoricalPolity> {
    let mut polities = Vec::new();
    let mut used_tags: HashMap<String, usize> = HashMap::new();

    let content = match fs::read_to_string(HISTORICAL_GEOJSON) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Warning: could not load {HISTORICAL_GEOJSON}: {e}. \
                 Province ownership will fall back to modern ISO codes."
            );
            return polities;
        }
    };

    let geojson: GeoJson = match content.parse() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("Warning: failed to parse {HISTORICAL_GEOJSON}: {e}");
            return polities;
        }
    };

    let features = match geojson {
        GeoJson::FeatureCollection(fc) => fc.features,
        _ => return polities,
    };

    for feat in &features {
        let props = match &feat.properties {
            Some(p) => p,
            None => continue,
        };
        let hist_name = match props.get("NAME").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        let (tag, display) = country_tag_for_name(hist_name);

        // Skip entries marked for skipping (e.g., Great Khanate → handled by warlord system)
        if tag == "_SKIP_" {
            continue;
        }

        let geom = match feat
            .geometry
            .as_ref()
            .and_then(|g| parse_geojson_geometry(&g.value))
        {
            Some(g) => g,
            None => continue,
        };

        let (final_tag, final_name) = if tag.is_empty() {
            let auto = auto_tag(hist_name);
            (auto, hist_name.to_string())
        } else {
            (tag.to_string(), display.to_string())
        };

        // Disambiguate duplicate tags
        let count = used_tags.entry(final_tag.clone()).or_insert(0);
        let unique_tag = if *count > 0 {
            format!("{}{}", final_tag, count)
        } else {
            final_tag.clone()
        };
        *count += 1;

        polities.push(HistoricalPolity {
            tag: unique_tag,
            name: final_name,
            geometry: geom,
        });
    }

    println!("Loaded {} historical polities from {}", polities.len(), HISTORICAL_GEOJSON);
    polities
}

/// Generate a full game world from EU5 map data using terrain-based population and historical 1356 boundaries.
pub fn generate_world(map_data: &MapData) -> GameState {
    let building_types = default_building_types();

    // Step 1: compute terrain-based raw population for each province, then scale to target.
    let mut raw_pops: Vec<f64> = Vec::with_capacity(map_data.provinces.len());
    let mut total_raw: f64 = 0.0;

    for mp in &map_data.provinces {
        let area = province_area(mp);
        let density = terrain_density(mp);
        let raw = area * density;
        raw_pops.push(raw);
        total_raw += raw;
    }

    // Scale factor to achieve target world population.
    let scale = if total_raw > 0.0 {
        TARGET_WORLD_POP / total_raw
    } else {
        1.0
    };

    // Step 2: load historical boundaries and assign provinces to historical countries.
    let polities = load_historical_polities();
    let mut province_owners: Vec<Option<String>> = Vec::with_capacity(map_data.provinces.len());

    for mp in &map_data.provinces {
        let lon = f64::from(mp.centroid[0]);
        let lat = f64::from(mp.centroid[1]);

        let owner = if is_china_region(lon, lat) {
            let tag = chinese_warlord_tag(lon, lat);
            if tag == "UNC" {
                None
            } else {
                Some(tag.to_string())
            }
        } else {
            // World provinces → point-in-polygon against historical boundaries
            let centroid = geo::Point::new(lon, lat);
            let mut found = None;
            for polity in &polities {
                if polity.geometry.contains(&centroid) {
                    found = Some(polity.tag.clone());
                    break;
                }
            }
            found
        };
        province_owners.push(owner);
    }

    // Step 3: collect all unique country tags and create Country entities.
    let mut country_map: HashMap<String, String> = HashMap::new();

    for (tag, name) in WARLORD_DEFS {
        country_map.insert(tag.to_string(), name.to_string());
    }

    for polity in &polities {
        country_map
            .entry(polity.tag.clone())
            .or_insert_with(|| polity.name.clone());
    }

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
            tag: tag.clone(),
            name: country_map
                .get(tag)
                .cloned()
                .unwrap_or_else(|| tag.clone()),
            capital_province: first_prov,
        })
        .collect();
    countries.sort_by(|a, b| a.tag.cmp(&b.tag));

    println!("Created {} countries", countries.len());

    // Step 4: create provinces with scaled population + resource-based initial stockpile.
    let provinces: Vec<Province> = map_data
        .provinces
        .iter()
        .enumerate()
        .map(|(idx, mp)| {
            let province_pop = f64_to_u32((raw_pops[idx] * scale).max(10.0));

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
                name: mp.name.clone(),
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
