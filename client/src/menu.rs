use bevy::prelude::*;
use rust_i18n::t;

use crate::map::{MapMode, SelectedProvince};
use crate::net::LatestGameState;
use crate::state::{AppState, PlayerCountry};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SelectedCountryForPlay>()
            // Start screen
            .add_systems(OnEnter(AppState::StartScreen), setup_start_screen)
            .add_systems(
                Update,
                start_screen_buttons.run_if(in_state(AppState::StartScreen)),
            )
            .add_systems(OnExit(AppState::StartScreen), despawn_tagged::<StartScreenRoot>)
            // Country selection
            .add_systems(
                OnEnter(AppState::CountrySelection),
                (setup_country_selection, set_political_mode),
            )
            .add_systems(
                Update,
                (update_country_selection_panel, country_selection_buttons)
                    .run_if(in_state(AppState::CountrySelection)),
            )
            .add_systems(
                OnExit(AppState::CountrySelection),
                (despawn_tagged::<CountrySelectionRoot>, clear_selected_province),
            );
    }
}

/// Tracks which country the player has highlighted in country selection.
#[derive(Resource, Default)]
pub struct SelectedCountryForPlay {
    pub tag: Option<String>,
    pub name: Option<String>,
}

// ─── marker components ────────────────────────────────────────────────────────

#[derive(Component)]
struct StartScreenRoot;

#[derive(Component)]
struct CountrySelectionRoot;

#[derive(Component)]
struct StartButton;

#[derive(Component)]
struct BackButton;

#[derive(Component)]
struct PlayAsButton;

#[derive(Component)]
struct CountryNameLabel;

// ─── helpers ──────────────────────────────────────────────────────────────────

fn despawn_tagged<T: Component>(mut commands: Commands, query: Query<Entity, With<T>>) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

fn clear_selected_province(mut selected: ResMut<SelectedProvince>) {
    selected.0 = None;
}

fn set_political_mode(mut mode: ResMut<MapMode>) {
    *mode = MapMode::Political;
}

// ─── start screen ─────────────────────────────────────────────────────────────

fn setup_start_screen(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font: Handle<Font> = asset_server.load("fonts/NotoSansCJKsc-Regular.otf");

    commands
        .spawn((
            StartScreenRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: Val::Px(0.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.02, 0.10, 0.93)),
            GlobalZIndex(10),
        ))
        .with_children(|parent| {
            // Title
            parent.spawn((
                Text::new(t!("start_title").to_string()),
                TextFont {
                    font: font.clone(),
                    font_size: 96.0,
                    ..default()
                },
                TextColor(Color::srgb(0.95, 0.88, 0.55)),
            ));
            // Subtitle
            parent.spawn((
                Text::new(t!("start_subtitle").to_string()),
                TextFont {
                    font: font.clone(),
                    font_size: 22.0,
                    ..default()
                },
                TextColor(Color::srgb(0.72, 0.72, 0.72)),
                Node {
                    margin: UiRect::top(Val::Px(10.0)),
                    ..default()
                },
            ));
            // Spacer
            parent.spawn(Node {
                height: Val::Px(64.0),
                ..default()
            });
            // Start button
            parent
                .spawn((
                    Button,
                    StartButton,
                    Node {
                        padding: UiRect::axes(Val::Px(52.0), Val::Px(18.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.50, 0.80)),
                    BorderRadius::all(Val::Px(6.0)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(t!("start_btn").to_string()),
                        TextFont {
                            font: font.clone(),
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
        });
}

fn start_screen_buttons(
    q: Query<&Interaction, (Changed<Interaction>, With<StartButton>)>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &q {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::CountrySelection);
        }
    }
}

// ─── country selection ────────────────────────────────────────────────────────

fn setup_country_selection(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font: Handle<Font> = asset_server.load("fonts/NotoSansCJKsc-Regular.otf");

    commands
        .spawn((
            CountrySelectionRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            // ── Top bar ──
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(20.0), Val::Px(14.0)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
            ))
            .with_children(|bar| {
                bar.spawn((
                    Text::new(t!("select_country_title").to_string()),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.95, 0.88, 0.55)),
                ));
                // Back button
                bar.spawn((
                    Button,
                    BackButton,
                    Node {
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.25, 0.27, 0.30)),
                    BorderRadius::all(Val::Px(4.0)),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(t!("back_btn").to_string()),
                        TextFont {
                            font: font.clone(),
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            });

            // ── Bottom bar ──
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    padding: UiRect::axes(Val::Px(20.0), Val::Px(14.0)),
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.78)),
            ))
            .with_children(|bar| {
                // Country name label (updated dynamically)
                bar.spawn((
                    Text::new(t!("select_country_hint").to_string()),
                    TextFont {
                        font: font.clone(),
                        font_size: 18.0,
                        ..default()
                    },
                    TextColor(Color::srgb(0.85, 0.85, 0.85)),
                    CountryNameLabel,
                ));
                // "Play as X" button — hidden until a valid country is selected
                bar.spawn((
                    Button,
                    PlayAsButton,
                    Node {
                        padding: UiRect::axes(Val::Px(24.0), Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.22, 0.60, 0.28)),
                    BorderRadius::all(Val::Px(4.0)),
                    Visibility::Hidden,
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new(t!("play_as_btn").to_string()),
                        TextFont {
                            font: font.clone(),
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            });
        });
}

