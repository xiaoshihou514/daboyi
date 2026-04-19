//! egui-based UI implementation for the map editor

use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_egui::{egui, EguiContexts};
use shared::conv::{u32_to_usize, unit_f32_to_u8, u8_to_unit_f32, usize_to_f32};

use crate::editor::{
    ActiveAdmin, ActiveCountry, AdminAreas, BrushTool, Countries, CountryMap, AdminMap,
    NextAdminId,
};
use crate::map::{
    BorderVersion, ColoringVersion, MapMode, MapResource, ProvinceNames,
    SelectedProvince,
};
use crate::ui::UiInputBlock;
use shared::{AdminArea, EditorCountry};

/// UI state stored in Local
#[derive(Default)]
pub struct UiState {
    pub show_rename_dialog: bool,
    pub rename_target: RenameTarget,
    pub rename_buffer: String,
    pub show_new_country_dialog: bool,
    pub new_country_name: String,
    pub show_new_admin_dialog: bool,
    pub new_admin_name: String,
    pub show_delete_confirm: Option<DeleteTarget>,
}

#[derive(Default, Clone)]
pub enum RenameTarget {
    #[default]
    None,
    Country(String),
    Admin(u32),
}

#[derive(Clone)]
pub enum DeleteTarget {
    Country(String),
    Admin(u32),
}

#[derive(SystemParam)]
pub(crate) struct EditorUiData<'w, 's> {
    active_country: ResMut<'w, ActiveCountry>,
    active_admin: ResMut<'w, ActiveAdmin>,
    countries: ResMut<'w, Countries>,
    admin_areas: ResMut<'w, AdminAreas>,
    country_map: ResMut<'w, CountryMap>,
    admin_map: ResMut<'w, AdminMap>,
    next_admin_id: ResMut<'w, NextAdminId>,
    _marker: std::marker::PhantomData<&'s ()>,
}

#[derive(SystemParam)]
pub(crate) struct UiRuntime<'w, 's> {
    map_mode: ResMut<'w, MapMode>,
    coloring_version: ResMut<'w, ColoringVersion>,
    border_version: ResMut<'w, BorderVersion>,
    ui_input_block: ResMut<'w, UiInputBlock>,
    _marker: std::marker::PhantomData<&'s ()>,
}

