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

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SelectedProvince::default())
            .add_systems(Startup, load_map)
            .add_systems(Update, (color_provinces, camera_controls));
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

/// Update province colors based on game state (owner → color).
fn color_provinces(
    state: Res<LatestGameState>,
    map: Option<Res<MapResource>>,
    query: Query<(&ProvinceEntity, &MeshMaterial2d<ColorMaterial>)>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let Some(gs) = &state.0 else { return };
    let Some(_map) = map else { return };

    for (pe, mat_handle) in query.iter() {
        let province_id = pe.0 as usize;
        if province_id >= gs.provinces.len() {
            continue;
        }

        let owner = gs.provinces[province_id]
            .owner
            .as_deref()
            .unwrap_or("UNK");
        let color = country_color(owner);

        if let Some(mat) = materials.get_mut(&mat_handle.0) {
            mat.color = color;
        }
    }
}

/// Camera pan (right-click drag) and zoom (scroll wheel).
fn camera_controls(
    mouse_input: Res<ButtonInput<MouseButton>>,
    mut scroll_evts: EventReader<bevy::input::mouse::MouseWheel>,
    mut motion_evts: EventReader<bevy::input::mouse::MouseMotion>,
    mut camera_q: Query<(&mut Transform, &OrthographicProjection), With<Camera2d>>,
) {
    let Ok((mut transform, projection)) = camera_q.get_single_mut() else {
        return;
    };

    // Pan with right mouse button.
    if mouse_input.pressed(MouseButton::Right) {
        for ev in motion_evts.read() {
            let scale = projection.scale;
            transform.translation.x -= ev.delta.x * scale;
            transform.translation.y += ev.delta.y * scale;
        }
    } else {
        motion_evts.clear();
    }

    // Zoom with scroll wheel.
    for ev in scroll_evts.read() {
        let Ok((mut transform, _)) = camera_q.get_single_mut() else {
            return;
        };
        let zoom_factor = 1.0 - ev.y * 0.1;
        transform.scale *= Vec3::splat(zoom_factor.clamp(0.5, 2.0));
        transform.scale = transform.scale.clamp(
            Vec3::splat(0.01),
            Vec3::splat(100.0),
        );
    }
}
