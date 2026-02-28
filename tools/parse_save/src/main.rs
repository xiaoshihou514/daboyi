use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use zip::ZipArchive;

fn main() {
    let binary_save_path = "/home/xiaoshihou/Playground/github/EU5toGIS/save.eu5";
    let text_save_path = "/home/xiaoshihou/Playground/github/EU5toGIS/ti.eu5";
    let out_path = "/home/xiaoshihou/Playground/github/daboyi/assets/ownership.tsv";

    // Load binary save for location name mapping (compatibility.locations section).
    // The compatibility section contains all location names in GPKG_id order.
    eprintln!("Loading binary save for location names...");
    let binary_file = File::open(binary_save_path).expect("Failed to open binary save");
    let mut zip = ZipArchive::new(binary_file).expect("Failed to read ZIP");
    let mut gamestate_data = Vec::new();
    {
        let mut gs = zip.by_name("gamestate").unwrap();
        gs.read_to_end(&mut gamestate_data).unwrap();
    }
    let location_tags = read_location_tags_from_compat(&gamestate_data);
    eprintln!("Location tags from binary save: {}", location_tags.len());

    // Parse text save (ti.eu5) for ownership data.
    eprintln!("Parsing text save {}...", text_save_path);
    let ownership = parse_text_save(text_save_path, &location_tags);
    eprintln!("Ownership mappings: {}", ownership.len());

    let checks = ["stockholm", "london", "paris", "beijing", "delhi", "hangzhou", "istanbul", "cairo"];
    eprintln!("\nSample:");
    for tag in &checks {
        let owner = ownership.get(*tag).map(|s| s.as_str()).unwrap_or("(none)");
        eprintln!("  {tag} -> {owner}");
    }

    let mut out = File::create(out_path).expect("Failed to create output");
    writeln!(out, "location_tag\towner_tag").unwrap();
    let mut sorted: Vec<_> = ownership.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());
    for (loc, owner) in &sorted {
        writeln!(out, "{loc}\t{owner}").unwrap();
    }
    eprintln!("Written {out_path}");
}

/// Read the location tag list from the binary save's compatibility.locations section.
/// The tags are quoted strings at depth >= 2 in the first large block of the binary.
/// Index in this list = loc_id; GPKG_id = loc_id - 9 (first 9 entries are preamble strings).
fn read_location_tags_from_compat(data: &[u8]) -> Vec<String> {
    // The compatibility section is a UTF-8 quoted-string list at the start of the binary.
    // We scan for NUL-free ASCII printable quoted regions to extract them.
    // Format: 0x000f u16_len utf8_bytes  (QUOTED token in jomini binary)
    let mut tags: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 3 <= data.len() {
        // QUOTED token type = 0x000f (little-endian)
        if data[i] == 0x0f && data[i + 1] == 0x00 {
            let len = usize::from(u16::from_le_bytes([data[i + 2], data[i + 3]]));
            i += 4;
            if i + len <= data.len() {
                if let Ok(s) = std::str::from_utf8(&data[i..i + len]) {
                    tags.push(s.to_string());
                }
                i += len;
            }
            // Stop after collecting the first large batch (>1000 entries then seeing end marker)
            if tags.len() > 1000 && tags.last().map(|s: &String| s.is_empty()).unwrap_or(false) {
                break;
            }
        } else {
            i += 1;
        }
    }
    tags
}

