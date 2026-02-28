use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use shared::conv::usize_to_u32;
use shared::map::TerrainData;

const TERRAIN_BIN_PATH: &str = "assets/terrain.bin";
const RIVERS_PNG_PATH: &str = "rivers.png";
/// Three copies of the 360°-wide world for seamless horizontal wrapping.
const WORLD_OFFSETS: [f32; 3] = [-360.0, 0.0, 360.0];
/// World height in degrees (equirectangular: 180°).
const MAP_HEIGHT: f32 = 180.0;
/// World width in degrees (equirectangular: 360°).
const MAP_WIDTH: f32 = 360.0;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (load_terrain, spawn_rivers));
    }
}

/// Build a single merged mesh from all terrain polygons and spawn three copies.
fn load_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let terrain = match TerrainData::load(TERRAIN_BIN_PATH) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to load {TERRAIN_BIN_PATH}: {e}");
            eprintln!("Terrain will not be rendered. Run mapgen first.");
            return;
        }
    };

    // Count total vertices/indices up front for pre-allocation.
    let total_verts: usize = terrain.polygons.iter().map(|p| p.vertices.len()).sum();
    let total_idxs: usize = terrain.polygons.iter().map(|p| p.indices.len()).sum();

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(total_verts);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(total_verts);
    let mut indices: Vec<u32> = Vec::with_capacity(total_idxs);

    for poly in &terrain.polygons {
        let base = usize_to_u32(positions.len());
        for &[x, y] in &poly.vertices {
            positions.push([x, y, 0.0]);
            colors.push(poly.color);
        }
        for &i in &poly.indices {
            indices.push(i + base);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));

    let handle = meshes.add(mesh);
    let material = materials.add(ColorMaterial::default());

    for &x_off in &WORLD_OFFSETS {
        commands.spawn((
            Mesh2d(handle.clone()),
            MeshMaterial2d(material.clone()),
            Transform::from_xyz(x_off, 0.0, -1.0),
        ));
    }

    eprintln!(
        "Terrain: {} polygons, {} vertices",
        terrain.polygons.len(),
        total_verts,
    );
}

/// Spawn the rivers PNG overlay as three sprites covering the world copies.
fn spawn_rivers(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
) {
    let rivers_handle: Handle<Image> = asset_server.load(RIVERS_PNG_PATH);

    for &x_off in &WORLD_OFFSETS {
        // Sprite is rendered in world units: 360 wide × 180 tall, centered at (x_off, 0).
        commands.spawn((
            Sprite {
                image: rivers_handle.clone(),
                custom_size: Some(Vec2::new(MAP_WIDTH, MAP_HEIGHT)),
                ..default()
            },
            Transform::from_xyz(x_off, 0.0, 0.5),
        ));
    }
}
