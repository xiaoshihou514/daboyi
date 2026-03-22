use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::editor::{
    self, ActiveArea, ActiveCountry, AdminAreas, AdminAssignments, EditorCountries, MapColoring,
    NextAreaId,
};
use crate::map::{ColoringVersion, MapMode, MapResource, ProvinceNames, SelectedProvince};
use crate::state::AppState;
use shared::{AdminArea, EditorCountry};

pub struct UiPlugin;

const FONT_PATH: &str = "fonts/NotoSansCJKsc-Regular.otf";

/// Holds the loaded CJK font handle (Simplified Chinese).
#[derive(Resource)]
pub struct CjkFont(pub Handle<Font>);

// ── UI component markers ───────────────────────────────────────────────────────

#[derive(Component)]
struct CountryListItem(String); // stores country tag

#[derive(Component)]
struct AdminAreaListItem(u32); // stores area id

#[derive(Component)]
struct CountryListPanel;

#[derive(Component)]
struct ProvinceInfoPanel;

#[derive(Component)]
struct MapModeButton(MapMode);

#[derive(Component)]
struct SaveButton;

#[derive(Component)]
struct LoadButton;

#[derive(Component)]
struct AddCountryButton;

#[derive(Component)]
struct DeleteCountryButton(String);

#[derive(Component)]
struct AddAreaButton(String); // country_tag for top-level areas

#[derive(Component)]
struct AddSubAreaButton(u32); // parent_area_id

#[derive(Component)]
struct DeleteAreaButton(u32);

#[derive(Component)]
struct RenameCountryButton(String);

#[derive(Component)]
struct RenameAreaButton(u32);

// ── Rename input ───────────────────────────────────────────────────────────────

#[derive(Default, Clone, PartialEq)]
pub enum RenameTarget {
    #[default]
    None,
    Country(String),
    Area(u32),
}

#[derive(Resource, Default)]
pub struct RenameInput {
    pub target: RenameTarget,
    pub buffer: String,
}

// ── Colors ─────────────────────────────────────────────────────────────────────

const BTN_NORMAL: Color = Color::srgb(0.18, 0.20, 0.22);
const BTN_HOVER: Color = Color::srgb(0.28, 0.31, 0.34);
const BTN_ACTIVE: Color = Color::srgb(0.22, 0.50, 0.80);
const BTN_SELECTED: Color = Color::srgb(0.15, 0.45, 0.15);
const AREA_SELECTED: Color = Color::srgb(0.15, 0.30, 0.50);
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.92);
const PANEL_BG: Color = Color::srgba(0.08, 0.08, 0.10, 0.92);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenameInput>()
            .add_systems(Startup, (load_font, setup_camera))
            .add_systems(OnEnter(AppState::Editing), setup_editor_ui)
            .add_systems(
                Update,
                (
                    button_interactions,
                    handle_rename_input,
                    update_province_panel,
                    update_country_list,
                    paint_province,
                )
                    .run_if(in_state(AppState::Editing)),
            );
    }
}

