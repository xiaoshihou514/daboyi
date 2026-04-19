mod color;
pub mod borders;
mod interact;
mod material;

use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::sprite::Material2dPlugin;
use material::ProvinceMapMaterial;
use shared::conv::{u32_to_usize, unit_f32_to_u8, usize_to_f32, usize_to_u32};
use shared::map::MapData;
use std::collections::{HashMap, HashSet};

use crate::editor::{
    classify_province_for_active_admin, ActiveAdmin, AdminAreas, AdminBrushRelation, AdminMap,
    Countries, CountryMap,
};
use crate::state::AppState;
use color::{brighten, dim, owner_color_rgba, terrain_province_color};
use interact::{camera_controls, province_click};
pub use borders::BordersPlugin;

pub const MAP_BIN_PATH: &str = "assets/map.bin";
/// Equal Earth x-range width: longitude ±180° maps exactly to x ∈ [-180, 180].
pub const MAP_WIDTH: f32 = 360.0;

/// Province tag (lowercased) → Chinese display name.
#[derive(Resource, Default)]
pub struct ProvinceNames(pub HashMap<String, String>);

pub struct MapPlugin;

/// Loaded map geometry, available as a Bevy resource.
#[derive(Resource)]
pub struct MapResource(pub MapData);

/// Currently selected province.
#[derive(Resource, Default)]
pub struct SelectedProvince(pub Option<u32>);

/// Map coloring mode.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum MapMode {
    Province,
    Terrain,
    #[default]
    Political,
}

impl std::fmt::Display for MapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = match self {
            MapMode::Province => "省份",
            MapMode::Terrain => "地形",
            MapMode::Political => "政治",
        };
        write!(f, "{s}")
    }
}

/// Width of the province colour texture in texels (columns).
/// Using 256 keeps the texture within the guaranteed 8192-texel limit and
/// gives a height of ceil(21111/256) = 83 for the expected province count.
const TEX_WIDTH: usize = 256;

/// Colour lookup texture for the province map.
///
/// One texel per province at pixel index `pid` (= province array index):
///   col = pid % TEX_WIDTH
///   row = pid / TEX_WIDTH
#[derive(Resource)]
pub struct ProvinceColorTexture {
    pub handle: Handle<Image>,
}

/// Tracks the last coloring state to avoid redundant recoloring.
#[derive(Resource, Default)]
struct LastColorState {
    mode: Option<MapMode>,
    selected: Option<u32>,
    active_admin: Option<u32>,
    coloring_version: u64,
}

/// Incremented when editor data changes require a full recolor.
#[derive(Resource, Default)]
pub struct ColoringVersion(pub u64);

/// Province IDs whose colours need a targeted texture update this frame.
#[derive(Resource, Default)]
pub struct PendingProvinceRecolor(pub HashSet<u32>);

/// Incremented when ownership or admin assignments change and borders must rebuild.
#[derive(Resource, Default)]
pub struct BorderVersion(pub u64);

/// Set while edits have changed border semantics but the expensive rebuild is intentionally deferred.
#[derive(Resource, Default)]
pub struct BorderDirty(pub bool);

/// Debounce for border rebuilds: fires 150 ms after the last brush stroke ends.
#[derive(Resource)]
pub struct PaintDebounce {
    timer: Timer,
    /// A border rebuild needs to happen once the cooldown expires.
    pub pending_border: bool,
}

impl Default for PaintDebounce {
    fn default() -> Self {
        Self {
            timer: Timer::from_seconds(0.15, TimerMode::Once),
            pending_border: false,
        }
    }
}

impl PaintDebounce {
    /// (Re)start the 150 ms countdown after a stroke ends.
    pub fn kick(&mut self) {
        self.timer.reset();
    }
}

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<ProvinceMapMaterial>::default())
            .insert_resource(SelectedProvince::default())
            .insert_resource(MapMode::default())
            .insert_resource(LastColorState::default())
            .insert_resource(ColoringVersion::default())
            .insert_resource(PendingProvinceRecolor::default())
            .insert_resource(BorderVersion::default())
            .insert_resource(BorderDirty::default())
            .insert_resource(PaintDebounce::default())
            .insert_resource(ProvinceNames::default())
            .add_systems(Startup, load_map)
            .add_systems(
                Update,
                flush_paint_debounce.run_if(in_state(AppState::Editing)),
            )
            .add_systems(
                Update,
                color_provinces
                    .run_if(in_state(AppState::Editing))
                    .after(flush_paint_debounce),
            )
            .add_systems(
                Update,
                (camera_controls,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            )
            .add_systems(
                Update,
                (province_click,)
                    .run_if(in_state(AppState::Editing))
                    .after(crate::ui::UiPass),
            );
    }
}

