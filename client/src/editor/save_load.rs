//! 着色文件保存/加载

use bevy::prelude::*;
use std::fs;

use crate::editor::{AdminAreas, AdminMap, Countries, CountryMap, NextAdminId};
use shared::ColoringFile;

const COLORING_FILE: &str = "assets/coloring.json";

/// 从 JSON 文件加载着色数据
pub fn load_coloring(commands: &mut Commands) {
    let Ok(json) = fs::read_to_string(COLORING_FILE) else {
        bevy::log::warn!(target: "daboyi::startup", "未找到着色文件，使用空数据");
        return;
    };

    let file: ColoringFile = match serde_json::from_str(&json) {
        Ok(f) => f,
        Err(e) => {
            bevy::log::error!(target: "daboyi::startup", "解析着色文件失败：{e}");
            return;
        }
    };

    // 更新下一个行政区 ID
    let max_id = file.admin_areas.iter().map(|a| a.id).max().unwrap_or(0);

    commands.insert_resource(Countries(file.countries));
    commands.insert_resource(AdminAreas(file.admin_areas));
    commands.insert_resource(CountryMap(file.assignments));
    commands.insert_resource(AdminMap(file.admin_assignments));
    commands.insert_resource(NextAdminId(max_id + 1));

    bevy::log::info!(target: "daboyi::startup", "已加载着色数据从 {COLORING_FILE}");
}
