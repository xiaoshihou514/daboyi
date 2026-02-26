use bevy::prelude::*;

use crate::map::{MapMode, MapResource, SelectedProvince};
use crate::net::LatestGameState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (update_hud, update_province_panel));
    }
}

#[derive(Component)]
struct DateLabel;

#[derive(Component)]
struct ProvincePanel;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

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
}

fn update_hud(
    state: Res<LatestGameState>,
    mode: Res<MapMode>,
    mut query: Query<&mut Text, With<DateLabel>>,
) {
    if let Some(gs) = &state.0 {
        for mut text in query.iter_mut() {
            *text = Text::new(format!(
                "Date: {}-{:02}-{:02}   Tick: {}   [{}]  (1/2/3 switch)",
                gs.date.year, gs.date.month, gs.date.day, gs.tick, *mode
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

        let Some(province) = gs.provinces.get(pid as usize) else {
            *text = Text::new(format!("Province #{pid} (no data)"));
            return;
        };

        let mut info = String::new();

        // Province name (prefer map name if available)
        let name = map
            .as_ref()
            .and_then(|m| m.0.provinces.get(pid as usize))
            .map(|mp| mp.name.as_str())
            .unwrap_or(&province.name);
        info.push_str(&format!("=== {} ===\n", name));
        info.push_str(&format!(
            "Owner: {}\n",
            province.owner.as_deref().unwrap_or("None")
        ));

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
