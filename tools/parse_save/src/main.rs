use jomini::binary::{Token, TokenReader};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use zip::ZipArchive;

fn parse_string_lookup(data: &[u8]) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 5usize;
    while i + 2 <= data.len() {
        let length = u16::from_le_bytes([data[i], data[i + 1]]) as usize;
        i += 2;
        if i + length > data.len() {
            break;
        }
        result.push(String::from_utf8_lossy(&data[i..i + length]).into_owned());
        i += length;
    }
    result
}

// Field token IDs discovered by statistics scan:
// FIELD 0x2dc0 = likely "owner" (20184 occurrences, values 1..=2263)
const OWNER_FIELD: u16 = 0x2dc0;

fn main() {
    let save_path = "/home/xiaoshihou/Playground/github/EU5toGIS/save.eu5";
    let file = File::open(save_path).expect("Failed to open save file");
    let mut zip = ZipArchive::new(file).expect("Failed to read ZIP");

    eprintln!("Loading save file...");
    let mut gamestate_data = Vec::new();
    {
        let mut gs = zip.by_name("gamestate").unwrap();
        gs.read_to_end(&mut gamestate_data).unwrap();
    }

    let mut sl_data = Vec::new();
    {
        let mut sl = zip.by_name("string_lookup").unwrap();
        sl.read_to_end(&mut sl_data).unwrap();
    }
    let strings = parse_string_lookup(&sl_data);
    eprintln!("Strings: {}", strings.len());

    // Step 1: Read location tag order from compatibility.locations list
    // These are the quoted strings at the start of the binary
    eprintln!("Reading location tags from compatibility section...");
    let location_tags = read_location_tags_from_compat(&gamestate_data);
    eprintln!("Location tags: {}", location_tags.len());

    // Step 2: Build CountryId -> country tag map
    // The countries database has 2263 entries. Each country has a "tag" field
    // whose value is a LOOKUP token mapping to a 3-letter uppercase string.
    // We'll find this by looking for LOOKUP values that are 3-letter uppercase strings
    // appearing ~2263 times in a specific context.
    eprintln!("Building CountryId -> tag map...");
    let country_id_to_tag = build_country_id_map(&gamestate_data, &strings);
    eprintln!("Countries found: {}", country_id_to_tag.len());
    // Sample check
    for id in [1, 2, 3, 4, 5] {
        if let Some(tag) = country_id_to_tag.get(&id) {
            eprintln!("  CountryId {id} = {tag}");
        }
    }

    // Step 3: Analyze depth structure to find correct location depth
    eprintln!("Analyzing OWNER_FIELD depth structure...");
    analyze_owner_field_depths(&gamestate_data);

    // Step 4: Extract location ownership
    // Scan for objects containing OWNER_FIELD. Track position within parent array.
    // Map position -> location_tags[position] and value -> country tag.
    eprintln!("Extracting location ownership...");
    let ownership = extract_location_ownership(
        &gamestate_data,
        &strings,
        &location_tags,
        &country_id_to_tag,
    );
    eprintln!("Ownership mappings: {}", ownership.len());

    // Sample checks
    let checks = ["stockholm", "hangzhou", "delhi", "paris", "london", "beijing"];
    eprintln!("\nSample:");
    for tag in &checks {
        let owner = ownership.get(*tag).map(|s| s.as_str()).unwrap_or("(none)");
        eprintln!("  {tag} -> {owner}");
    }

    // Write output
    let out_path = "/home/xiaoshihou/Playground/github/daboyi/assets/ownership.tsv";
    let mut out = File::create(out_path).expect("Failed to create output");
    writeln!(out, "location_tag\towner_tag").unwrap();
    let mut sorted: Vec<_> = ownership.iter().collect();
    sorted.sort_by_key(|(k, _)| k.as_str());
    for (loc, owner) in &sorted {
        writeln!(out, "{loc}\t{owner}").unwrap();
    }
    eprintln!("Written {out_path}");
}