/// Reads the currently selected province and derives the owning country.
fn update_country_selection_panel(
    selected: Res<SelectedProvince>,
    state: Res<LatestGameState>,
    mut selected_country: ResMut<SelectedCountryForPlay>,
    mut labels: Query<&mut Text, With<CountryNameLabel>>,
    mut play_btns: Query<&mut Visibility, With<PlayAsButton>>,
) {
    let hide = |labels: &mut Query<&mut Text, With<CountryNameLabel>>,
                play_btns: &mut Query<&mut Visibility, With<PlayAsButton>>,
                msg: String| {
        for mut t in labels.iter_mut() {
            *t = Text::new(msg.clone());
        }
        for mut vis in play_btns.iter_mut() {
            *vis = Visibility::Hidden;
        }
    };

    let Some(pid) = selected.0 else {
        selected_country.tag = None;
        selected_country.name = None;
        hide(&mut labels, &mut play_btns, t!("select_country_hint").to_string());
        return;
    };

    let Some(gs) = &state.0 else {
        selected_country.tag = None;
        selected_country.name = None;
        hide(&mut labels, &mut play_btns, t!("waiting_state").to_string());
        return;
    };

    let pid_usize = pid as usize;
    if pid_usize >= gs.provinces.len() {
        return;
    }

    let province = &gs.provinces[pid_usize];
    if let Some(owner_tag) = province.owner.as_deref() {
        let country = gs.countries.iter().find(|c| c.tag == owner_tag);
        let country_name = country.map(|c| c.name.as_str()).unwrap_or(owner_tag);
        selected_country.tag = Some(owner_tag.to_string());
        selected_country.name = Some(country_name.to_string());
        for mut t in labels.iter_mut() {
            *t = Text::new(format!("{}：{}", t!("player_country"), country_name));
        }
        for mut vis in play_btns.iter_mut() {
            *vis = Visibility::Visible;
        }
    } else {
        selected_country.tag = None;
        selected_country.name = None;
        hide(&mut labels, &mut play_btns, t!("no_provinces").to_string());
    }
}

fn country_selection_buttons(
    back_q: Query<&Interaction, (Changed<Interaction>, With<BackButton>)>,
    play_q: Query<&Interaction, (Changed<Interaction>, With<PlayAsButton>)>,
    selected_country: Res<SelectedCountryForPlay>,
    mut player_country: ResMut<PlayerCountry>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for interaction in &back_q {
        if *interaction == Interaction::Pressed {
            next_state.set(AppState::StartScreen);
        }
    }
    for interaction in &play_q {
        if *interaction == Interaction::Pressed {
            if let Some(tag) = &selected_country.tag {
                player_country.0 = Some(tag.clone());
                next_state.set(AppState::Playing);
            }
        }
    }
}