fn load_font(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load(FONT_PATH);
    commands.insert_resource(CjkFont(font));
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn setup_editor_ui(mut commands: Commands, cjk: Option<Res<CjkFont>>) {
    let font = cjk.map(|r| r.0.clone()).unwrap_or_default();

    // Root full-screen container
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        })
        .with_children(|root| {
            // Top toolbar
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(44.0),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(8.0)),
                    column_gap: Val::Px(6.0),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                GlobalZIndex(10),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new("大博弈 地图编辑器  "),
                    TextFont { font: font.clone(), font_size: 16.0, ..default() },
                    TextColor(Color::srgb(0.8, 0.8, 0.6)),
                ));

                // Map mode buttons
                for (label, mode) in [("省份", MapMode::Province), ("地形", MapMode::Terrain), ("政治", MapMode::Political)] {
                    bar.spawn((
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                        MapModeButton(mode),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(label),
                            TextFont { font: font.clone(), font_size: 13.0, ..default() },
                            TextColor(TEXT_COLOR),
                        ));
                    });
                }

                // Spacer
                bar.spawn(Node { flex_grow: 1.0, ..default() });

                // Save / Load buttons
                bar.spawn((
                    Button,
                    Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..default() },
                    BackgroundColor(BTN_NORMAL),
                    SaveButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("保存"),
                        TextFont { font: font.clone(), font_size: 13.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
                bar.spawn((
                    Button,
                    Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)), ..default() },
                    BackgroundColor(BTN_NORMAL),
                    LoadButton,
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("加载"),
                        TextFont { font: font.clone(), font_size: 13.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
            });

            // Main row: left panel + center (map) + right panel
            root.spawn(Node {
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                ..default()
            })
            .with_children(|row| {
                // Left panel: country/area tree
                row.spawn((
                    Node {
                        width: Val::Px(230.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(6.0)),
                        row_gap: Val::Px(3.0),
                        overflow: Overflow::clip_y(),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    CountryListPanel,
                    GlobalZIndex(9),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("国家 / 行政区"),
                        TextFont { font: font.clone(), font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.7, 0.7, 0.9)),
                    ));

                    panel.spawn((
                        Button,
                        Node {
                            width: Val::Percent(100.0),
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.15, 0.35, 0.15)),
                        AddCountryButton,
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new("＋ 添加国家"),
                            TextFont { font: font.clone(), font_size: 12.0, ..default() },
                            TextColor(TEXT_COLOR),
                        ));
                    });
                });

                // Center spacer (map renders behind)
                row.spawn(Node { flex_grow: 1.0, ..default() });

                // Right panel: province info
                row.spawn((
                    Node {
                        width: Val::Px(220.0),
                        height: Val::Percent(100.0),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(Val::Px(8.0)),
                        row_gap: Val::Px(6.0),
                        ..default()
                    },
                    BackgroundColor(PANEL_BG),
                    ProvinceInfoPanel,
                    GlobalZIndex(9),
                ))
                .with_children(|panel| {
                    panel.spawn((
                        Text::new("省份信息"),
                        TextFont { font: font.clone(), font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.7, 0.7, 0.9)),
                    ));

                    panel.spawn((
                        Text::new("点击一个省份"),
                        TextFont { font: font.clone(), font_size: 12.0, ..default() },
                        TextColor(Color::srgb(0.6, 0.6, 0.6)),
                        ProvinceInfoText,
                    ));
                });
            });
        });
}

/// Marker for the province info text entity.
#[derive(Component)]
struct ProvinceInfoText;

/// Update the province info panel when the selection changes.
fn update_province_panel(
    selected: Res<SelectedProvince>,
    map: Option<Res<MapResource>>,
    coloring: Res<MapColoring>,
    admin_assignments: Res<AdminAssignments>,
    admin_areas: Res<AdminAreas>,
    countries: Res<EditorCountries>,
    province_names: Option<Res<ProvinceNames>>,
    mut info_q: Query<&mut Text, With<ProvinceInfoText>>,
) {
    let Ok(mut text) = info_q.get_single_mut() else { return };

    let Some(pid) = selected.0 else {
        *text = Text::new("点击一个省份");
        return;
    };
    let Some(map) = map else { return };

    let pidx = pid as usize;
    if pidx >= map.0.provinces.len() {
        return;
    }
    let mp = &map.0.provinces[pidx];

    // Chinese name from province_names.tsv.
    let zh_name = province_names
        .and_then(|pn| pn.0.get(&mp.name.to_lowercase()).cloned())
        .unwrap_or_else(|| mp.name.clone());

    // Owner display: admin area > country > unassigned.
    let owner = if let Some(&area_id) = admin_assignments.0.get(&pid) {
        if let Some(area) = admin_areas.0.iter().find(|a| a.id == area_id) {
            format!("{}（行政区）", area.name)
        } else {
            "(未知行政区)".to_string()
        }
    } else if let Some(tag) = coloring.assignments.get(&pid) {
        countries.0.iter().find(|c| &c.tag == tag)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| tag.clone())
    } else {
        "(未分配)".to_string()
    };

    let info = format!(
        "{}\n\nID: {}\n地形: {}\n植被: {}\n气候: {}\n\n归属: {}",
        zh_name, pid, mp.topography, mp.vegetation, mp.climate, owner
    );
    *text = Text::new(info);
}