/// Read the location tag list (compatibility.locations section at start of file)
/// The tags are quoted strings at depth >= 2, forming the first large string list.
fn read_location_tags_from_compat(data: &[u8]) -> Vec<String> {
    let mut reader = TokenReader::from_slice(data);
    let mut tags: Vec<String> = Vec::new();
    let mut depth = 0i32;

    loop {
        match reader.next() {
            Ok(Some(token)) => match token {
                Token::Open => {
                    depth += 1;
                }
                Token::Close => {
                    if depth > 0 {
                        depth -= 1;
                    }
                    // Stop once we exit depth 1 (after collecting the first large list)
                    if depth == 0 && tags.len() > 1000 {
                        break;
                    }
                }
                Token::Quoted(s) | Token::Unquoted(s) if depth >= 2 => {
                    let s = s.to_string();
                    if !s.is_empty() {
                        tags.push(s);
                    }
                }
                _ => {}
            },
            Ok(None) | Err(_) => break,
        }
    }
    tags
}

/// Build a map from CountryId (u32) -> country tag (3-letter string)
/// Strategy: scan for a consecutive run of objects (at depth >= 2) where
/// the first LOOKUP value inside each object maps to a 3-letter uppercase string.
/// The N-th such object (0-indexed) corresponds to CountryId N+1.
fn build_country_id_map(data: &[u8], strings: &[String]) -> HashMap<u32, String> {
    let mut reader = TokenReader::from_slice(data);
    let mut depth = 0i32;
    let mut map: HashMap<u32, String> = HashMap::new();

    // Strategy: track LOOKUP values after OPEN at depth=2
    // We're looking for the countries array where each object starts with
    // the country tag as a LOOKUP value
    let mut obj_index = 0u32; // within current parent container
    let mut parent_depth = -1i32;
    let mut in_countries_section = false;
    let mut country_count = 0u32;

    // Alternative: just find the longest run of objects where first LOOKUP = 3-letter string
    // For simplicity, scan for patterns where we see many consecutive
    // sequences: OPEN ... LOOKUP(3-letter) ... CLOSE at the same depth

    // Track: for each depth, how many objects we've seen with a 3-letter LOOKUP tag
    let mut depth_obj_counts: Vec<u32> = vec![0; 20];
    let mut depth_in_obj: Vec<bool> = vec![false; 20];
    let mut depth_first_lookup: Vec<Option<u32>> = vec![None; 20]; // lookup idx for first LOOKUP seen
    let mut depth_country_ids: Vec<HashMap<u32, String>> = vec![HashMap::new(); 20];

    // Just scan ALL objects at all depths for their first LOOKUP value
    // If it's a 3-letter uppercase string, record it as a potential country tag
    // The countries database should have ~2263 consecutive such objects at the same depth

    let mut obj_at_depth: Vec<Vec<String>> = vec![vec![]; 10]; // depth -> [country tags]
    let mut obj_start_depth: i32 = -1;

    // Collect all objects at ANY depth where the first LOOKUP inside = 3-letter uppercase string
    // The countries database will show as a long consecutive run of such objects at depth 3
    // We scan for objects at each depth and find the longest run of country-tag objects
    let mut depth_obj_tags: HashMap<i32, Vec<(u32, String)>> = HashMap::new();
    // For each depth: list of (obj_index, first_3letter_tag)

    let mut reader = TokenReader::from_slice(data);
    let mut depth = 0i32;
    let mut obj_index_by_depth: Vec<u32> = vec![0; 15];
    let mut first_tag_by_depth: Vec<Option<String>> = vec![None; 15];

    loop {
        match reader.next() {
            Ok(Some(token)) => match token {
                Token::Open => {
                    depth += 1;
                    let d = depth as usize;
                    if d < 15 {
                        first_tag_by_depth[d] = None;
                        if d + 1 < 15 {
                            obj_index_by_depth[d + 1] = 0;
                        }
                    }
                }
                Token::Close => {
                    let d = depth as usize;
                    if d < 15 {
                        if let Some(tag) = first_tag_by_depth[d].take() {
                            depth_obj_tags
                                .entry(depth)
                                .or_default()
                                .push((obj_index_by_depth[d], tag));
                        }
                        obj_index_by_depth[d] = obj_index_by_depth[d].wrapping_add(1);
                    }
                    if depth > 0 {
                        depth -= 1;
                    }
                }
                Token::Lookup(idx) => {
                    let d = depth as usize;
                    if d < 15 && first_tag_by_depth[d].is_none() {
                        if let Some(s) = strings.get(idx as usize) {
                            if s.len() == 3 && s.chars().all(|c| c.is_ascii_uppercase()) {
                                first_tag_by_depth[d] = Some(s.clone());
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(None) | Err(_) => break,
        }
    }

    // Find the best depth with longest consecutive country run
    let mut best_depth = -1i32;
    let mut best_len = 0usize;
    let mut best_tags: Vec<String> = Vec::new();

    for (d, tags) in &depth_obj_tags {
        eprintln!("  depth {d}: {} objects with 3-letter LOOKUP", tags.len());
        if tags.len() < 100 {
            continue;
        }
        // Find longest consecutive run
        let mut cur_start = 0usize;
        let mut cur_len = 1usize;
        let mut b_start = 0usize;
        let mut b_len = 1usize;
        for i in 1..tags.len() {
            if tags[i].0 == tags[i - 1].0 + 1 {
                cur_len += 1;
            } else {
                if cur_len > b_len {
                    b_len = cur_len;
                    b_start = cur_start;
                }
                cur_start = i;
                cur_len = 1;
            }
        }
        if cur_len > b_len {
            b_len = cur_len;
            b_start = cur_start;
        }
        eprintln!("    -> longest consecutive run: {b_len} at offset {b_start}");
        if b_len > best_len {
            best_len = b_len;
            best_depth = *d;
            best_tags = tags[b_start..b_start + b_len].iter().map(|(_, t)| t.clone()).collect();
        }
    }

    eprintln!("  Best: depth {best_depth}, {best_len} countries");
    for (seq, tag) in best_tags.iter().enumerate().take(5) {
        eprintln!("    CountryId {} = {}", seq + 1, tag);
    }

    // Build the map: CountryId 1..=N -> tag
    for (seq, tag) in best_tags.iter().enumerate() {
        map.insert((seq + 1) as u32, tag.clone());
    }

    map
}

/// Analyze OWNER_FIELD depth structure to understand which depth contains location objects.
/// For each OWNER_FIELD occurrence, record the obj_index at each depth level 3-8.
fn analyze_owner_field_depths(data: &[u8]) {
    let mut reader = TokenReader::from_slice(data);
    let mut depth = 0i32;
    let mut obj_index: Vec<u32> = vec![0; 30];
    let mut after_equal = false;
    let mut last_field_id: Option<u16> = None;

    struct Obs {
        owner_id: u32,
        depth: i32,
        indices: [u32; 10],
    }
    let mut observations: Vec<Obs> = Vec::new();

    loop {
        match reader.next() {
            Ok(Some(token)) => match token {
                Token::Open => {
                    depth += 1;
                    let d = depth as usize;
                    if d + 1 < 30 {
                        obj_index[d + 1] = 0;
                    }
                    after_equal = false;
                    last_field_id = None;
                }
                Token::Close => {
                    let d = depth as usize;
                    if d < 30 {
                        obj_index[d] = obj_index[d].wrapping_add(1);
                    }
                    if depth > 0 {
                        depth -= 1;
                    }
                    after_equal = false;
                    last_field_id = None;
                }
                Token::Equal => {
                    after_equal = true;
                }
                Token::Id(id) => {
                    if !after_equal {
                        last_field_id = Some(id);
                    } else {
                        after_equal = false;
                    }
                }
                Token::U32(v) if after_equal && last_field_id == Some(OWNER_FIELD) => {
                    let mut indices = [0u32; 10];
                    for (k, idx) in indices.iter_mut().enumerate() {
                        *idx = if k < 30 { obj_index[k] } else { 0 };
                    }
                    observations.push(Obs { owner_id: v, depth, indices });
                    after_equal = false;
                    last_field_id = None;
                }
                Token::I32(v) if after_equal && last_field_id == Some(OWNER_FIELD) && v > 0 => {
                    let v = v as u32;
                    let mut indices = [0u32; 10];
                    for (k, idx) in indices.iter_mut().enumerate() {
                        *idx = if k < 30 { obj_index[k] } else { 0 };
                    }
                    observations.push(Obs { owner_id: v, depth, indices });
                    after_equal = false;
                    last_field_id = None;
                }
                // FIXED5 tokens come through as F64 raw bytes
                Token::F64(bytes) if after_equal && last_field_id == Some(OWNER_FIELD) => {
                    // Interpret raw bytes as i64 (FIXED5 stores integer × 1 in compact form)
                    let raw = i64::from_le_bytes(bytes);
                    if raw > 0 && raw <= 3000 {
                        let mut indices = [0u32; 10];
                        for (k, idx) in indices.iter_mut().enumerate() {
                            *idx = if k < 30 { obj_index[k] } else { 0 };
                        }
                        observations.push(Obs { owner_id: raw as u32, depth, indices });
                    }
                    after_equal = false;
                    last_field_id = None;
                }
                Token::U32(_) | Token::I32(_) | Token::F64(_) => {
                    after_equal = false;
                    last_field_id = None;
                }
                _ => {
                    after_equal = false;
                }
            },
            Ok(None) | Err(_) => break,
        }
    }

    eprintln!("  Total OWNER_FIELD observations: {}", observations.len());

    // For each depth level, analyze obj_index[level] across observations where obs.depth==D
    let mut by_depth: HashMap<i32, Vec<&Obs>> = HashMap::new();
    for obs in &observations {
        by_depth.entry(obs.depth).or_default().push(obs);
    }
    let mut depths: Vec<i32> = by_depth.keys().cloned().collect();
    depths.sort();

    for d in &depths {
        let obs_at_d = &by_depth[d];
        eprintln!("  Depth {d}: {} observations", obs_at_d.len());
        // Print obj_index at each ancestor level to find sequential one
        for level in 3..9usize {
            let indices: Vec<u32> = obs_at_d.iter().map(|o| o.indices[level]).collect();
            let max_idx = indices.iter().max().copied().unwrap_or(0);
            let unique: std::collections::HashSet<u32> = indices.iter().cloned().collect();
            if max_idx > 100 && max_idx < 30000 {
                eprintln!("    obj_index[{level}]: max={max_idx}, unique={}, first=[{},{},{},{},{}]",
                    unique.len(),
                    indices.get(0).unwrap_or(&0),
                    indices.get(1).unwrap_or(&0),
                    indices.get(2).unwrap_or(&0),
                    indices.get(3).unwrap_or(&0),
                    indices.get(4).unwrap_or(&0));
            }
        }
        // Print first 5 observations
        for obs in obs_at_d.iter().take(5) {
            eprintln!("    obs: owner={} indices={:?}", obs.owner_id, &obs.indices[..8]);
        }
    }
}


/// Scans location objects which are keyed with an integer (loc_id) in the binary save,
/// finds OWNER_FIELD inside them, and maps location_tags[loc_id] -> country_tag.
///
/// Binary pattern: U32(loc_id) EQUAL OPEN { ... OWNER_FIELD EQUAL U32(country_id) ... } CLOSE
/// The OWNER_FIELD may appear at sub-depths within the location object.
/// We track integer keys at each depth and propagate the first OWNER_FIELD value found
/// within the object up to the keyed ancestor.
fn extract_location_ownership(
    data: &[u8],
    _strings: &[String],
    location_tags: &[String],
    country_id_to_tag: &HashMap<u32, String>,
) -> HashMap<String, String> {
    let mut ownership: HashMap<String, String> = HashMap::new();
    let mut reader = TokenReader::from_slice(data);
    let mut depth = 0i32;

    // int_key_at_depth[d] = Some(loc_id) if depth d was opened with a U32 integer key
    let mut int_key_at_depth: Vec<Option<u32>> = vec![None; 30];
    // owner_at_depth[d] = first CountryId found within the depth-d subtree
    let mut owner_at_depth: Vec<Option<u32>> = vec![None; 30];

    let mut after_equal = false;
    let mut last_field_id: Option<u16> = None;
    // The last U32 token seen NOT after_equal (potential integer key)
    let mut pending_int_key: Option<u32> = None;

    loop {
        match reader.next() {
            Ok(Some(token)) => match token {
                Token::Open => {
                    depth += 1;
                    let d = depth as usize;
                    if d < 30 {
                        // If we were after_equal with a pending U32 key, this object is integer-keyed
                        int_key_at_depth[d] = if after_equal { pending_int_key } else { None };
                        owner_at_depth[d] = None;
                    }
                    after_equal = false;
                    last_field_id = None;
                    pending_int_key = None;
                }
                Token::Close => {
                    let d = depth as usize;
                    if d < 30 {
                        if let Some(loc_id) = int_key_at_depth[d].take() {
                            // loc_id range check: valid location IDs are 10..=30000
                            if loc_id >= 10 && loc_id < 30000 {
                                if let Some(owner_id) = owner_at_depth[d].take() {
                                    if let Some(loc_name) = location_tags.get(loc_id as usize) {
                                        if !loc_name.is_empty() {
                                            if let Some(country_tag) =
                                                country_id_to_tag.get(&owner_id)
                                            {
                                                ownership
                                                    .entry(loc_name.clone())
                                                    .or_insert_with(|| country_tag.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // Propagate any found owner to parent depths with valid int keys
                        if let Some(owner_id) = owner_at_depth[d].take() {
                            for pd in (1..d).rev() {
                                if let Some(key) = int_key_at_depth[pd] {
                                    if key >= 10 && key < 30000 && owner_at_depth[pd].is_none() {
                                        owner_at_depth[pd] = Some(owner_id);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if depth > 0 {
                        depth -= 1;
                    }
                    after_equal = false;
                    last_field_id = None;
                    pending_int_key = None;
                }
                Token::Equal => {
                    after_equal = true;
                }
                Token::Id(id) => {
                    if !after_equal {
                        last_field_id = Some(id);
                        pending_int_key = None;
                    } else {
                        after_equal = false;
                        last_field_id = None;
                    }
                }
                Token::U32(v) => {
                    if after_equal {
                        if last_field_id == Some(OWNER_FIELD) && v >= 1 && v <= 3000 {
                            // OWNER_FIELD = CountryId; propagate to nearest ancestor with valid loc_id
                            let d = depth as usize;
                            for pd in (1..=d).rev() {
                                if let Some(key) = int_key_at_depth[pd] {
                                    if key >= 10 && key < 30000 && owner_at_depth[pd].is_none() {
                                        owner_at_depth[pd] = Some(v);
                                        break;
                                    }
                                }
                            }
                        }
                        after_equal = false;
                        last_field_id = None;
                        pending_int_key = None;
                    } else {
                        // Could be an integer key for the next object
                        pending_int_key = Some(v);
                        last_field_id = None;
                    }
                }
                Token::I32(v) => {
                    if after_equal && last_field_id == Some(OWNER_FIELD) && v >= 1 && v <= 3000 {
                        let d = depth as usize;
                        for pd in (1..=d).rev() {
                            if let Some(key) = int_key_at_depth[pd] {
                                if key >= 10 && key < 30000 && owner_at_depth[pd].is_none() {
                                    owner_at_depth[pd] = Some(v as u32);
                                    break;
                                }
                            }
                        }
                    }
                    after_equal = false;
                    last_field_id = None;
                    pending_int_key = None;
                }
                Token::F64(bytes) => {
                    if after_equal && last_field_id == Some(OWNER_FIELD) {
                        // FIXED5 token: raw bytes as i64
                        let raw = i64::from_le_bytes(bytes);
                        if raw >= 1 && raw <= 3000 {
                            let d = depth as usize;
                            for pd in (1..=d).rev() {
                                if let Some(key) = int_key_at_depth[pd] {
                                    if key >= 10 && key < 30000 && owner_at_depth[pd].is_none() {
                                        owner_at_depth[pd] = Some(raw as u32);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    after_equal = false;
                    last_field_id = None;
                    pending_int_key = None;
                }
                _ => {
                    if after_equal {
                        after_equal = false;
                        last_field_id = None;
                    }
                    pending_int_key = None;
                }
            },
            Ok(None) | Err(_) => break,
        }
    }

    eprintln!("  Ownership mappings collected: {}", ownership.len());
    for tag in &["stockholm", "paris", "hangzhou", "delhi", "beijing", "london"] {
        let owner = ownership.get(*tag).map(|s| s.as_str()).unwrap_or("(none)");
        eprintln!("    {tag} -> {owner}");
    }

    ownership
}

