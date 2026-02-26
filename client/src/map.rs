use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::map::{MapData, MapProvince};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::net::LatestGameState;

const MAP_BIN_PATH: &str = "assets/map.bin";

pub struct MapPlugin;

/// Loaded map geometry, available as a Bevy resource.
#[derive(Resource)]
pub struct MapResource(pub MapData);

/// Tag component linking a mesh entity to a province ID.
#[derive(Component)]
pub struct ProvinceEntity(pub u32);

/// Currently selected province.
#[derive(Resource, Default)]
pub struct SelectedProvince(pub Option<u32>);

/// Map coloring mode, switchable with keys 1/2/3.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MapMode {
    #[default]
    Political,
    Population,
    Production,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MapMode::Political => write!(f, "Political"),
            MapMode::Population => write!(f, "Population"),
            MapMode::Production => write!(f, "Production"),
        }
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SelectedProvince::default())
            .insert_resource(MapMode::default())
            .add_systems(Startup, load_map)
            .add_systems(Update, (
                color_provinces,
                camera_controls,
                province_click,
                map_mode_switch,
            ));
    }
}

/// Country code → deterministic color.
fn country_color(code: &str) -> Color {
    let mut hasher = DefaultHasher::new();
    code.hash(&mut hasher);
    let h = hasher.finish();
    let r = ((h >> 0) & 0xFF) as f32 / 255.0 * 0.6 + 0.2;
    let g = ((h >> 8) & 0xFF) as f32 / 255.0 * 0.6 + 0.2;
    let b = ((h >> 16) & 0xFF) as f32 / 255.0 * 0.6 + 0.2;
    Color::srgb(r, g, b)
}

/// Heatmap: 0.0 → dark blue, 0.5 → green, 1.0 → red.
fn heatmap_color(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let r = (2.0 * t - 0.5).clamp(0.0, 1.0);
    let g = (1.0 - (2.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    let b = (1.0 - 2.0 * t).clamp(0.0, 1.0);
    Color::srgb(r * 0.8 + 0.1, g * 0.8 + 0.1, b * 0.8 + 0.1)
}

fn build_province_mesh(mp: &MapProvince) -> Mesh {
    let positions: Vec<[f32; 3]> = mp.vertices.iter().map(|v| [v[0], v[1], 0.0]).collect();
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0]; positions.len()];
    let uvs: Vec<[f32; 2]> = mp.vertices.to_vec();

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(mp.indices.clone()));
    mesh
}

fn load_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let map_data = match MapData::load(MAP_BIN_PATH) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to load {MAP_BIN_PATH}: {e}");
            eprintln!("Map will not be rendered. Run mapgen first.");
            return;
        }
    };

    println!("Loaded map: {} provinces", map_data.provinces.len());

    for mp in &map_data.provinces {
        if mp.vertices.is_empty() || mp.indices.is_empty() {
            continue;
        }

        let mesh = build_province_mesh(mp);
        let color = country_color(&mp.country_code);

        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(color))),
            Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)),
            ProvinceEntity(mp.id),
        ));
    }

    commands.insert_resource(MapResource(map_data));
}

/// Update province colors based on game state and current map mode.
fn color_provinces(
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    mode: Res<MapMode>,
    selected: Res<SelectedProvince>,
    query: Query<(&ProvinceEntity, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(gs) = &state.0 else { return };
    let Some(_map) = map else { return };

    // Pre-compute max values for heatmap normalization.
    let (max_pop, max_prod) = if *mode != MapMode::Political {
        let mut mp = 0u32;
        let mut mprod = 0.0f32;
        for p in &gs.provinces {
            let total_pop: u32 = p.pops.iter().map(|pop| pop.size).sum();
            mp = mp.max(total_pop);
            let total_prod: f32 = p.stockpile.values().sum();
            mprod = mprod.max(total_prod);
        }
        (mp.max(1), mprod.max(1.0))
    } else {
        (1, 1.0)
    };

    for (pe, mat_handle) in query.iter() {
        let pid = pe.0 as usize;
        if pid >= gs.provinces.len() {
            continue;
        }

        let province = &gs.provinces[pid];
        let is_selected = selected.0 == Some(pe.0);

        let base_color = match *mode {
            MapMode::Political => {
                let owner = province.owner.as_deref().unwrap_or("UNK");
                country_color(owner)
            }
            MapMode::Population => {
                let total: u32 = province.pops.iter().map(|p| p.size).sum();
                heatmap_color(total as f32 / max_pop as f32)
            }
            MapMode::Production => {
                let total: f32 = province.stockpile.values().sum();
                heatmap_color(total / max_prod)
            }
        };

        let color = if is_selected {
            if let Color::Srgba(s) = base_color {
                Color::srgb(
                    (s.red + 0.25).min(1.0),
                    (s.green + 0.25).min(1.0),
                    (s.blue + 0.25).min(1.0),
                )
            } else {
                base_color
            }
        } else {
            base_color
        };

        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.color = color;
        }
    }
}

/// Camera pan (right-click drag) and zoom (scroll wheel via OrthographicProjection).
fn camera_controls(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut scroll_evts: EventReader<bevy::input::mouse::MouseWheel>,
    mut motion_evts: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera_q: Query<(&mut Transform, &mut OrthographicProjection), With<Camera2d>>,
) {
    let Ok((mut transform, mut projection)) = camera_q.get_single_mut() else {
        return;
    };

    // Pan with right mouse button.
    if mouse_input.pressed(MouseButton::Right) {
        for ev in motion_evts.read() {
            transform.translation.x -= ev.delta.x * projection.scale;
            transform.translation.y += ev.delta.y * projection.scale;
        }
    } else {
        motion_evts.clear();
    }

    // Zoom with scroll wheel.
    for ev in scroll_evts.read() {
        let zoom_factor = 1.0 - ev.y * 0.1;
        projection.scale *= zoom_factor.clamp(0.5, 2.0);
        projection.scale = projection.scale.clamp(0.01, 10.0);
    }
}

/// Point-in-polygon test (ray casting algorithm).
fn point_in_polygon(px: f32, py: f32, ring: &[[f32; 2]]) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (ring[i][0], ring[i][1]);
        let (xj, yj) = (ring[j][0], ring[j][1]);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Check if a point is inside a province polygon (outer ring minus holes).
fn point_in_province(px: f32, py: f32, mp: &MapProvince) -> bool {
    if mp.boundary.is_empty() {
        return false;
    }
    if !point_in_polygon(px, py, &mp.boundary[0]) {
        return false;
    }
    for hole in mp.boundary.iter().skip(1) {
        if point_in_polygon(px, py, hole) {
            return false;
        }
    }
    true
}

/// Detect left-click on a province and update SelectedProvince.
fn province_click(
    mouse_input: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    map: Option<Res<MapResource>>,
    mut selected: ResMut<SelectedProvince>,
) {
    if !mouse_input.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(map) = map else { return };
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = camera_q.get_single() else {
        return;
    };
    let Ok(world_pos) = camera.viewport_to_world_2d(cam_transform, cursor_pos) else {
        return;
    };

    for mp in &map.0.provinces {
        if point_in_province(world_pos.x, world_pos.y, mp) {
            selected.0 = Some(mp.id);
            return;
        }
    }
    selected.0 = None;
}

/// Keyboard shortcuts: 1 = Political, 2 = Population, 3 = Production.
fn map_mode_switch(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<MapMode>) {
    if keys.just_pressed(KeyCode::Digit1) {
        *mode = MapMode::Political;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        *mode = MapMode::Population;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        *mode = MapMode::Production;
    }
}
