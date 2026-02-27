use bevy::prelude::*;
use shared::conv::u32_to_usize;

use crate::map::{MapMode, MapResource, SelectedProvince};
use crate::net::{LatestGameState, Paused};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (update_hud, update_province_panel, button_interactions, update_button_styles));
    }
}

#[derive(Component)]
struct DateLabel;

#[derive(Component)]
struct ProvincePanel;

/// Marker for a map-mode button; stores which mode it activates.
#[derive(Component)]
struct MapModeButton(MapMode);

/// Marker for the pause/unpause button.
#[derive(Component)]
struct PauseButton;

const BTN_NORMAL: Color = Color::srgb(0.18, 0.20, 0.22);
const BTN_HOVER: Color = Color::srgb(0.28, 0.31, 0.34);
const BTN_ACTIVE: Color = Color::srgb(0.22, 0.50, 0.80);
const TEXT_COLOR: Color = Color::srgb(0.92, 0.92, 0.92);

fn setup(mut commands: Commands) {
    // Camera centered on East Asia (Equal Earth ≈ 105°E, 35°N → x≈105, y≈38).
    commands.spawn((
        Camera2d,
        OrthographicProjection {
            scale: 0.1,
            ..OrthographicProjection::default_2d()
        },
        Transform::from_xyz(105.0, 38.0, 999.9),
    ));

    // HUD: date + tick + map mode in the top-left corner.
    commands.spawn((
        Text::new("Connecting..."),
        TextFont {
            font_size: 18.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        DateLabel,
    ));

    // Province info panel on the right side.
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            right: Val::Px(10.0),
            max_width: Val::Px(300.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ProvincePanel,
    ));

    // Bottom toolbar: map mode buttons + pause button.
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexEnd,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(12.0)),
            ..default()
        })
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                ..default()
            })
            .with_children(|row| {
                let modes: &[(MapMode, &str)] = &[
                    (MapMode::Political, "Political [1]"),
                    (MapMode::Population, "Population [2]"),
                    (MapMode::Production, "Production [3]"),
                    (MapMode::Terrain, "Terrain [4]"),
                    (MapMode::Owner, "Owner [5]"),
                ];
                for &(mode, label) in modes {
                    row.spawn((
                        Button,
                        MapModeButton(mode),
                        Node {
                            padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                            ..default()
                        },
                        BackgroundColor(BTN_NORMAL),
                        BorderRadius::all(Val::Px(4.0)),
                    ))
                    .with_children(|btn| {
                        btn.spawn((
                            Text::new(label),
                            TextFont { font_size: 13.0, ..default() },
                            TextColor(TEXT_COLOR),
                        ));
                    });
                }

                // Spacer.
                row.spawn(Node { width: Val::Px(20.0), ..default() });

                // Pause button.
                row.spawn((
                    Button,
                    PauseButton,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.60, 0.28)),
                    BorderRadius::all(Val::Px(4.0)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Unpause [Space]"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(TEXT_COLOR),
                    ));
                });
            });
        });
}

fn update_hud(
    state: Res<LatestGameState>,
    mode: Res<MapMode>,
    paused: Res<Paused>,
    mut query: Query<&mut Text, With<DateLabel>>,
) {
    if let Some(gs) = &state.0 {
        let pause_str = if paused.0 { "PAUSED" } else { "▶" };
        for mut text in query.iter_mut() {
            *text = Text::new(format!(
                "Date: {}-{:02}-{:02}   Tick: {}   [{}]  {}  (Space=pause, 1/2/3=map)",
                gs.date.year, gs.date.month, gs.date.day, gs.tick, *mode, pause_str
            ));
        }
    }
}

