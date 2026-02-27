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

/// Historical population data for year 1356, interpolated from HYDE 3.3 (via OWID).
/// Source: OWID indicator 953903. Interpolated: p1300 + (p1400 - p1300) * 56/100.
/// Key: ISO-3166 alpha-3 code (matches shapeGroup in world GeoJSON).
/// Note: TWN population is folded into CHN since Taiwan uses CN ADM3 data.
fn pop_1356_data() -> HashMap<&'static str, u32> {
    HashMap::from([
        ("ABW", 97),
        ("AFG", 1820000),
        ("AGO", 1646560),
        ("AIA", 760),
        ("ALB", 200000),
        ("AND", 1602),
        ("ANT", 755),
        ("ARE", 31000),
        ("ARG", 271200),
        ("ARM", 193360),
        ("ASM", 454),
        ("ATG", 6198),
        ("AUS", 316016),
        ("AUT", 1580000),
        ("AZE", 412920),
        ("BDI", 552656),
        ("BEL", 998000),
        ("BEN", 2568000),
        ("BFA", 1160040),
        ("BGD", 8559360),
        ("BGR", 748000),
        ("BHR", 18624),
        ("BHS", 3739),
        ("BIH", 336960),
        ("BLR", 432000),
        ("BLZ", 6079),
        ("BOL", 813600),
        ("BRA", 15718111),
        ("BRB", 837),
        ("BRN", 4624),
        ("BTN", 11560),
        ("BWA", 78254),
        ("CAF", 262000),
        ("CAN", 171200),
        ("CHE", 688000),
        ("CHL", 542400),
        // CHN includes TWN (169637) folded in since Taiwan uses CN ADM3 data.
        ("CHN", 75257589),
        ("CIV", 852019),
        ("CMR", 1139351),
        ("COD", 4828160),
        ("COG", 217319),
        ("COL", 3534753),
        ("COM", 17120),
        ("CRI", 397375),
        ("CUB", 46580),
        ("CYP", 172000),
        ("CZE", 1958400),
        ("DEU", 7600000),
        ("DJI", 18489),
        ("DMA", 993),
        ("DNK", 488000),
        ("DOM", 145920),
        ("DZA", 1564600),
        ("ECU", 542400),
        ("EGY", 4336000),
        ("ERI", 96511),
        ("ESH", 181),
        ("ESP", 6380000),
        ("EST", 58162),
        ("ETH", 2355000),
        ("FIN", 104400),
        ("FJI", 43793),
        ("FRA", 13076279),
        ("FRO", 158),
        ("GAB", 122461),
        ("GBR", 4410501),
        ("GEO", 504160),
        ("GHA", 1418242),
        ("GIN", 880879),
        ("GLP", 42430),
        ("GMB", 2898),
        ("GNB", 68709),
        ("GNQ", 90912),
        ("GRC", 1054000),
        ("GRD", 15984),
        ("GRL", 1128),
        ("GTM", 742400),
        ("GUF", 1356),
        ("GUY", 75600),
        ("HKG", 13287),
        ("HND", 796332),
        ("HRV", 505440),
        ("HTI", 113777),
        ("HUN", 1110000),
        ("IDN", 5770000),
        ("IND", 125843376),
        ("IRN", 3640000),
        ("IRQ", 1000000),
        ("ISL", 64400),
        ("ISR", 304458),
        ("ITA", 9984080),
        ("JAM", 111714),
        ("JOR", 80440),
        ("JPN", 8420000),
        ("KAZ", 1142800),
        ("KEN", 1298400),
        ("KGZ", 321200),
        ("KHM", 1356000),
        ("KNA", 836),
        ("KOR", 2099200),
        ("KWT", 69000),
        ("LAO", 354742),
        ("LBN", 450880),
        ("LBR", 315591),
        ("LBY", 356000),
        ("LCA", 18352),
        ("LIE", 6815),
        ("LKA", 947600),
        ("LSO", 51594),
        ("LTU", 93149),
        ("LUX", 60062),
        ("LVA", 79899),
        ("MAR", 1892400),
        ("MDA", 234000),
        ("MDG", 496666),
        ("MEX", 21667446),
        ("MKD", 168480),
        ("MLI", 956000),
        ("MLT", 14400),
        ("MMR", 3208000),
        ("MNG", 710000),
        ("MOZ", 798400),
        ("MRT", 231200),
        ("MSR", 393),
        ("MTQ", 35037),
        ("MWI", 305592),
        ("MYS", 339552),
        ("NAM", 120706),
        ("NCL", 14957),
        ("NER", 728000),
        ("NGA", 9560000),
        ("NIC", 287079),
        ("NLD", 740352),
        ("NOR", 288000),
        ("NPL", 1856000),
        ("NZL", 20000),
        ("OMN", 200000),
        ("PAK", 7578600),
        ("PAN", 169368),
        ("PER", 3712000),
        ("PHL", 384712),
        ("PNG", 1050791),
        ("POL", 3500000),
        ("PRI", 2616),
        ("PRK", 1180800),
        ("PRT", 967160),
        ("PRY", 171200),
        ("QAT", 601),
        ("REU", 164),
        ("ROU", 1360000),
        ("RUS", 5868000),
        ("RWA", 510144),
        ("SAU", 2068800),
        ("SDN", 3615999),
        ("SEN", 270544),
        ("SGP", 339),
        ("SJM", 1),
        ("SLB", 823),
        ("SLE", 5716),
        ("SLV", 81017),
        ("SOM", 666310),
        ("SPM", 158),
        ("STP", 2994),
        ("SUR", 12881),
        ("SVK", 761600),
        ("SVN", 218878),
        ("SWE", 726831),
        ("SWZ", 10785),
        ("SYR", 1103560),
        ("TCA", 6),
        ("TCD", 744800),
        ("TGO", 356003),
        ("THA", 1729080),
        ("TJK", 281200),
        ("TKM", 213840),
        ("TLS", 74833),
        ("TON", 3315),
        ("TTO", 44),
        ("TUN", 888000),
        ("TUR", 6416000),
        ("TZA", 2491000),
        ("UGA", 1298400),
        ("UKR", 2061000),
        ("URY", 20606),
        ("USA", 1665497),
        ("UZB", 1149200),
        ("VCT", 1802),
        ("VEN", 371200),
        ("VIR", 539),
        ("VNM", 1654400),
        ("VUT", 3992),
        ("WSM", 2124),
        ("YEM", 2116000),
        ("ZAF", 441696),
        ("ZMB", 259368),
        ("ZWE", 170000),
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

/// Assign a Chinese province (by gadm_id) to a warlord faction.
/// Uses GB administrative code prefix to determine region.
fn chinese_warlord_tag(gadm_id: &str) -> &'static str {
    let gb = gadm_id.strip_prefix("CN_").unwrap_or("");

    // Taiwan — indigenous peoples, not under organized state
    if gb.starts_with("1567") {
        return "UNC"; // uncolonized
    }

    let prefix = if gb.len() >= 2 { &gb[..2] } else { "" };

    match prefix {
        // Yuan (North China + frontiers + south coast still under Yuan)
        "11" | "12" | "13" | "14" | "15" | "21" | "22" | "23" | "37" | "61" | "62" | "63"
        | "64" | "65" | "54" | "35" | "44" | "45" | "46" => "YUA",
        // Han Song / Red Turban North (Henan)
        "41" => "HSG",
        // Zhu Yuanzhang (Anhui — just captured Nanjing)
        "34" => "ZYZ",
        // Chen Youliang / Xu Shouhui (Hubei, Hunan, Jiangxi)
        "42" | "43" | "36" => "CYL",
        // Zhang Shicheng (Shanghai, Jiangsu)
        "31" | "32" => "ZSC",
        // Fang Guozhen (Zhejiang coast)
        "33" => "FGZ",
        // Ming Yuzhen (Sichuan, Chongqing, Guizhou, Yunnan)
        "50" | "51" | "52" | "53" => "XIA",
        _ => "YUA",
    }
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

/// Generate a full game world from map data using historical 1356 boundaries and population.
/// Population is distributed by modern ISO country → area-proportional.
/// Ownership is determined by historical boundaries (1400 GeoJSON + Chinese warlord factions).
pub fn generate_world(map_data: &MapData) -> GameState {
    let building_types = default_building_types();
    let pop_data = pop_1356_data();

    // Step 1: compute total polygon area per modern country (for population distribution).
    let mut country_total_area: HashMap<String, f64> = HashMap::new();
    let mut province_areas: Vec<f64> = Vec::with_capacity(map_data.provinces.len());

    for mp in &map_data.provinces {
        let area = province_area(mp);
        province_areas.push(area);
        *country_total_area
            .entry(mp.country_code.clone())
            .or_insert(0.0) += area;
    }

    // Step 2: load historical boundaries and assign provinces to historical countries.
    let polities = load_historical_polities();
    let mut province_owners: Vec<Option<String>> = Vec::with_capacity(map_data.provinces.len());

    for mp in &map_data.provinces {
        let owner = if mp.country_code == "CHN" {
            // Chinese provinces → warlord faction by GB code
            let tag = chinese_warlord_tag(&mp.gadm_id);
            if tag == "UNC" {
                None
            } else {
                Some(tag.to_string())
            }
        } else if mp.country_code == "MNG" {
            // Mongolia → Yuan dynasty
            Some("YUA".to_string())
        } else {
            // World provinces → point-in-polygon against historical boundaries
            let centroid = geo::Point::new(
                f64::from(mp.centroid[0]),
                f64::from(mp.centroid[1]),
            );
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
    let mut country_map: HashMap<String, String> = HashMap::new(); // tag → display name

    // Chinese warlord factions
    for (tag, name) in WARLORD_DEFS {
        country_map.insert(tag.to_string(), name.to_string());
    }

    // Historical polities from GeoJSON
    for polity in &polities {
        country_map
            .entry(polity.tag.clone())
            .or_insert_with(|| polity.name.clone());
    }

    // Create Country entities for all tags that actually own at least one province.
    let mut active_tags: HashMap<String, u32> = HashMap::new(); // tag → first province id
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

    // Step 4: create provinces with population (by modern ISO → area-proportional).
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
    println!("World population (1356): {total}");

    GameState {
        tick: 0,
        date: GameDate::default(),
        countries,
        provinces,
        building_types,
    }
}