// ── Country/area list ──────────────────────────────────────────────────────────

/// Rebuild the country/area tree whenever relevant state changes.
fn update_country_list(
    mut commands: Commands,
    countries: Res<EditorCountries>,
    admin_areas: Res<AdminAreas>,
    active_country: Res<ActiveCountry>,
    active_area: Res<ActiveArea>,
    rename_input: Res<RenameInput>,
    cjk: Option<Res<CjkFont>>,
    panel_q: Query<Entity, With<CountryListPanel>>,
    items_q: Query<Entity, Or<(With<CountryListItem>, With<AdminAreaListItem>)>>,
) {
    if !countries.is_changed()
        && !admin_areas.is_changed()
        && !active_country.is_changed()
        && !active_area.is_changed()
        && !rename_input.is_changed()
    {
        return;
    }
    let Ok(panel) = panel_q.get_single() else { return };
    let font = cjk.map(|r| r.0.clone()).unwrap_or_default();

    // Despawn old items.
    for e in items_q.iter() {
        commands.entity(e).despawn_recursive();
    }

    for country in &countries.0 {
        let is_active_country = active_country.0.as_deref() == Some(&country.tag)
            && active_area.0.is_none();
        let row_bg = if is_active_country { BTN_SELECTED } else { BTN_NORMAL };
        let swatch = Color::srgba(
            country.color[0], country.color[1], country.color[2], country.color[3],
        );
        let tag = country.tag.clone();

        // Is this country being renamed?
        let renaming = matches!(&rename_input.target, RenameTarget::Country(t) if t == &tag);
        let display_name = if renaming {
            format!("✏ {}_", rename_input.buffer)
        } else {
            format!("{} [{}]", country.name, tag)
        };

        let row = commands.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                column_gap: Val::Px(4.0),
                ..default()
            },
            BackgroundColor(row_bg),
            CountryListItem(tag.clone()),
        ))
        .with_children(|row| {
            // Color swatch
            row.spawn((
                Node { width: Val::Px(12.0), height: Val::Px(12.0), ..default() },
                BackgroundColor(swatch),
            ));
            // Name
            row.spawn((
                Text::new(display_name),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(TEXT_COLOR),
                Node { flex_grow: 1.0, ..default() },
            ));
            // Rename button
            if !renaming {
                row.spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(3.0), Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.25, 0.4)),
                    RenameCountryButton(tag.clone()),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("✏"),
                        TextFont { font: font.clone(), font_size: 10.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
            }
            // Delete button
            row.spawn((
                Button,
                Node {
                    width: Val::Px(16.0),
                    height: Val::Px(16.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.1, 0.1)),
                DeleteCountryButton(tag.clone()),
            ))
            .with_children(|d| {
                d.spawn((
                    Text::new("×"),
                    TextFont { font: font.clone(), font_size: 12.0, ..default() },
                    TextColor(TEXT_COLOR),
                ));
            });
        })
        .id();
        commands.entity(panel).add_child(row);

        // "Add area" button under this country
        let add_row = commands.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                padding: UiRect::new(Val::Px(20.0), Val::Px(4.0), Val::Px(2.0), Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.25, 0.12)),
            AddAreaButton(tag.clone()),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("＋ 添加行政区"),
                TextFont { font: font.clone(), font_size: 10.0, ..default() },
                TextColor(Color::srgb(0.6, 0.9, 0.6)),
            ));
        })
        .id();
        commands.entity(panel).add_child(add_row);

        // Spawn area tree for this country (top-level areas first, then recursively).
        spawn_area_subtree(
            &mut commands,
            panel,
            &admin_areas.0,
            &tag,
            None,
            1,
            &active_area,
            &rename_input,
            &font,
        );
    }
}