fn update_province_panel(
    state: Res<LatestGameState>,
    selected: Res<SelectedProvince>,
    map: Option<Res<MapResource>>,
    mut query: Query<&mut Text, With<ProvincePanel>>,
) {
    for mut text in query.iter_mut() {
        let Some(pid) = selected.0 else {
            *text = Text::new("Click a province");
            return;
        };

        let Some(gs) = &state.0 else {
            *text = Text::new("");
            return;
        };

        let Some(province) = gs.provinces.get(u32_to_usize(pid)) else {
            *text = Text::new(format!("Province #{pid} (no data)"));
            return;
        };

        let mut info = String::new();

        // Province name (prefer map name if available)
        let name = map
            .as_ref()
            .and_then(|m| m.0.provinces.get(u32_to_usize(pid)))
            .map(|mp| mp.name.as_str())
            .unwrap_or(&province.name);
        info.push_str(&format!("=== {} ===\n", name));
        info.push_str(&format!(
            "Owner: {}\n",
            province.owner.as_deref().unwrap_or("None")
        ));

        // Terrain / geography info from map data
        if let Some(mp) = map
            .as_ref()
            .and_then(|m| m.0.provinces.get(u32_to_usize(pid)))
        {
            info.push_str(&format!("Topography: {}\n", mp.topography));
            info.push_str(&format!("Vegetation: {}\n", mp.vegetation));
            info.push_str(&format!("Climate: {}\n", mp.climate));
            if !mp.raw_material.is_empty() {
                info.push_str(&format!("Resource: {}\n", mp.raw_material));
            }
            if mp.harbor_suitability > 0.0 {
                info.push_str(&format!("Harbor: {:.0}%\n", mp.harbor_suitability * 100.0));
            }
            if let Some(sz) = &mp.port_sea_zone {
                info.push_str(&format!("Sea Zone: {}\n", sz));
            }
        }

        // Population
        let total_pop: u32 = province.pops.iter().map(|p| p.size).sum();
        info.push_str(&format!("\nPopulation: {}\n", total_pop));
        for pop in &province.pops {
            if pop.size > 0 {
                info.push_str(&format!(
                    "  {}: {} ({:.0}%)\n",
                    pop.class,
                    pop.size,
                    pop.needs_satisfaction * 100.0
                ));
            }
        }

        // Buildings
        if !province.buildings.is_empty() {
            info.push_str("\nBuildings:\n");
            for b in &province.buildings {
                let name = gs
                    .building_types
                    .iter()
                    .find(|bt| bt.id == b.type_id)
                    .map(|bt| bt.name.as_str())
                    .unwrap_or(&b.type_id);
                info.push_str(&format!("  {} Lv.{}\n", name, b.level));
            }
        }

        // Stockpile
        let non_empty: Vec<_> = province
            .stockpile
            .iter()
            .filter(|(_, v)| **v > 0.01)
            .collect();
        if !non_empty.is_empty() {
            info.push_str("\nStockpile:\n");
            for (good, amount) in non_empty {
                info.push_str(&format!("  {}: {:.1}\n", good, amount));
            }
        }

        *text = Text::new(info);
    }
}

/// Handle button clicks → update MapMode or toggle Paused.
fn button_interactions(
    interaction_q: Query<
        (&Interaction, Option<&MapModeButton>, Option<&PauseButton>),
        (Changed<Interaction>, With<Button>),
    >,
    mut mode: ResMut<MapMode>,
    mut paused: ResMut<Paused>,
) {
    for (interaction, map_btn, pause_btn) in &interaction_q {
        if *interaction != Interaction::Pressed {
            continue;
        }
        if let Some(MapModeButton(m)) = map_btn {
            *mode = *m;
        }
        if pause_btn.is_some() {
            paused.0 = !paused.0;
        }
    }
}

/// Keep button backgrounds and pause button label in sync with current state.
fn update_button_styles(
    mode: Res<MapMode>,
    paused: Res<Paused>,
    mut mode_btns: Query<(&MapModeButton, &Interaction, &mut BackgroundColor)>,
    mut pause_btns: Query<
        (&Interaction, &mut BackgroundColor, &Children),
        (With<PauseButton>, Without<MapModeButton>),
    >,
    mut texts: Query<&mut Text>,
) {
    for (MapModeButton(btn_mode), interaction, mut bg) in &mut mode_btns {
        *bg = if *btn_mode == *mode {
            BackgroundColor(BTN_ACTIVE)
        } else if *interaction == Interaction::Hovered {
            BackgroundColor(BTN_HOVER)
        } else {
            BackgroundColor(BTN_NORMAL)
        };
    }

    // Pause button: green when paused (game stopped), red when running.
    let (pause_base, pause_label) = if paused.0 {
        (Color::srgb(0.22, 0.60, 0.28), "Unpause [Space]")
    } else {
        (Color::srgb(0.60, 0.22, 0.18), "Pause [Space]")
    };
    for (interaction, mut bg, children) in &mut pause_btns {
        let lighter = *interaction == Interaction::Hovered;
        let c = pause_base.to_srgba();
        *bg = BackgroundColor(if lighter {
            Color::srgb(
                (c.red + 0.1).min(1.0),
                (c.green + 0.1).min(1.0),
                (c.blue + 0.1).min(1.0),
            )
        } else {
            pause_base
        });
        for &child in children {
            if let Ok(mut t) = texts.get_mut(child) {
                *t = Text::new(pause_label);
            }
        }
    }
}
