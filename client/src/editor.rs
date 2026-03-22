/// Map editor core: province coloring resources and save/load.
use bevy::prelude::*;
use shared::{AdminArea, ColoringFile, EditorCountry};
use std::collections::HashMap;

// ── Resources ─────────────────────────────────────────────────────────────────

/// Province-level assignments: province_id → country_tag.
#[derive(Resource, Default, Clone)]
pub struct MapColoring {
    pub assignments: HashMap<u32, String>,
}

/// All countries defined in the current editing session.
#[derive(Resource, Default, Clone)]
pub struct EditorCountries(pub Vec<EditorCountry>);

/// All administrative areas (ADM1, ADM2, …) defined in the session.
#[derive(Resource, Default, Clone)]
pub struct AdminAreas(pub Vec<AdminArea>);

/// Province-level admin area assignments: province_id → admin_area_id.
#[derive(Resource, Default, Clone)]
pub struct AdminAssignments(pub HashMap<u32, u32>);

/// Which country is currently selected as the country-level paint target.
#[derive(Resource, Default)]
pub struct ActiveCountry(pub Option<String>);

/// Which admin area is currently selected as the paint target (overrides country).
#[derive(Resource, Default)]
pub struct ActiveArea(pub Option<u32>);

/// Monotonically increasing ID allocator for new admin areas.
#[derive(Resource, Default)]
pub struct NextAreaId(pub u32);

// ── Save / Load ───────────────────────────────────────────────────────────────

pub const SAVE_PATH: &str = "coloring.json";

pub fn save_coloring(
    coloring: &MapColoring,
    countries: &EditorCountries,
    admin_areas: &AdminAreas,
    admin_assignments: &AdminAssignments,
) {
    let file = ColoringFile {
        countries: countries.0.clone(),
        assignments: coloring.assignments.clone(),
        admin_areas: admin_areas.0.clone(),
        admin_assignments: admin_assignments.0.clone(),
    };
    match serde_json::to_string_pretty(&file) {
        Ok(json) => {
            if let Err(e) = std::fs::write(SAVE_PATH, json) {
                eprintln!("保存失败: {e}");
            } else {
                println!("已保存到 {SAVE_PATH}");
            }
        }
        Err(e) => eprintln!("序列化失败: {e}"),
    }
}

pub fn load_coloring(
    coloring: &mut MapColoring,
    countries: &mut EditorCountries,
    admin_areas: &mut AdminAreas,
    admin_assignments: &mut AdminAssignments,
    next_id: &mut NextAreaId,
) {
    match std::fs::read_to_string(SAVE_PATH) {
        Ok(json) => match serde_json::from_str::<ColoringFile>(&json) {
            Ok(file) => {
                coloring.assignments = file.assignments;
                countries.0 = file.countries;
                admin_areas.0 = file.admin_areas;
                admin_assignments.0 = file.admin_assignments;
                // Reset allocator to max existing ID + 1.
                next_id.0 = admin_areas.0.iter().map(|a| a.id + 1).max().unwrap_or(0);
                println!("已从 {SAVE_PATH} 加载");
            }
            Err(e) => eprintln!("解析失败: {e}"),
        },
        Err(e) => eprintln!("无法读取文件: {e}"),
    }
}
