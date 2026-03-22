use bevy::prelude::*;

use crate::editor::{self, ActiveCountry, EditorCountries, MapColoring};
use crate::map::{ColoringVersion, MapMode, MapResource, SelectedProvince};
use crate::state::AppState;
use shared::EditorCountry;

pub struct UiPlugin;

const FONT_PATH: &str = "fonts/NotoSansCJKsc-Regular.otf";

/// Holds the loaded CJK font handle (Simplified Chinese).
#[derive(Resource)]
pub struct CjkFont(pub Handle<Font>);

// ── UI component markers ───────────────────────────────────────────────────────

#[derive(Component)]
struct CountryListItem(String); // stores country tag

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

// ── Colors ─────────────────────────────────────────────────────────────────────

const BTN_NORMAL: Color = Color::srgb(0.18, 0.20, 0.22);
const BTN_HOVER: Color = Color::srgb(0.28, 0.31, 0.34);
const BTN_ACTIVE: Color = Color::srgb(0.22, 0.50, 0.80);
const BTN_SELECTED: Color = Color::srgb(0.15, 0.45, 0.15);
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.92);
const PANEL_BG: Color = Color::srgba(0.08, 0.08, 0.10, 0.92);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_font, setup_camera))
            .add_systems(OnEnter(AppState::Editing), setup_editor_ui)
            .add_systems(
                Update,
                (
                    button_interactions,
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
    commands.spawn(Node {
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
            // Title
            bar.spawn((
                Text::new("Daboyi Map Editor  "),
                TextFont { font: font.clone(), font_size: 16.0, ..default() },
                TextColor(Color::srgb(0.8, 0.8, 0.6)),
            ));

            // Map mode buttons
            for (label, mode) in [("Province (1)", MapMode::Province), ("Terrain (2)", MapMode::Terrain), ("Political (3)", MapMode::Political)] {
                bar.spawn((
                    Button,
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
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

            // Save / Load
            bar.spawn((
                Button,
                Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), ..default() },
                BackgroundColor(BTN_NORMAL),
                SaveButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Save"),
                    TextFont { font: font.clone(), font_size: 13.0, ..default() },
                    TextColor(TEXT_COLOR),
                ));
            });

            bar.spawn((
                Button,
                Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)), ..default() },
                BackgroundColor(BTN_NORMAL),
                LoadButton,
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("Load"),
                    TextFont { font: font.clone(), font_size: 13.0, ..default() },
                    TextColor(TEXT_COLOR),
                ));
            });
        });

        // Main area row
        root.spawn(Node {
            flex_grow: 1.0,
            flex_direction: FlexDirection::Row,
            ..default()
        })
        .with_children(|row| {
            // Left panel: country list
            row.spawn((
                Node {
                    width: Val::Px(200.0),
                    height: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    padding: UiRect::all(Val::Px(6.0)),
                    row_gap: Val::Px(4.0),
                    overflow: Overflow::clip_y(),
                    ..default()
                },
                BackgroundColor(PANEL_BG),
                CountryListPanel,
                GlobalZIndex(9),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new("Countries"),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
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
                        Text::new("+ Add Country"),
                        TextFont { font: font.clone(), font_size: 12.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
            });

            // Center: empty space (map renders behind UI)
            row.spawn(Node { flex_grow: 1.0, ..default() });

            // Right panel: province info
            row.spawn((
                Node {
                    width: Val::Px(210.0),
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
                    Text::new("Province Info"),
                    TextFont { font: font.clone(), font_size: 14.0, ..default() },
                    TextColor(Color::srgb(0.7, 0.7, 0.9)),
                ));

                panel.spawn((
                    Text::new("Click a province"),
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
    countries: Res<EditorCountries>,
    mut info_q: Query<&mut Text, With<ProvinceInfoText>>,
) {
    let Ok(mut text) = info_q.get_single_mut() else { return };

    let Some(pid) = selected.0 else {
        *text = Text::new("Click a province");
        return;
    };
    let Some(map) = map else { return };

    let pidx = pid as usize;
    if pidx >= map.0.provinces.len() {
        return;
    }
    let mp = &map.0.provinces[pidx];
    let owner = coloring.assignments.get(&pid).and_then(|tag| {
        countries.0.iter().find(|c| &c.tag == tag).map(|c| c.name.clone())
    });

    let info = format!(
        "ID: {}\nTag: {}\nTerrain: {}\nOwner: {}",
        pid,
        mp.tag,
        mp.topography,
        owner.as_deref().unwrap_or("(unassigned)")
    );
    *text = Text::new(info);
}

/// Rebuild the country list whenever EditorCountries changes.
fn update_country_list(
    mut commands: Commands,
    countries: Res<EditorCountries>,
    active: Res<ActiveCountry>,
    cjk: Option<Res<CjkFont>>,
    panel_q: Query<Entity, With<CountryListPanel>>,
    items_q: Query<Entity, With<CountryListItem>>,
) {
    if !countries.is_changed() && !active.is_changed() {
        return;
    }
    let Ok(panel) = panel_q.get_single() else { return };
    let font = cjk.map(|r| r.0.clone()).unwrap_or_default();

    // Despawn old country items.
    for e in items_q.iter() {
        commands.entity(e).despawn_recursive();
    }

    for country in &countries.0 {
        let is_active = active.0.as_deref() == Some(&country.tag);
        let swatch_color = Color::srgba(
            country.color[0],
            country.color[1],
            country.color[2],
            country.color[3],
        );
        let row_bg = if is_active { BTN_SELECTED } else { BTN_NORMAL };
        let tag = country.tag.clone();
        let name = country.name.clone();

        let row = commands.spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                padding: UiRect::axes(Val::Px(4.0), Val::Px(3.0)),
                column_gap: Val::Px(5.0),
                ..default()
            },
            BackgroundColor(row_bg),
            CountryListItem(tag.clone()),
        ))
        .with_children(|row| {
            // Color swatch
            row.spawn((
                Node {
                    width: Val::Px(14.0),
                    height: Val::Px(14.0),
                    ..default()
                },
                BackgroundColor(swatch_color),
            ));
            // Country name
            row.spawn((
                Text::new(format!("{name} [{tag}]")),
                TextFont { font: font.clone(), font_size: 11.0, ..default() },
                TextColor(TEXT_COLOR),
                Node { flex_grow: 1.0, ..default() },
            ));
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
                    TextFont { font: font.clone(), font_size: 11.0, ..default() },
                    TextColor(TEXT_COLOR),
                ));
            });
        })
        .id();

        commands.entity(panel).add_child(row);
    }
}