/// Main egui UI system
pub fn egui_ui_system(
    mut contexts: EguiContexts,
    mut editor: EditorUiData,
    mut brush: ResMut<BrushTool>,
    selected: Res<SelectedProvince>,
    map: Option<Res<MapResource>>,
    province_names: Option<Res<ProvinceNames>>,
    mut ui_state: Local<UiState>,
    mut runtime: UiRuntime,
) {
    let ctx = contexts.ctx_mut();
    let pointer_pos = ctx.pointer_latest_pos();
    let mut ui_blocks_pointer = false;

    // === Left Panel ===
    let left_panel = egui::SidePanel::left("left_panel")
        .min_width(250.0)
        .max_width(400.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("国家与行政区");
            ui.separator();

            // Country list
            ui.heading("国家列表");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    let mut next_country_selection: Option<String> = None;

                    for country in &mut editor.countries.0 {
                        let tag = country.tag.clone();
                        let name = country.name.clone();
                        let is_selected = editor.active_country.0.as_ref() == Some(&tag);

                        ui.horizontal(|ui| {
                            let response =
                                ui.selectable_label(is_selected, egui::RichText::new(&name));

                            if response.clicked() {
                                next_country_selection = Some(tag.clone());
                            }

                            let rename_tag = tag.clone();
                            let rename_name = name.clone();
                            let delete_tag = tag.clone();
                            response.context_menu(|ui| {
                                if ui.button("重命名").clicked() {
                                    ui_state.show_rename_dialog = true;
                                    ui_state.rename_target =
                                        RenameTarget::Country(rename_tag.clone());
                                    ui_state.rename_buffer = rename_name.clone();
                                    ui.close_menu();
                                }
                                if ui.button("删除").clicked() {
                                    ui_state.show_delete_confirm =
                                        Some(DeleteTarget::Country(delete_tag.clone()));
                                    ui.close_menu();
                                }
                            });

                            let mut color = color32_from_rgba(country.color);
                            let color_response = ui.color_edit_button_srgba(&mut color);
                            if color_response.changed() {
                                country.color = rgba_from_color32(color);
                                runtime.coloring_version.0 += 1;
                            }
                        });
                    }

                    if let Some(tag) = next_country_selection {
                        editor.active_country.0 = Some(tag);
                        editor.active_admin.0 = None;
                    }
                });

            ui.separator();

            // Admin area list
            ui.heading("行政区列表");
            
            if let Some(ref country_tag) = editor.active_country.0 {
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let top_level: Vec<&AdminArea> = editor.admin_areas.0
                            .iter()
                            .filter(|a| &a.country_tag == country_tag && a.parent_id.is_none())
                            .collect();

                        for area in &top_level {
                            show_admin_area_tree(
                                ui,
                                area,
                                &editor.admin_areas.0,
                                &mut editor.active_admin.0,
                                &mut ui_state,
                                0,
                            );
                        }
                    });
            } else {
                ui.label("请先选择一个国家");
            }

            ui.separator();

            // Add buttons
            ui.horizontal(|ui| {
                if ui.button("新建国家").clicked() {
                    ui_state.show_new_country_dialog = true;
                    ui_state.new_country_name = String::new();
                }

                let adm_btn_label = if editor.active_admin.0.is_some() {
                    "新建子区域"
                } else {
                    "新建行政区"
                };
                if ui.button(adm_btn_label).clicked() && editor.active_country.0.is_some() {
                    ui_state.show_new_admin_dialog = true;
                    ui_state.new_admin_name = String::new();
                }
            });
        });
    if let Some(pos) = pointer_pos {
        ui_blocks_pointer |= left_panel.response.rect.contains(pos);
    }

    // === Right Panel ===
    let right_panel = egui::SidePanel::right("right_panel")
        .min_width(200.0)
        .max_width(300.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("工具");
            ui.separator();

            // Brush
            ui.heading("刷子工具");
            ui.checkbox(&mut brush.enabled, "启用刷子");
            
            if brush.enabled {
                ui.add(
                    egui::Slider::new(&mut brush.radius, 8.0..=240.0)
                        .text("半径")
                        .suffix(" px")
                );
                ui.label(format!("当前半径：{:.0}px", brush.radius));
                ui.label("按住 Shift + 滚轮调整刷子大小");
                ui.checkbox(&mut brush.eraser_mode, "橡皮擦模式 (E)");
                if brush.eraser_mode {
                    ui.colored_label(egui::Color32::RED, "⚠ 橡皮擦：点击移除归属");
                }
            }

            ui.separator();

            // Map mode
            ui.heading("地图模式");
            ui.horizontal(|ui| {
                if ui.button("省份").clicked() { *runtime.map_mode = MapMode::Province; }
                if ui.button("地形").clicked() { *runtime.map_mode = MapMode::Terrain; }
                if ui.button("政治").clicked() { *runtime.map_mode = MapMode::Political; }
                
                let mode_str = match *runtime.map_mode {
                    MapMode::Province => "省份",
                    MapMode::Terrain => "地形",
                    MapMode::Political => "政治",
                };
                ui.label(format!("当前：{}", mode_str));
            });

            if let Some(area_id) = editor.active_admin.0 {
                ui.separator();
                ui.heading("行政区颜色");
                if let Some(area) = editor
                    .admin_areas
                    .0
                    .iter_mut()
                    .find(|area| area.id == area_id)
                {
                    let mut color =
                        color32_from_rgba(area.color.unwrap_or([0.55, 0.55, 0.55, 1.0]));
                    if ui.color_edit_button_srgba(&mut color).changed() {
                        area.color = Some(rgba_from_color32(color));
                        runtime.coloring_version.0 += 1;
                    }
                    if area.color.is_some() && ui.button("清除自定义颜色").clicked() {
                        area.color = None;
                        runtime.coloring_version.0 += 1;
                    }
                }
            }

            ui.separator();

            // Save/Load
            if ui.button("保存").clicked() {
                println!("保存功能需要实现事件系统");
            }
            if ui.button("加载").clicked() {
                println!("加载功能需要实现事件系统");
            }
        });
    if let Some(pos) = pointer_pos {
        ui_blocks_pointer |= right_panel.response.rect.contains(pos);
    }

    // === Bottom Panel ===
    let bottom_panel = egui::TopBottomPanel::bottom("bottom_panel")
        .min_height(40.0)
        .max_height(80.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(prov_id) = selected.0 {
                    ui.label(format!("选中省份 ID: {}", prov_id));
                    
                    if let Some(ref map) = map {
                        let prov_index = u32_to_usize(prov_id);
                        if prov_index < map.0.provinces.len() {
                            let prov = &map.0.provinces[prov_index];
                            let display_name = province_names
                                .as_ref()
                                .and_then(|names| names.0.get(&prov.tag.to_lowercase()))
                                .cloned()
                                .unwrap_or_else(|| prov.name.clone());
                            ui.label(format!("名称：{}", display_name));
                            
                            if let Some(tag) = editor.country_map.0.get(&prov_id) {
                                if let Some(country) =
                                    editor.countries.0.iter().find(|c| &c.tag == tag)
                                {
                                    ui.label(format!("国家：{}", country.name));
                                }
                            }
                            
                            if let Some(area_id) = editor.admin_map.0.get(&prov_id) {
                                if let Some(area) =
                                    editor.admin_areas.0.iter().find(|a| a.id == *area_id)
                                {
                                    ui.label(format!("行政区：{}", area.name));
                                }
                            }
                        }
                    }
                } else {
                    ui.label("点击一个省份查看详情");
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("刷子：{}", if brush.enabled { "开启" } else { "关闭" }));
                });
            });
        });
    if let Some(pos) = pointer_pos {
        ui_blocks_pointer |= bottom_panel.response.rect.contains(pos);
    }

    // === Central Panel ===
    egui::CentralPanel::default()
        .frame(egui::Frame::default().fill(egui::Color32::TRANSPARENT))
        .show(ctx, |_ui| {
            // Empty - map shows through
        });

    // === Dialogs ===
    
    // Rename dialog
    if ui_state.show_rename_dialog {
        let rename_window = egui::Window::new("重命名")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("请输入新名称:");
                ui.text_edit_singleline(&mut ui_state.rename_buffer);
                
                ui.horizontal(|ui| {
                    if ui.button("确定").clicked() {
                        match &ui_state.rename_target {
                            RenameTarget::Country(tag) => {
                                if let Some(c) =
                                    editor.countries.0.iter_mut().find(|c| &c.tag == tag)
                                {
                                    c.name = ui_state.rename_buffer.clone();
                                }
                            }
                            RenameTarget::Admin(id) => {
                                if let Some(a) =
                                    editor.admin_areas.0.iter_mut().find(|a| a.id == *id)
                                {
                                    a.name = ui_state.rename_buffer.clone();
                                }
                            }
                            RenameTarget::None => {}
                        }
                        ui_state.show_rename_dialog = false;
                        ui_state.rename_target = RenameTarget::None;
                    }
                    if ui.button("取消").clicked() {
                        ui_state.show_rename_dialog = false;
                        ui_state.rename_target = RenameTarget::None;
                    }
                });
            });
        if let (Some(pos), Some(window)) = (pointer_pos, rename_window) {
            ui_blocks_pointer |= window.response.rect.contains(pos);
        }
    }

    // New country dialog
    if ui_state.show_new_country_dialog {
        let new_country_window = egui::Window::new("新建国家")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("国家名称:");
                ui.text_edit_singleline(&mut ui_state.new_country_name);
                
                ui.horizontal(|ui| {
                    if ui.button("创建").clicked() && !ui_state.new_country_name.is_empty() {
                        let idx = editor.countries.0.len();
                        let tag = format!("C{:03}", idx);
                        let hue = (usize_to_f32(idx) * 137.5) % 360.0;
                        let color = hsl_to_rgba(hue, 0.65, 0.50);
                        
                        editor.countries.0.push(EditorCountry {
                            tag: tag.clone(),
                            name: ui_state.new_country_name.clone(),
                            color,
                            capital_province: Some(0),
                        });
                        editor.active_country.0 = Some(tag);
                        editor.active_admin.0 = None;
                        
                        ui_state.show_new_country_dialog = false;
                        ui_state.new_country_name = String::new();
                    }
                    if ui.button("取消").clicked() {
                        ui_state.show_new_country_dialog = false;
                        ui_state.new_country_name = String::new();
                    }
                });
            });
        if let (Some(pos), Some(window)) = (pointer_pos, new_country_window) {
            ui_blocks_pointer |= window.response.rect.contains(pos);
        }
    }

    // New admin dialog
    if ui_state.show_new_admin_dialog {
        let new_admin_window = egui::Window::new("新建行政区")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                // Parent context hint
                if let Some(parent_id) = editor.active_admin.0 {
                    if let Some(parent) = editor.admin_areas.0.iter().find(|a| a.id == parent_id) {
                        ui.label(format!("将创建为「{}」的子区域", parent.name));
                    }
                } else {
                    ui.label("将创建为顶级行政区");
                }
                ui.label("行政区名称:");
                ui.text_edit_singleline(&mut ui_state.new_admin_name);
                
                ui.horizontal(|ui| {
                    if ui.button("创建").clicked() && !ui_state.new_admin_name.is_empty() {
                        if let Some(ref country_tag) = editor.active_country.0 {
                            let id = editor.next_admin_id.0;
                            editor.next_admin_id.0 += 1;
                            
                            editor.admin_areas.0.push(AdminArea {
                                id,
                                name: ui_state.new_admin_name.clone(),
                                country_tag: country_tag.clone(),
                                parent_id: editor.active_admin.0,
                                color: None,
                            });
                            editor.active_admin.0 = Some(id);
                            
                            ui_state.show_new_admin_dialog = false;
                            ui_state.new_admin_name = String::new();
                        }
                    }
                    if ui.button("取消").clicked() {
                        ui_state.show_new_admin_dialog = false;
                        ui_state.new_admin_name = String::new();
                    }
                });
            });
        if let (Some(pos), Some(window)) = (pointer_pos, new_admin_window) {
            ui_blocks_pointer |= window.response.rect.contains(pos);
        }
    }

    // Delete confirmation
    let delete_target = ui_state.show_delete_confirm.clone();
    if let Some(ref target) = delete_target {
        let (title, message) = match target {
            DeleteTarget::Country(tag) => {
                let name = editor.countries.0.iter()
                    .find(|c| &c.tag == tag)
                    .map(|c| c.name.as_str())
                    .unwrap_or("未知");
                ("删除国家", format!("确定要删除国家 '{}' 吗？\n这将同时删除所有相关行政区。", name))
            }
            DeleteTarget::Admin(id) => {
                let name = editor.admin_areas.0.iter()
                    .find(|a| a.id == *id)
                    .map(|a| a.name.as_str())
                    .unwrap_or("未知");
                ("删除行政区", format!("确定要删除行政区 '{}' 吗？", name))
            }
        };

        let delete_window = egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(message);
                
                ui.horizontal(|ui| {
                    if ui.button("删除").clicked() {
                        match target {
                            DeleteTarget::Country(tag) => {
                                let tag_clone = tag.clone();
                                editor.countries.0.retain(|c| c.tag != tag_clone);
                                editor
                                    .admin_areas
                                    .0
                                    .retain(|a| a.country_tag != tag_clone);
                                editor.country_map.0.retain(|_, v| v != &tag_clone);
                                if editor.active_country.0.as_ref() == Some(&tag_clone) {
                                    editor.active_country.0 = None;
                                    editor.active_admin.0 = None;
                                }
                                runtime.border_version.0 += 1;
                                runtime.coloring_version.0 += 1;
                            }
                            DeleteTarget::Admin(id) => {
                                let id_clone = *id;
                                editor.admin_areas.0.retain(|a| a.id != id_clone);
                                editor.admin_map.0.retain(|_, v| *v != id_clone);
                                if editor.active_admin.0 == Some(id_clone) {
                                    editor.active_admin.0 = None;
                                }
                                runtime.border_version.0 += 1;
                                runtime.coloring_version.0 += 1;
                            }
                        }
                        ui_state.show_delete_confirm = None;
                    }
                    if ui.button("取消").clicked() {
                        ui_state.show_delete_confirm = None;
                    }
                });
            });
        if let (Some(pos), Some(window)) = (pointer_pos, delete_window) {
            ui_blocks_pointer |= window.response.rect.contains(pos);
        }
    }

    runtime.ui_input_block.0 =
        ui_blocks_pointer || ctx.is_using_pointer() || ctx.wants_keyboard_input();
}

