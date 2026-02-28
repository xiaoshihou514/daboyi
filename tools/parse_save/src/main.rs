use std::fs::File;
use std::io::{BufRead, BufReader, Write};

fn main() {
    let mut args = std::env::args().skip(1);
    let save_path = args
        .next()
        .unwrap_or_else(|| "/home/xiaoshihou/Playground/github/EU5toGIS/ti.eu5".to_string());
    let out_path = args
        .next()
        .unwrap_or_else(|| "assets/ownership.tsv".to_string());

    eprintln!("Parsing {}...", save_path);

    let (compat_names, country_tags, ownership_ids) = parse_text_save(&save_path);

    eprintln!(
        "Location names: {}, Country tags: {}, Raw ownerships: {}",
        compat_names.len(),
        country_tags.len(),
        ownership_ids.len()
    );

    // Join: location_id (GPKG_id, 1-based) → location_name → owner_tag
    let mut ownership: Vec<(String, String)> = Vec::new();
    for (loc_id, owner_country_id) in &ownership_ids {
        // compat_names is 0-indexed; GPKG_id is 1-based
        let idx = usize::try_from(*loc_id)
            .ok()
            .and_then(|n| n.checked_sub(1));
        let Some(idx) = idx else { continue };
        let Some(loc_name) = compat_names.get(idx) else { continue };
        if loc_name.is_empty() { continue; }
        let Some(owner_tag) = country_tags.get(usize::try_from(*owner_country_id).unwrap_or(usize::MAX)) else { continue };
        if owner_tag.is_empty() { continue; }
        ownership.push((loc_name.clone(), owner_tag.clone()));
    }

    // Spot-check known provinces
    let checks = ["stockholm", "london", "paris", "delhi", "hangzhou", "istanbul", "cairo", "wuxian"];
    eprintln!("\nSample:");
    for name in &checks {
        let owner = ownership.iter().find(|(l, _)| l == name).map(|(_, o)| o.as_str()).unwrap_or("(none)");
        eprintln!("  {name} -> {owner}");
    }

    // Write TSV sorted by location name
    ownership.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = File::create(&out_path).expect("Failed to create output");
    writeln!(out, "location_tag\towner_tag").unwrap();
    for (loc, owner) in &ownership {
        writeln!(out, "{loc}\t{owner}").unwrap();
    }
    eprintln!("\nWritten {} entries to {out_path}", ownership.len());
}

/// Parse ti.eu5 and return:
///   - compat_names: Vec of province names, index i = GPKG_id (i+1)
///   - country_tags: Vec of country tags, index i = country integer id
///   - ownership_ids: Vec of (location_gpkg_id, owner_country_id)
fn parse_text_save(path: &str) -> (Vec<String>, Vec<String>, Vec<(u32, u32)>) {
    let file = File::open(path).expect("Failed to open text save");
    let reader = BufReader::new(file);

    let mut compat_names: Vec<String> = Vec::new();
    let mut country_tags: Vec<String> = Vec::new();
    let mut ownership_ids: Vec<(u32, u32)> = Vec::new();

    // Section state
    let mut in_compatibility = false;
    let mut in_countries = false;
    let mut in_tags_block = false;
    let mut tags_depth: i32 = 0;
    let mut in_locations_outer = false;
    let mut in_locations_inner = false;
    let mut locations_depth: i32 = 0;
    let mut current_loc_id: Option<u32> = None;
    let mut current_owner_id: Option<u32> = None;
    let mut loc_block_depth: i32 = 0;

    for line in reader.lines() {
        let line = line.expect("Read error");
        let trimmed = line.trim();

        // ── Compatibility section (location names) ──────────────────────────
        if trimmed == "compatibility={" {
            in_compatibility = true;
            continue;
        }
        if in_compatibility {
            // The compatibility.locations line contains all names space-separated
            if let Some(rest) = trimmed.strip_prefix("locations={") {
                let names_str = rest.trim_end_matches('}');
                compat_names = names_str.split_whitespace().map(str::to_string).collect();
                in_compatibility = false;
                continue;
            }
            if trimmed == "}" {
                in_compatibility = false;
            }
            continue;
        }

        // ── Countries section (id → tag mapping) ────────────────────────────
        if trimmed == "countries={" {
            in_countries = true;
            continue;
        }
        if in_countries {
            if trimmed == "tags={" {
                in_tags_block = true;
                tags_depth = 1;
                continue;
            }
            if in_tags_block {
                if trimmed == "}" {
                    tags_depth -= 1;
                    if tags_depth <= 0 {
                        in_tags_block = false;
                        in_countries = false;
                    }
                    continue;
                }
                if let Some(eq) = trimmed.find('=') {
                    if let Ok(id) = trimmed[..eq].parse::<usize>() {
                        let tag = trimmed[eq + 1..].trim().to_string();
                        if id >= country_tags.len() {
                            country_tags.resize(id + 1, String::new());
                        }
                        country_tags[id] = tag;
                    }
                }
                continue;
            }
            if trimmed == "}" {
                in_countries = false;
            }
            continue;
        }

        // ── Locations section (GPKG_id → owner country id) ─────────────────
        if trimmed == "locations={" && !in_locations_outer {
            in_locations_outer = true;
            locations_depth = 0;
            continue;
        }
        if in_locations_outer {
            if trimmed == "locations={" && locations_depth == 0 {
                in_locations_inner = true;
                locations_depth = 1;
                continue;
            }
            if in_locations_inner {
                let opens = trimmed.chars().filter(|&c| c == '{').count();
                let closes = trimmed.chars().filter(|&c| c == '}').count();

                if locations_depth == 1 {
                    if let Some(rest) = trimmed.strip_suffix("={") {
                        if let Ok(id) = rest.trim().parse::<u32>() {
                            current_loc_id = Some(id);
                            current_owner_id = None;
                            loc_block_depth = 1;
                            locations_depth += 1;
                            continue;
                        }
                    }
                    if trimmed == "}" {
                        locations_depth -= 1;
                        if locations_depth <= 0 {
                            in_locations_inner = false;
                            in_locations_outer = false;
                        }
                        continue;
                    }
                }

                if locations_depth >= 2 && current_loc_id.is_some() {
                    if loc_block_depth == 1 {
                        if let Some(val) = trimmed.strip_prefix("owner=") {
                            if let Ok(id) = val.parse::<u32>() {
                                current_owner_id.get_or_insert(id);
                            }
                        }
                    }

                    let delta = i32::try_from(opens).unwrap_or(0)
                        - i32::try_from(closes).unwrap_or(0);
                    locations_depth += delta;
                    loc_block_depth += delta;

                    if loc_block_depth <= 0 {
                        if let (Some(loc_id), Some(owner_id)) =
                            (current_loc_id.take(), current_owner_id.take())
                        {
                            ownership_ids.push((loc_id, owner_id));
                        }
                        current_loc_id = None;
                        current_owner_id = None;
                        loc_block_depth = 0;
                        locations_depth = 1;
                    }
                }
            }
        }
    }

    (compat_names, country_tags, ownership_ids)
}