/// Build a single merged mesh for ALL provinces.
fn load_map(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut province_materials: ResMut<Assets<ProvinceMapMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let map_data = match MapData::load(MAP_BIN_PATH) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to load {MAP_BIN_PATH}: {e}");
            eprintln!("Map will not be rendered. Run mapgen first.");
            next_state.set(AppState::Editing);
            return;
        }
    };

    println!("Loaded map: {} provinces", map_data.provinces.len());

    let n = map_data.provinces.len();
    let tex_height = (n + TEX_WIDTH - 1) / TEX_WIDTH;

    let mut all_positions: Vec<[f32; 3]> = Vec::new();
    let mut all_normals: Vec<[f32; 3]> = Vec::new();
    // UV stores raw texel coordinates [col, row] so the fragment shader can
    // do a direct textureLoad without normalised UV arithmetic.
    let mut all_uvs: Vec<[f32; 2]> = Vec::new();
    let mut all_indices: Vec<u32> = Vec::new();
    // Province colour texture data: one RGBA8 pixel per province at offset pid*4.
    let mut pixel_data = vec![0u8; tex_height * TEX_WIDTH * 4];

    for (pid, mp) in map_data.provinces.iter().enumerate() {
        // Write province base colour into the texture.
        let c = mp.hex_color;
        let offset = pid * 4;
        pixel_data[offset] = unit_f32_to_u8(c[0]);
        pixel_data[offset + 1] = unit_f32_to_u8(c[1]);
        pixel_data[offset + 2] = unit_f32_to_u8(c[2]);
        pixel_data[offset + 3] = unit_f32_to_u8(c[3]);

        if mp.vertices.is_empty() || mp.indices.is_empty() {
            continue;
        }

        let col_f = usize_to_f32(pid % TEX_WIDTH);
        let row_f = usize_to_f32(pid / TEX_WIDTH);
        let base_idx = usize_to_u32(all_positions.len());

        for v in &mp.vertices {
            all_positions.push([v[0], v[1], 0.0]);
            all_normals.push([0.0, 0.0, 1.0]);
            all_uvs.push([col_f, row_f]);
        }
        for idx in &mp.indices {
            all_indices.push(idx + base_idx);
        }
    }

    println!(
        "Map mesh: {} vertices, {} triangles",
        all_positions.len(),
        all_indices.len() / 3
    );

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, all_positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, all_normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, all_uvs);
    mesh.insert_indices(Indices::U32(all_indices));

    let mesh_handle = meshes.add(mesh);

    // Province colour lookup texture: 256 × ceil(N/256), Rgba8Unorm.
    let mut color_image = Image::new(
        Extent3d {
            width: usize_to_u32(TEX_WIDTH),
            height: usize_to_u32(tex_height),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixel_data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    color_image.sampler = ImageSampler::nearest();
    let color_tex_handle = images.add(color_image);

    let material_handle = province_materials.add(ProvinceMapMaterial {
        color_texture: color_tex_handle.clone(),
    });

    for &x_offset in &[-MAP_WIDTH, 0.0, MAP_WIDTH] {
        commands.spawn((
            Mesh2d(mesh_handle.clone()),
            MeshMaterial2d(material_handle.clone()),
            Transform::from_xyz(x_offset, 0.0, 0.0),
        ));
    }

    commands.insert_resource(ProvinceColorTexture {
        handle: color_tex_handle,
    });

    let mut stripped_map = map_data;
    for province in &mut stripped_map.provinces {
        province.vertices.clear();
        province.indices.clear();
        province.vertices.shrink_to_fit();
        province.indices.shrink_to_fit();
    }
    commands.insert_resource(MapResource(stripped_map));

    // Load province names (official Chinese translations).
    let mut province_name_map: HashMap<String, String> = HashMap::new();
    if let Ok(content) = std::fs::read_to_string("assets/province_names.tsv") {
        for line in content.lines() {
            let mut parts = line.splitn(2, '\t');
            if let (Some(en), Some(zh)) = (parts.next(), parts.next()) {
                province_name_map.insert(en.trim().to_lowercase(), zh.trim().to_string());
            }
        }
        println!("已加载 {} 个省份名称", province_name_map.len());
    }
    commands.insert_resource(ProvinceNames(province_name_map));

    next_state.set(AppState::Editing);
}

/// Ticks the paint debounce timer; bumps `BorderVersion` when it fires.
fn flush_paint_debounce(
    time: Res<Time>,
    mut debounce: ResMut<PaintDebounce>,
    mut border_version: ResMut<BorderVersion>,
) {
    if !debounce.pending_border {
        return;
    }
    debounce.timer.tick(time.delta());
    if debounce.timer.just_finished() {
        debounce.pending_border = false;
        border_version.0 += 1;
    }
}

/// Bundles last-state tracking into a single SystemParam.
#[derive(SystemParam)]
struct ColoringGuard<'w> {
    coloring_version: Res<'w, ColoringVersion>,
    last: ResMut<'w, LastColorState>,
}

