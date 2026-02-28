use bevy::prelude::*;
use rust_i18n::t;
use shared::conv::u32_to_usize;
use shared::{Good, PopClass};

use crate::map::{MapMode, MapResource, SelectedProvince};
use crate::net::{LatestGameState, Paused};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (update_hud, update_province_panel, button_interactions, update_button_styles));
    }
}

/// Holds the loaded CJK font handle (Simplified Chinese).
#[derive(Resource)]
#[allow(dead_code)]
pub struct CjkFont(pub Handle<Font>);

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

fn good_name(good: Good) -> std::borrow::Cow<'static, str> {
    let key = match good {
        Good::Grain => "good_grain",
        Good::Clothing => "good_clothing",
        Good::Fuel => "good_fuel",
        Good::Tools => "good_tools",
        Good::Luxuries => "good_luxuries",
        Good::Metal => "good_metal",
        Good::BuildingMaterials => "good_building_materials",
    };
    t!(key)
}

fn pop_class_name(class: PopClass) -> std::borrow::Cow<'static, str> {
    let key = match class {
        PopClass::TenantFarmer => "pop_tenant_farmer",
        PopClass::Yeoman => "pop_yeoman",
        PopClass::Landlord => "pop_landlord",
        PopClass::Capitalist => "pop_capitalist",
        PopClass::PetitBourgeois => "pop_petit_bourgeois",
        PopClass::Clergy => "pop_clergy",
        PopClass::Bureaucrat => "pop_bureaucrat",
        PopClass::Nobility => "pop_nobility",
        PopClass::Soldier => "pop_soldier",
        PopClass::Intelligentsia => "pop_intelligentsia",
    };
    t!(key)
}

fn building_name(id: &str) -> std::borrow::Cow<'static, str> {
    let key = match id {
        "farm" => "building_farm",
        "yeoman_farm" => "building_yeoman_farm",
        "textile_workshop" => "building_textile_workshop",
        "mine" => "building_mine",
        "charcoal_kiln" => "building_charcoal_kiln",
        "smithy" => "building_smithy",
        "luxury_workshop" => "building_luxury_workshop",
        "sawmill" => "building_sawmill",
        _ => return std::borrow::Cow::Owned(id.to_string()),
    };
    t!(key)
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    // Load CJK font (Simplified Chinese) and store as a global resource.
    let cjk: Handle<Font> = asset_server.load("fonts/NotoSansCJKsc-Regular.otf");
    commands.insert_resource(CjkFont(cjk.clone()));

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
        Text::new(t!("connecting").to_string()),
        TextFont {
            font: cjk.clone(),
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
            font: cjk.clone(),
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
                let modes: &[(MapMode, u8)] = &[
                    (MapMode::Province, 1),
                    (MapMode::Population, 2),
                    (MapMode::Production, 3),
                    (MapMode::Terrain, 4),
                    (MapMode::Political, 5),
                ];
                for &(mode, key_num) in modes {
                    let label = format!("{} [{}]", mode, key_num);
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
                            TextFont { font: cjk.clone(), font_size: 13.0, ..default() },
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
                        Text::new(t!("unpause_btn").to_string()),
                        TextFont { font: cjk.clone(), font_size: 13.0, ..default() },
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
        let pause_str = if paused.0 { t!("paused") } else { t!("playing") };
        for mut text in query.iter_mut() {
            *text = Text::new(format!(
                "{}: {}-{:02}-{:02}   {}: {}   [{}]  {}  {}",
                t!("hud_date"), gs.date.year, gs.date.month, gs.date.day,
                t!("hud_tick"), gs.tick, *mode, pause_str,
                t!("hud_hint")
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
            *text = Text::new(t!("click_province").to_string());
            return;
        };

        let Some(gs) = &state.0 else {
            *text = Text::new("");
            return;
        };

        let Some(province) = gs.provinces.get(u32_to_usize(pid)) else {
            *text = Text::new(format!("#{pid} ({})", t!("no_data")));
            return;
        };

        let mut info = String::new();

        // Province name: prefer Chinese name from GameState (loaded from province_names.tsv)
        // Fall back to map tag if GameState name is just the raw tag
        let name = &province.name;
        info.push_str(&format!("=== {} ===\n", name));
        // Show Chinese country name if available, else tag
        let owner_display = province.owner.as_deref().map(|tag| {
            gs.countries
                .iter()
                .find(|c| c.tag == tag)
                .map(|c| c.name.as_str())
                .unwrap_or(tag)
                .to_string()
        });
        info.push_str(&format!(
            "{}: {}\n",
            t!("owner"),
            owner_display.as_deref().unwrap_or(&t!("none_owner"))
        ));

        // Terrain / geography info from map data
        if let Some(mp) = map
            .as_ref()
            .and_then(|m| m.0.provinces.get(u32_to_usize(pid)))
        {
            info.push_str(&format!("{}: {}\n", t!("topography"), mp.topography));
            info.push_str(&format!("{}: {}\n", t!("vegetation"), mp.vegetation));
            info.push_str(&format!("{}: {}\n", t!("climate"), mp.climate));
            if !mp.raw_material.is_empty() {
                info.push_str(&format!("{}: {}\n", t!("resource"), mp.raw_material));
            }
            if mp.harbor_suitability > 0.0 {
                info.push_str(&format!("{}: {:.0}%\n", t!("harbor"), mp.harbor_suitability * 100.0));
            }
            if let Some(sz) = &mp.port_sea_zone {
                info.push_str(&format!("{}: {}\n", t!("sea_zone"), sz));
            }
        }

        // Population
        let total_pop: u32 = province.pops.iter().map(|p| p.size).sum();
        info.push_str(&format!("\n{}: {}\n", t!("population"), total_pop));
        for pop in &province.pops {
            if pop.size > 0 {
                info.push_str(&format!(
                    "  {}: {} ({:.0}%)\n",
                    pop_class_name(pop.class),
                    pop.size,
                    pop.needs_satisfaction * 100.0
                ));
            }
        }

        // Buildings
        if !province.buildings.is_empty() {
            info.push_str(&format!("\n{}:\n", t!("buildings")));
            for b in &province.buildings {
                let bname = gs
                    .building_types
                    .iter()
                    .find(|bt| bt.id == b.type_id)
                    .map(|bt| building_name(&bt.id))
                    .unwrap_or_else(|| building_name(&b.type_id));
                info.push_str(&format!("  {} Lv.{}\n", bname, b.level));
            }
        }

        // Stockpile
        let non_empty: Vec<_> = province
            .stockpile
            .iter()
            .filter(|(_, v)| **v > 0.01)
            .collect();
        if !non_empty.is_empty() {
            info.push_str(&format!("\n{}:\n", t!("stockpile")));
            for (good, amount) in non_empty {
                info.push_str(&format!("  {}: {:.1}\n", good_name(*good), amount));
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
        (Color::srgb(0.22, 0.60, 0.28), t!("unpause_btn").to_string())
    } else {
        (Color::srgb(0.60, 0.22, 0.18), t!("pause_btn").to_string())
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
                *t = Text::new(pause_label.clone());
            }
        }
    }
}