/// Handle all button clicks.
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
        ),
        (Changed<Interaction>, With<Button>),
    >,
    mut mode: ResMut<MapMode>,
    mut coloring: ResMut<MapColoring>,
    mut countries: ResMut<EditorCountries>,
    mut active: ResMut<ActiveCountry>,
    mut coloring_version: ResMut<ColoringVersion>,
) {
    for (interaction, mode_btn, save_btn, load_btn, add_btn, list_item, delete_btn) in
        interaction_q.iter()
    {
        if *interaction != Interaction::Pressed {
            continue;
        }

        if let Some(MapModeButton(m)) = mode_btn {
            *mode = *m;
        } else if save_btn.is_some() {
            editor::save_coloring(&coloring, &countries);
        } else if load_btn.is_some() {
            editor::load_coloring(&mut coloring, &mut countries);
            coloring_version.0 += 1;
        } else if add_btn.is_some() {
            let idx = countries.0.len();
            let tag = format!("C{idx:03}");
            // Cycle through distinct hues.
            let hue = (idx as f32 * 137.5) % 360.0;
            let color = hsl_to_rgba(hue, 0.65, 0.50);
            countries.0.push(EditorCountry {
                tag: tag.clone(),
                name: format!("Country {}", idx + 1),
                color,
                capital_province: None,
            });
            active.0 = Some(tag);
        } else if let Some(CountryListItem(tag)) = list_item {
            active.0 = Some(tag.clone());
        } else if let Some(DeleteCountryButton(tag)) = delete_btn {
            countries.0.retain(|c| &c.tag != tag);
            coloring.assignments.retain(|_, v| v != tag);
            coloring_version.0 += 1;
            if active.0.as_deref() == Some(tag) {
                active.0 = None;
            }
        }
    }
}

/// Left-click on a province (while in Political mode) assigns it to the active country.
/// Right-click deassigns.
fn paint_province(
    mouse: Res<ButtonInput<MouseButton>>,
    selected: Res<SelectedProvince>,
    mode: Res<MapMode>,
    active: Res<ActiveCountry>,
    mut coloring: ResMut<MapColoring>,
    mut coloring_version: ResMut<ColoringVersion>,
) {
    if *mode != MapMode::Political {
        return;
    }
    let Some(pid) = selected.0 else { return };

    // Left-click: assign to active country.
    if mouse.just_pressed(MouseButton::Left) {
        if let Some(tag) = &active.0 {
            coloring.assignments.insert(pid, tag.clone());
            coloring_version.0 += 1;
        }
    }

    // Right-click: deassign province.
    if mouse.just_pressed(MouseButton::Right) {
        if coloring.assignments.remove(&pid).is_some() {
            coloring_version.0 += 1;
        }
    }
}

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