/// Recursively spawn area rows for children of `parent_id` (None = top-level).
#[allow(clippy::too_many_arguments)]
fn spawn_area_subtree(
    commands: &mut Commands,
    panel: Entity,
    areas: &[AdminArea],
    country_tag: &str,
    parent_id: Option<u32>,
    depth: u32,
    active_area: &ActiveArea,
    rename_input: &RenameInput,
    font: &Handle<Font>,
) {
    let children: Vec<&AdminArea> = areas
        .iter()
        .filter(|a| a.country_tag == country_tag && a.parent_id == parent_id)
        .collect();

    for area in children {
        let indent = Val::Px(6.0 + depth as f32 * 14.0);
        let is_active = active_area.0 == Some(area.id);
        let row_bg = if is_active { AREA_SELECTED } else { BTN_NORMAL };

        let renaming = matches!(&rename_input.target, RenameTarget::Area(id) if *id == area.id);
        let display_name = if renaming {
            format!("✏ {}_", rename_input.buffer)
        } else {
            area.name.clone()
        };

        let row = commands.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::new(indent, Val::Px(2.0), Val::Px(2.0), Val::Px(2.0)),
                column_gap: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(row_bg),
            AdminAreaListItem(area.id),
        ))
        .with_children(|row| {
            if let Some(col) = area.color {
                row.spawn((
                    Node { width: Val::Px(10.0), height: Val::Px(10.0), ..default() },
                    BackgroundColor(Color::srgba(col[0], col[1], col[2], col[3])),
                ));
            }
            row.spawn((
                Text::new(display_name),
                TextFont { font: font.clone(), font_size: 10.0, ..default() },
                TextColor(Color::srgb(0.75, 0.85, 0.95)),
                Node { flex_grow: 1.0, ..default() },
            ));
            if !renaming {
                row.spawn((
                    Button,
                    Node { padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)), ..default() },
                    BackgroundColor(Color::srgb(0.25, 0.25, 0.4)),
                    RenameAreaButton(area.id),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("✏"),
                        TextFont { font: font.clone(), font_size: 9.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
            }
            row.spawn((
                Button,
                Node { padding: UiRect::axes(Val::Px(2.0), Val::Px(1.0)), ..default() },
                BackgroundColor(Color::srgb(0.12, 0.20, 0.12)),
                AddSubAreaButton(area.id),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("＋"),
                    TextFont { font: font.clone(), font_size: 9.0, ..default() },
                    TextColor(Color::srgb(0.6, 0.9, 0.6)),
                ));
            });
            row.spawn((
                Button,
                Node {
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.5, 0.1, 0.1)),
                DeleteAreaButton(area.id),
            ))
            .with_children(|d| {
                d.spawn((
                    Text::new("×"),
                    TextFont { font: font.clone(), font_size: 10.0, ..default() },
                    TextColor(TEXT_COLOR),
                ));
            });
        })
        .id();
        commands.entity(panel).add_child(row);

        // Recurse into children.
        spawn_area_subtree(
            commands, panel, areas, country_tag, Some(area.id), depth + 1,
            active_area, rename_input, font,
        );
    }
}

// ── Button interactions ────────────────────────────────────────────────────────

