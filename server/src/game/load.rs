/// TSV asset loaders for EU5-derived game data.
use std::collections::HashMap;
use std::fs;

const OWNERSHIP_TSV: &str = "assets/ownership.tsv";
const POPS_TSV: &str = "assets/pops.tsv";
const PROVINCE_NAMES_TSV: &str = "assets/province_names.tsv";
const COUNTRY_NAMES_TSV: &str = "assets/country_names.tsv";
const VASSALS_TSV: &str = "assets/vassals.tsv";
const MERCHANDIZE_TSV: &str = "assets/merchandize.tsv";
const CAPITALS_TSV: &str = "assets/capitals.tsv";

pub fn load_province_names() -> HashMap<String, String> {
    load_tsv(PROVINCE_NAMES_TSV, "province names")
}

pub fn load_country_names() -> HashMap<String, String> {
    load_tsv(COUNTRY_NAMES_TSV, "country names")
}

/// Generic two-column TSV loader: col0=key, col1=value, skip header row.
fn load_tsv(path: &str, label: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {path}: {e}. {label} will fall back to tag.");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(2, '\t');
        if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            let k = k.trim();
            let v = v.trim();
            if !k.is_empty() && !v.is_empty() {
                map.insert(k.to_string(), v.to_string());
            }
        }
    }
    println!("Loaded {} {label} from {path}", map.len());
    map
}

/// Load province population data.
/// Returns location_tag → headcount (EU5 file is in thousands).
pub fn load_eu5_pops() -> HashMap<String, u32> {
    let mut map = HashMap::new();
    let content = match fs::read_to_string(POPS_TSV) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {POPS_TSV}: {e}. Population falls back to terrain estimates.");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(3, '\t');
        if let (Some(loc), Some(pop_str)) = (parts.next(), parts.next()) {
            let loc = loc.trim();
            if let Ok(pop_thousands) = pop_str.trim().parse::<f64>() {
                let headcount = ((pop_thousands * 1000.0).round() as u32).max(10);
                map.insert(loc.to_string(), headcount);
            }
        }
    }
    println!("Loaded {} EU5 province populations from {POPS_TSV}", map.len());
    map
}

/// Load province ownership: location_tag → owner_tag.
pub fn load_eu5_ownership() -> HashMap<String, String> {
    load_tsv(OWNERSHIP_TSV, "province ownerships")
}

/// Load vassal relationships: subject_tag → overlord_tag.
pub fn load_vassals() -> HashMap<String, String> {
    load_tsv(VASSALS_TSV, "vassal relationships")
}

/// Load country capital locations: country_tag → capital_location_name.
pub fn load_capitals() -> HashMap<String, String> {
    load_tsv(CAPITALS_TSV, "country capitals")
}

/// Load merchandize production: country_tag → Vec<(good, amount)> sorted by amount desc.
pub fn load_merchandize() -> HashMap<String, Vec<(String, f32)>> {
    let mut map: HashMap<String, Vec<(String, f32)>> = HashMap::new();
    let content = match fs::read_to_string(MERCHANDIZE_TSV) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Warning: could not load {MERCHANDIZE_TSV}: {e}");
            return map;
        }
    };
    for line in content.lines().skip(1) {
        let mut parts = line.splitn(3, '\t');
        if let (Some(tag), Some(good), Some(amt_str)) = (parts.next(), parts.next(), parts.next()) {
            if let Ok(amount) = amt_str.trim().parse::<f32>() {
                map.entry(tag.trim().to_string())
                    .or_default()
                    .push((good.trim().to_string(), amount));
            }
        }
    }
    // TSV is pre-sorted by amount desc (parse_save writes it that way), so no re-sort needed.
    println!("Loaded merchandize for {} countries from {MERCHANDIZE_TSV}", map.len());
    map
}