/// Show admin area tree recursively
fn show_admin_area_tree(
    ui: &mut egui::Ui,
    area: &AdminArea,
    all_areas: &[AdminArea],
    active_admin: &mut Option<u32>,
    ui_state: &mut UiState,
    depth: usize,
) {
    let is_selected = *active_admin == Some(area.id);
    let indent = 16.0 * (depth + 1) as f32;
    
    ui.horizontal(|ui| {
        ui.add_space(indent);
        
        let response = ui.selectable_label(
            is_selected,
            area.name.as_str()
        );

        if response.clicked() {
            *active_admin = Some(area.id);
        }

        response.context_menu(|ui| {
            if ui.button("重命名").clicked() {
                ui_state.show_rename_dialog = true;
                ui_state.rename_target = RenameTarget::Admin(area.id);
                ui_state.rename_buffer = area.name.clone();
                ui.close_menu();
            }
            if ui.button("删除").clicked() {
                ui_state.show_delete_confirm = Some(DeleteTarget::Admin(area.id));
                ui.close_menu();
            }
        });
    });

    let children: Vec<&AdminArea> = all_areas
        .iter()
        .filter(|a| a.parent_id == Some(area.id))
        .collect();

    for child in &children {
        show_admin_area_tree(ui, child, all_areas, active_admin, ui_state, depth + 1);
    }
}

/// HSL to RGBA conversion
fn hsl_to_rgba(h: f32, s: f32, l: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    [r + m, g + m, b + m, 1.0]
}

fn color32_from_rgba(rgba: [f32; 4]) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        unit_f32_to_u8(rgba[0]),
        unit_f32_to_u8(rgba[1]),
        unit_f32_to_u8(rgba[2]),
        unit_f32_to_u8(rgba[3]),
    )
}

fn rgba_from_color32(color: egui::Color32) -> [f32; 4] {
    [
        u8_to_unit_f32(color.r()),
        u8_to_unit_f32(color.g()),
        u8_to_unit_f32(color.b()),
        u8_to_unit_f32(color.a()),
    ]
}
