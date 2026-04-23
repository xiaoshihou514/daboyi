//! 着色文件保存/加载

use bevy::prelude::*;
use std::fs;
use std::path::Path;

use crate::editor::{
    ActiveAdmin, ActiveCountry, AdminAreas, AdminMap, Countries, CountryMap, NextAdminId,
};
use crate::map::BorderVersion;
use crate::map::ColoringVersion;
use shared::ColoringFile;

const COLORING_FILE: &str = "assets/coloring.json";

#[derive(Event)]
pub struct LoadColoringEvent(pub String);

#[derive(Event)]
pub struct SaveColoringEvent(pub String);

fn load_coloring_from_path(path: &Path) -> Result<ColoringFile, String> {
    let json = fs::read_to_string(path).map_err(|e| format!("读取着色文件失败：{e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("解析着色文件失败：{e}"))
}

fn apply_coloring(commands: &mut Commands, file: ColoringFile) {
    let max_id = file.admin_areas.iter().map(|a| a.id).max().unwrap_or(0);

    commands.insert_resource(Countries(file.countries));
    commands.insert_resource(AdminAreas(file.admin_areas));
    commands.insert_resource(CountryMap(file.assignments));
    commands.insert_resource(AdminMap(file.admin_assignments));
    commands.insert_resource(NextAdminId(max_id + 1));
}

fn current_coloring_file(
    countries: &Countries,
    admin_areas: &AdminAreas,
    country_map: &CountryMap,
    admin_map: &AdminMap,
) -> ColoringFile {
    ColoringFile {
        countries: countries.0.clone(),
        assignments: country_map.0.clone(),
        admin_areas: admin_areas.0.clone(),
        admin_assignments: admin_map.0.clone(),
    }
}

/// 从 JSON 文件加载着色数据
pub fn load_coloring(commands: &mut Commands) {
    let file = match load_coloring_from_path(Path::new(COLORING_FILE)) {
        Ok(file) => file,
        Err(error) => {
            bevy::log::warn!(target: "daboyi::startup", "{error}；使用空数据");
            return;
        }
    };
    apply_coloring(commands, file);
    bevy::log::info!(target: "daboyi::startup", "已加载着色数据从 {COLORING_FILE}");
}

pub fn handle_load_coloring(
    mut commands: Commands,
    mut events: EventReader<LoadColoringEvent>,
    mut active_country: ResMut<ActiveCountry>,
    mut active_admin: ResMut<ActiveAdmin>,
    mut coloring_version: ResMut<ColoringVersion>,
    mut border_version: ResMut<BorderVersion>,
) {
    for event in events.read() {
        let path = Path::new(&event.0);
        let file = match load_coloring_from_path(path) {
            Ok(file) => file,
            Err(error) => {
                bevy::log::error!(target: "daboyi::editor", "{}: {}", path.display(), error);
                continue;
            }
        };
        apply_coloring(&mut commands, file);
        active_country.0 = None;
        active_admin.0 = None;
        coloring_version.0 += 1;
        border_version.0 += 1;
        bevy::log::info!(target: "daboyi::editor", "已加载着色数据从 {}", path.display());
    }
}

pub fn handle_save_coloring(
    mut events: EventReader<SaveColoringEvent>,
    countries: Res<Countries>,
    admin_areas: Res<AdminAreas>,
    country_map: Res<CountryMap>,
    admin_map: Res<AdminMap>,
) {
    for event in events.read() {
        let path = Path::new(&event.0);
        let file = current_coloring_file(&countries, &admin_areas, &country_map, &admin_map);
        let json = match serde_json::to_string_pretty(&file) {
            Ok(json) => json,
            Err(error) => {
                bevy::log::error!(target: "daboyi::editor", "序列化着色文件失败：{}", error);
                continue;
            }
        };
        match fs::write(path, json) {
            Ok(()) => {
                bevy::log::info!(target: "daboyi::editor", "已保存着色数据到 {}", path.display())
            }
            Err(error) => {
                bevy::log::error!(target: "daboyi::editor", "{}: 写入着色文件失败：{}", path.display(), error)
            }
        }
    }
}
