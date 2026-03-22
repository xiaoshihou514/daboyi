/// Map editor core: province coloring resources and save/load.
use bevy::prelude::*;
use shared::{ColoringFile, EditorCountry};
use std::collections::HashMap;

// ── Resources ─────────────────────────────────────────────────────────────────

/// Stores the current user-defined coloring: province_id → country_tag.
#[derive(Resource, Default, Clone)]
pub struct MapColoring {
    pub assignments: HashMap<u32, String>,
}

/// All countries defined in the current editing session.
#[derive(Resource, Default, Clone)]
pub struct EditorCountries(pub Vec<EditorCountry>);

/// Which country is currently selected as the "paint" target.
#[derive(Resource, Default)]
pub struct ActiveCountry(pub Option<String>);

// ── Save / Load ───────────────────────────────────────────────────────────────

const SAVE_PATH: &str = "coloring.json";

pub fn save_coloring(coloring: &MapColoring, countries: &EditorCountries) {
    let file = ColoringFile {
        countries: countries.0.clone(),
        assignments: coloring.assignments.clone(),
    };
    match serde_json::to_string_pretty(&file) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SAVE_PATH, json) {
                eprintln!("Failed to save coloring: {e}");
            } else {
                println!("Saved coloring to {SAVE_PATH}");
            }
        }
        Err(e) => eprintln!("Failed to serialize coloring: {e}"),
    }
}

pub fn load_coloring(
    coloring: &mut MapColoring,
    countries: &mut EditorCountries,
) {
    match std::fs::read_to_string(SAVE_PATH) {
        Ok(json) => match serde_json::from_str::<ColoringFile>(&json) {
            Ok(file) => {
                coloring.assignments = file.assignments;
                countries.0 = file.countries;
                println!("Loaded coloring from {SAVE_PATH}");
            }
            Err(e) => eprintln!("Failed to parse coloring: {e}"),
        },
        Err(e) => eprintln!("No coloring file to load: {e}"),
    }
}