fn button_interactions(
    interaction_q: Query<
        (
            &Interaction,
            Option<&MapModeButton>,
            Option<&SaveButton>,
            Option<&LoadButton>,
            Option<&AddCountryButton>,
            Option<&CountryListItem>,
            Option<&DeleteCountryButton>,
            Option<&AddAreaButton>,
            Option<&AddSubAreaButton>,
            Option<&DeleteAreaButton>,
            Option<&AdminAreaListItem>,
            Option<&RenameCountryButton>,
            Option<&RenameAreaButton>,
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut mode: ResMut<MapMode>,
    mut coloring: ResMut<MapColoring>,
    mut countries: ResMut<EditorCountries>,
    mut admin_areas: ResMut<AdminAreas>,
    mut admin_assignments: ResMut<AdminAssignments>,
    mut active_country: ResMut<ActiveCountry>,
    mut active_area: ResMut<ActiveArea>,
    mut next_id: ResMut<NextAreaId>,
    mut coloring_version: ResMut<ColoringVersion>,
    mut rename_input: ResMut<RenameInput>,
) {
    for (
        interaction, mode_btn, save_btn, load_btn, add_btn, list_item, delete_btn,
        add_area_btn, add_sub_btn, delete_area_btn, area_item, rename_country_btn, rename_area_btn,
    ) in interaction_q.iter()
    {
        if *interaction != Interaction::Pressed {
            continue;
        }

        if let Some(MapModeButton(m)) = mode_btn {
            *mode = *m;
        } else if save_btn.is_some() {
            editor::save_coloring(&coloring, &countries, &admin_areas, &admin_assignments);
        } else if load_btn.is_some() {
            let mut next = NextAreaId::default();
            editor::load_coloring(
                &mut coloring, &mut countries, &mut admin_areas, &mut admin_assignments, &mut next,
            );
            next_id.0 = next.0;
            coloring_version.0 += 1;
        } else if add_btn.is_some() {
            let idx = countries.0.len();
            let tag = format!("C{idx:03}");
            let hue = (idx as f32 * 137.5) % 360.0;
            let color = hsl_to_rgba(hue, 0.65, 0.50);
            countries.0.push(EditorCountry {
                tag: tag.clone(),
                name: format!("国家{}", idx + 1),
                color,
                capital_province: None,
            });
            active_country.0 = Some(tag);
            active_area.0 = None;
        } else if let Some(CountryListItem(tag)) = list_item {
            active_country.0 = Some(tag.clone());
            active_area.0 = None;
            rename_input.target = RenameTarget::None;
        } else if let Some(DeleteCountryButton(tag)) = delete_btn {
            let tag_clone = tag.clone();
            countries.0.retain(|c| &c.tag != &tag_clone);
            coloring.assignments.retain(|_, v| v != &tag_clone);
            // Remove areas belonging to this country.
            let removed_ids: Vec<u32> = admin_areas.0.iter()
                .filter(|a| &a.country_tag == &tag_clone)
                .map(|a| a.id)
                .collect();
            admin_areas.0.retain(|a| &a.country_tag != &tag_clone);
            admin_assignments.0.retain(|_, aid| !removed_ids.contains(aid));
            coloring_version.0 += 1;
            if active_country.0.as_deref() == Some(&tag_clone) {
                active_country.0 = None;
                active_area.0 = None;
            }
        } else if let Some(AddAreaButton(ctag)) = add_area_btn {
            let id = next_id.0;
            next_id.0 += 1;
            let hue = (id as f32 * 73.5) % 360.0;
            let col = hsl_to_rgba(hue, 0.55, 0.45);
            admin_areas.0.push(AdminArea {
                id,
                name: format!("行政区{}", id + 1),
                country_tag: ctag.clone(),
                parent_id: None,
                color: Some(col),
            });
            active_area.0 = Some(id);
            active_country.0 = Some(ctag.clone());
        } else if let Some(AddSubAreaButton(parent_id)) = add_sub_btn {
            // Inherit country_tag from parent.
            if let Some(parent) = admin_areas.0.iter().find(|a| a.id == *parent_id) {
                let ctag = parent.country_tag.clone();
                let id = next_id.0;
                next_id.0 += 1;
                let hue = (id as f32 * 73.5) % 360.0;
                let col = hsl_to_rgba(hue, 0.55, 0.40);
                admin_areas.0.push(AdminArea {
                    id,
                    name: format!("行政区{}", id + 1),
                    country_tag: ctag,
                    parent_id: Some(*parent_id),
                    color: Some(col),
                });
                active_area.0 = Some(id);
            }
        } else if let Some(DeleteAreaButton(area_id)) = delete_area_btn {
            // Recursively collect all descendant IDs.
            let to_remove = collect_area_subtree(&admin_areas.0, *area_id);
            admin_areas.0.retain(|a| !to_remove.contains(&a.id));
            admin_assignments.0.retain(|_, aid| !to_remove.contains(aid));
            coloring_version.0 += 1;
            if active_area.0.map(|id| to_remove.contains(&id)).unwrap_or(false) {
                active_area.0 = None;
            }
        } else if let Some(AdminAreaListItem(area_id)) = area_item {
            active_area.0 = Some(*area_id);
            if let Some(area) = admin_areas.0.iter().find(|a| a.id == *area_id) {
                active_country.0 = Some(area.country_tag.clone());
            }
            rename_input.target = RenameTarget::None;
        } else if let Some(RenameCountryButton(tag)) = rename_country_btn {
            let current_name = countries.0.iter()
                .find(|c| &c.tag == tag)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            rename_input.target = RenameTarget::Country(tag.clone());
            rename_input.buffer = current_name;
        } else if let Some(RenameAreaButton(area_id)) = rename_area_btn {
            let current_name = admin_areas.0.iter()
                .find(|a| a.id == *area_id)
                .map(|a| a.name.clone())
                .unwrap_or_default();
            rename_input.target = RenameTarget::Area(*area_id);
            rename_input.buffer = current_name;
        }
    }
}

/// Collect all area IDs in the subtree rooted at `root_id` (including root).
fn collect_area_subtree(areas: &[AdminArea], root_id: u32) -> Vec<u32> {
    let mut result = vec![root_id];
    let mut i = 0;
    while i < result.len() {
        let current = result[i];
        for a in areas.iter().filter(|a| a.parent_id == Some(current)) {
            result.push(a.id);
        }
        i += 1;
    }
    result
}

// ── Rename input ───────────────────────────────────────────────────────────────

fn handle_rename_input(
    mut key_events: EventReader<KeyboardInput>,
    mut rename_input: ResMut<RenameInput>,
    mut countries: ResMut<EditorCountries>,
    mut admin_areas: ResMut<AdminAreas>,
) {
    if matches!(rename_input.target, RenameTarget::None) {
        return;
    }

    for ev in key_events.read() {
        if !ev.state.is_pressed() {
            continue;
        }
        match &ev.logical_key {
            Key::Character(c) => {
                rename_input.buffer.push_str(c.as_str());
            }
            Key::Backspace => {
                rename_input.buffer.pop();
            }
            Key::Enter => {
                let new_name = rename_input.buffer.clone();
                match &rename_input.target {
                    RenameTarget::Country(tag) => {
                        let tag_clone = tag.clone();
                        if let Some(c) = countries.0.iter_mut().find(|c| c.tag == tag_clone) {
                            c.name = new_name;
                        }
                    }
                    RenameTarget::Area(id) => {
                        let id = *id;
                        if let Some(a) = admin_areas.0.iter_mut().find(|a| a.id == id) {
                            a.name = new_name;
                        }
                    }
                    RenameTarget::None => {}
                }
                rename_input.target = RenameTarget::None;
                rename_input.buffer.clear();
            }
            Key::Escape => {
                rename_input.target = RenameTarget::None;
                rename_input.buffer.clear();
            }
            _ => {}
        }
    }
}

// ── Province painting ──────────────────────────────────────────────────────────

/// Assign the selected province to the active country or area.
/// Supports single click and drag-brush (runs every frame LMB is held + province changed).
fn paint_province(
    mouse: Res<ButtonInput<MouseButton>>,
    selected: Res<SelectedProvince>,
    mode: Res<MapMode>,
    active_country: Res<ActiveCountry>,
    active_area: Res<ActiveArea>,
    mut coloring: ResMut<MapColoring>,
    mut admin_assignments: ResMut<AdminAssignments>,
    mut coloring_version: ResMut<ColoringVersion>,
) {
    if *mode != MapMode::Political {
        return;
    }
    let Some(pid) = selected.0 else { return };

    // Paint on initial click OR while dragging and the hovered province changed.
    let should_paint = mouse.just_pressed(MouseButton::Left)
        || (mouse.pressed(MouseButton::Left) && selected.is_changed());

    if should_paint {
        if let Some(area_id) = active_area.0 {
            admin_assignments.0.insert(pid, area_id);
            coloring_version.0 += 1;
        } else if let Some(tag) = &active_country.0 {
            coloring.assignments.insert(pid, tag.clone());
            coloring_version.0 += 1;
        }
    }

    // Right-click: deassign from both country and admin area.
    if mouse.just_pressed(MouseButton::Right) {
        let c = coloring.assignments.remove(&pid).is_some();
        let a = admin_assignments.0.remove(&pid).is_some();
        if c || a {
            coloring_version.0 += 1;
        }
    }
}

// ── Color helpers ──────────────────────────────────────────────────────────────

/// Convert HSL color to RGBA [0,1].
fn hsl_to_rgba(h: f32, s: f32, l: f32) -> [f32; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
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