/// Write province RGBA8 into the colour texture at pixel offset `pid * 4`.
fn write_color(data: &mut [u8], pid: usize, color: [f32; 4]) {
    let offset = pid * 4;
    if offset + 4 > data.len() {
        return;
    }
    data[offset] = unit_f32_to_u8(color[0]);
    data[offset + 1] = unit_f32_to_u8(color[1]);
    data[offset + 2] = unit_f32_to_u8(color[2]);
    data[offset + 3] = unit_f32_to_u8(color[3]);
}

/// Recolor the province colour texture based on current mode and coloring assignments.
///
/// Colour updates are cheap (~84 KB texture write) so they happen immediately —
/// no deferral needed.  Only `pending_border` / border rebuilds remain debounced.
fn color_provinces(
    map: Option<Res<MapResource>>,
    color_tex: Option<Res<ProvinceColorTexture>>,
    mode: Res<MapMode>,
    selected: Res<SelectedProvince>,
    active_admin: Res<ActiveAdmin>,
    country_map: Res<CountryMap>,
    countries: Res<Countries>,
    admin_areas: Res<AdminAreas>,
    admin_assignments: Res<AdminMap>,
    mut guard: ColoringGuard,
    mut pending_province_recolor: ResMut<PendingProvinceRecolor>,
    mut images: ResMut<Assets<Image>>,
) {
    let (Some(map), Some(ct)) = (map, color_tex) else {
        return;
    };

    let mode_changed = guard.last.mode != Some(*mode);
    let selection_changed = guard.last.selected != selected.0;
    let active_admin_changed = guard.last.active_admin != active_admin.0;
    let coloring_changed = guard.last.coloring_version != guard.coloring_version.0;
    let has_pending_province = !pending_province_recolor.0.is_empty();

    let needs_full_recolor = mode_changed || coloring_changed || active_admin_changed;

    if !needs_full_recolor && !selection_changed && !has_pending_province {
        return;
    }

    // Build a quick lookup: tag → color for editor countries.
    let country_color_lookup: HashMap<&str, [f32; 4]> = countries
        .0
        .iter()
        .map(|c| (c.tag.as_str(), c.color))
        .collect();

    let base_color = |pid: usize| -> [f32; 4] {
        match *mode {
            MapMode::Province => map.0.provinces[pid].hex_color,
            MapMode::Terrain => terrain_province_color(&map.0.provinces[pid].topography),
            MapMode::Political => {
                let topo = &map.0.provinces[pid].topography;
                let prov_id = map.0.provinces[pid].id;

                if let Some(&area_id) = admin_assignments.0.get(&prov_id) {
                    let area_color = resolve_area_color(area_id, &admin_areas.0, &country_color_lookup);
                    if topo.contains("wasteland") {
                        let wc = terrain_province_color(topo);
                        return [
                            (wc[0] + area_color[0]) * 0.5,
                            (wc[1] + area_color[1]) * 0.5,
                            (wc[2] + area_color[2]) * 0.5,
                            1.0,
                        ];
                    }
                    return area_color;
                }

                if let Some(tag) = country_map.0.get(&prov_id) {
                    let country_color = country_color_lookup
                        .get(tag.as_str())
                        .copied()
                        .unwrap_or_else(|| owner_color_rgba(tag));

                    if topo.contains("wasteland") {
                        let wc = terrain_province_color(topo);
                        return [
                            (wc[0] + country_color[0]) * 0.5,
                            (wc[1] + country_color[1]) * 0.5,
                            (wc[2] + country_color[2]) * 0.5,
                            1.0,
                        ];
                    }
                    return country_color;
                }

                if topo.contains("wasteland") {
                    terrain_province_color(topo)
                } else {
                    [0.55, 0.55, 0.55, 1.0]
                }
            }
        }
    };

    let scoped_color = |pid: usize| -> [f32; 4] {
        let base = base_color(pid);
        let Some(selected_admin_id) = active_admin.0 else {
            return base;
        };
        if *mode != MapMode::Political {
            return base;
        }
        let prov_id = map.0.provinces[pid].id;
        match classify_province_for_active_admin(
            selected_admin_id,
            &admin_areas.0,
            &admin_assignments,
            &country_map,
            prov_id,
        ) {
            Some(AdminBrushRelation::Selected) | Some(AdminBrushRelation::Sibling) => base,
            Some(AdminBrushRelation::Unclaimed) => [0.70, 0.70, 0.70, 1.0],
            None => dim(base, 0.25),
        }
    };

    if needs_full_recolor {
        // Full texture rewrite (~84 KB): recompute every province's colour.
        let Some(image) = images.get_mut(&ct.handle) else {
            return;
        };
        for pid in 0..map.0.provinces.len() {
            let is_selected = selected.0 == Some(usize_to_u32(pid));
            let base = scoped_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            write_color(&mut image.data, pid, color);
        }
        guard.last.mode = Some(*mode);
        guard.last.selected = selected.0;
        guard.last.active_admin = active_admin.0;
        guard.last.coloring_version = guard.coloring_version.0;
        pending_province_recolor.0.clear();
        return;
    }

    // Targeted province update (during/after brush drag): only update
    // the provinces that actually changed this frame.
    if has_pending_province {
        let Some(image) = images.get_mut(&ct.handle) else {
            return;
        };
        // Drain pending set; also re-apply selection highlight if the
        // selected province is among the changed ones.
        let ids: Vec<u32> = pending_province_recolor.0.drain().collect();
        for prov_id in ids {
            let pid = u32_to_usize(prov_id);
            if pid >= map.0.provinces.len() {
                continue;
            }
            let is_selected = selected.0 == Some(prov_id);
            let base = scoped_color(pid);
            let color = if is_selected { brighten(base) } else { base };
            write_color(&mut image.data, pid, color);
        }
        // Selection state hasn't changed — keep last.selected current.
        return;
    }

    // Selection-only update: restore old selected province, highlight new one.
    if selection_changed {
        let Some(image) = images.get_mut(&ct.handle) else {
            return;
        };
        if let Some(old_u32) = guard.last.selected {
            let old_pid = u32_to_usize(old_u32);
            if old_pid < map.0.provinces.len() {
                write_color(&mut image.data, old_pid, scoped_color(old_pid));
            }
        }
        if let Some(new_u32) = selected.0 {
            let new_pid = u32_to_usize(new_u32);
            if new_pid < map.0.provinces.len() {
                write_color(&mut image.data, new_pid, brighten(scoped_color(new_pid)));
            }
        }
        guard.last.selected = selected.0;
    }
}

/// Walk the area parent chain to find the first explicit color, falling back to country color.
fn resolve_area_color(
    area_id: u32,
    admin_areas: &[shared::AdminArea],
    country_color_lookup: &HashMap<&str, [f32; 4]>,
) -> [f32; 4] {
    let mut current = area_id;
    loop {
        if let Some(area) = admin_areas.iter().find(|a| a.id == current) {
            if let Some(col) = area.color {
                return col;
            }
            match area.parent_id {
                Some(pid) => current = pid,
                None => {
                    return country_color_lookup
                        .get(area.country_tag.as_str())
                        .copied()
                        .unwrap_or([0.55, 0.55, 0.55, 1.0]);
                }
            }
        } else {
            return [0.55, 0.55, 0.55, 1.0];
        }
    }
}