/// Parse a text-format EU5 save (starts with "SAV", not a ZIP binary).
/// Returns map from location_name -> owner_tag.
///
/// Text save structure:
///   countries={ tags={ 0=DUMMY 1=PIR 2=MER ... } ... }
///   locations={ locations={ 1={ owner=3 ... } 2={ owner=3 ... } } }
///
/// Location IDs in text save = GPKG_ids (1-based).
/// Mapping: location_name = location_tags[gpkg_id + 9]
fn parse_text_save(path: &str, location_tags: &[String]) -> HashMap<String, String> {
    let file = File::open(path).expect("Failed to open text save");
    let reader = BufReader::new(file);

    // country_tags[id] = "SWE" etc.
    let mut country_tags: Vec<String> = Vec::new();
    let mut ownership: HashMap<String, String> = HashMap::new();

    // Section tracking
    let mut in_countries = false;
    let mut in_tags_block = false;
    let mut tags_brace_depth: i32 = 0;
    let mut in_locations_outer = false;
    let mut in_locations_inner = false;
    let mut locations_brace_depth: i32 = 0;
    let mut current_loc_id: Option<u32> = None;
    let mut current_owner_id: Option<u32> = None;
    let mut loc_block_depth: i32 = 0;

    for line in reader.lines() {
        let line = line.expect("Read error");
        let trimmed = line.trim();

        // Top-level section detection
        if trimmed == "countries={" {
            in_countries = true;
            in_locations_outer = false;
            continue;
        }
        if trimmed == "locations={" && !in_locations_outer {
            in_locations_outer = true;
            in_countries = false;
            locations_brace_depth = 0;
            continue;
        }

        // --- Parse country tags block ---
        if in_countries {
            if trimmed == "tags={" {
                in_tags_block = true;
                tags_brace_depth = 1;
                continue;
            }
            if in_tags_block {
                if trimmed == "}" {
                    tags_brace_depth -= 1;
                    if tags_brace_depth <= 0 {
                        in_tags_block = false;
                        in_countries = false;
                    }
                    continue;
                }
                // NUMBER=TAG
                if let Some(eq) = trimmed.find('=') {
                    let id_str = &trimmed[..eq];
                    let tag_str = trimmed[eq + 1..].trim();
                    if let Ok(id) = id_str.parse::<usize>() {
                        if id >= country_tags.len() {
                            country_tags.resize(id + 1, String::new());
                        }
                        country_tags[id] = tag_str.to_string();
                    }
                }
                continue;
            }
            if trimmed == "}" {
                in_countries = false;
            }
            continue;
        }

        // --- Parse locations section ---
        if in_locations_outer {
            if trimmed == "locations={" && locations_brace_depth == 0 {
                in_locations_inner = true;
                locations_brace_depth = 1;
                continue;
            }
            if in_locations_inner {
                let opens = i32::try_from(trimmed.chars().filter(|&c| c == '{').count()).unwrap_or(0);
                let closes = i32::try_from(trimmed.chars().filter(|&c| c == '}').count()).unwrap_or(0);

                if locations_brace_depth == 1 {
                    // Look for NUMBER={  (location block start)
                    if let Some(rest) = trimmed.strip_suffix("={") {
                        if let Ok(id) = rest.trim().parse::<u32>() {
                            current_loc_id = Some(id);
                            current_owner_id = None;
                            loc_block_depth = 1;
                            locations_brace_depth += 1;
                            continue;
                        }
                    }
                    if trimmed == "}" {
                        locations_brace_depth -= 1;
                        if locations_brace_depth <= 0 {
                            in_locations_inner = false;
                            in_locations_outer = false;
                        }
                        continue;
                    }
                }

                // Inside a location block
                if locations_brace_depth >= 2 && current_loc_id.is_some() {
                    // Capture the first owner= at loc_block_depth == 1
                    if loc_block_depth == 1 && trimmed.starts_with("owner=") {
                        let val = trimmed.trim_start_matches("owner=");
                        if let Ok(owner_id) = val.parse::<u32>() {
                            current_owner_id.get_or_insert(owner_id);
                        }
                    }

                    locations_brace_depth += opens - closes;
                    loc_block_depth += opens - closes;

                    if loc_block_depth <= 0 {
                        // Location block closed — record mapping
                        if let (Some(loc_id), Some(owner_id)) =
                            (current_loc_id.take(), current_owner_id.take())
                        {
                            // text save GPKG_id → location_tags index = gpkg_id + 9
                            let tags_idx = usize::try_from(loc_id)
                                .ok()
                                .and_then(|n| n.checked_add(9));
                            if let Some(idx) = tags_idx {
                                if let Some(loc_name) = location_tags.get(idx) {
                                    if !loc_name.is_empty() {
                                        let owner_idx = usize::try_from(owner_id).ok();
                                        if let Some(oi) = owner_idx {
                                            if let Some(tag) = country_tags.get(oi) {
                                                if !tag.is_empty() {
                                                    ownership.insert(
                                                        loc_name.clone(),
                                                        tag.clone(),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        current_loc_id = None;
                        current_owner_id = None;
                        loc_block_depth = 0;
                        locations_brace_depth = 1;
                    }
                }
            }
        }
    }

    ownership
}
