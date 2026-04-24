use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::editor::{AdminAreas, AdminMap, Countries, CountryMap, NonPlayableProvinces};
use crate::map::borders::ProvinceAdjacency;
use crate::map::{MapResource, ProvinceNames, MAP_WIDTH};
use crate::state::AppState;
use crate::ui::ProvinceLabelFont;
use crate::ui::{LoadingProgress, LoadingStage};

const LABEL_FONT_BASE_SIZE: f32 = 48.0;
const NEARBY_COMPONENT_GAP: f32 = 2.0;
const PROVINCE_LABEL_SCALE_MAX: f32 = 0.01;
const MULTI_CHAR_ADVANCE_UNITS: f32 = 1.28;
const MULTI_CHAR_SIDE_PADDING_UNITS: f32 = 0.55;
const BASE_LABEL_HEIGHT_UNITS: f32 = 1.2;
const THIN_REGION_SLENDERNESS_START: f32 = 2.5;
const THIN_REGION_SLENDERNESS_RANGE: f32 = 3.5;
const THIN_REGION_EXTRA_HEIGHT_UNITS: f32 = 0.45;
const REGION_SQUAREISH_SLENDERNESS: f32 = 1.6;
const REGION_AXIS_FOLLOW_SLENDERNESS: f32 = 3.0;
const REGION_SQUAREISH_TILT_FRACTION: f32 = 0.35;
const REGION_LABEL_MAX_VIEWPORT_WIDTH_FRACTION: f32 = 0.38;
const REGION_LABEL_MAX_VIEWPORT_HEIGHT_FRACTION: f32 = 0.16;
const PROVINCE_LABEL_FIT_MARGIN: f32 = 0.84;
const REGION_LABEL_FIT_MARGIN: f32 = 0.9;
const PROVINCE_COLLISION_PADDING_UNITS: f32 = 0.18;
const REGION_COLLISION_PADDING_UNITS: f32 = 0.08;
pub struct LabelsPlugin;

impl Plugin for LabelsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProvinceLabelCache>()
            .init_resource::<OwnershipLabelCache>()
            .init_resource::<LabelStartupState>()
            .init_resource::<ActiveLabelEntities>()
            .add_systems(
                Update,
                (
                    build_province_label_cache,
                    rebuild_ownership_label_cache.after(crate::map::borders::BorderAdjacencyPass),
                    update_world_labels.after(crate::map::camera_controls),
                )
                    .run_if(in_state(AppState::Editing)),
            )
            .add_systems(
                Update,
                (
                    build_province_label_cache,
                    rebuild_ownership_label_cache.after(crate::map::borders::BorderAdjacencyPass),
                    update_world_labels,
                    update_label_loading_progress,
                )
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LabelPriority {
    Province,
    Country,
    Admin,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum LabelKey {
    Province(u32),
    CountryPart(String, u32),
    AdminPart(u32, u32),
}

#[derive(Component)]
struct MapLabelMarker;

#[derive(Resource, Default)]
struct ActiveLabelEntities(pub HashMap<LabelKey, Entity>);

#[derive(Clone)]
struct LabelGeometry {
    center: [f32; 2],
    angle: f32,
    axis_length: f32,
    perp_span: f32,
    min_span: f32,
    aabb_min: [f32; 2],
    aabb_max: [f32; 2],
    single_char: bool,
}

#[derive(Clone)]
struct ProvinceLabelEntry {
    province_id: u32,
    text: String,
    geometry: LabelGeometry,
}

#[derive(Clone)]
struct RegionLabelEntry {
    key: LabelKey,
    text: String,
    geometry: LabelGeometry,
    priority: LabelPriority,
}

#[derive(Clone, Default, Resource)]
struct ProvinceLabelCache {
    entries: Vec<ProvinceLabelEntry>,
}

#[derive(Clone, Default, Resource)]
struct OwnershipLabelCache {
    countries: Vec<RegionLabelEntry>,
    admins: Vec<RegionLabelEntry>,
}

#[derive(Resource, Default)]
struct LabelStartupState {
    province_ready: bool,
    ownership_ready: bool,
}

#[derive(Clone)]
struct VisibleLabel {
    key: LabelKey,
    text: String,
    center: [f32; 2],
    angle: f32,
    font_world_size: f32,
    font_pixels: f32,
    bounds_width_units: f32,
    bounds_height_units: f32,
    collision_padding_units: f32,
    priority: LabelPriority,
}

enum LabelCandidateOutcome {
    Visible(VisibleLabel),
    OffViewport,
}

struct AcceptedLabelBounds {
    bounds: ([f32; 2], [f32; 2]),
}

fn build_province_label_cache(
    map: Option<Res<MapResource>>,
    province_names: Res<ProvinceNames>,
    non_playable_provinces: Res<NonPlayableProvinces>,
    mut cache: ResMut<ProvinceLabelCache>,
    mut startup_state: ResMut<LabelStartupState>,
) {
    let Some(map) = map else { return };
    if startup_state.province_ready
        && !map.is_changed()
        && !province_names.is_changed()
        && !non_playable_provinces.is_changed()
    {
        return;
    }

    let mut entries = Vec::new();
    for province in &map.0.provinces {
        if non_playable_provinces.0.contains(&province.id) || province.boundary.is_empty() {
            continue;
        }
        let text = province_names
            .0
            .get(&province.tag.to_lowercase())
            .cloned()
            .unwrap_or_else(|| province.name.clone());
        let points = boundary_points(&province.boundary);
        let single_char = text.chars().count() == 1;
        let Some(geometry) = compute_label_geometry(&points, province.centroid, single_char) else {
            continue;
        };
        entries.push(ProvinceLabelEntry {
            province_id: province.id,
            text,
            geometry,
        });
    }
    cache.entries = entries;
    startup_state.province_ready = true;
}

fn rebuild_ownership_label_cache(
    map: Option<Res<MapResource>>,
    adjacency: Res<ProvinceAdjacency>,
    countries: Res<Countries>,
    country_map: Res<CountryMap>,
    admin_areas: Res<AdminAreas>,
    admin_map: Res<AdminMap>,
    non_playable_provinces: Res<NonPlayableProvinces>,
    mut cache: ResMut<OwnershipLabelCache>,
    mut startup_state: ResMut<LabelStartupState>,
) {
    let Some(map) = map else { return };
    let changed = cache.countries.is_empty()
        || adjacency.is_changed()
        || countries.is_changed()
        || country_map.is_changed()
        || admin_areas.is_changed()
        || admin_map.is_changed()
        || non_playable_provinces.is_changed();
    if !changed {
        return;
    }

    let mut neighbors: HashMap<u32, Vec<u32>> = HashMap::new();
    for border in &adjacency.0 {
        let left = border.provinces[0];
        let right = border.provinces[1];
        neighbors.entry(left).or_default().push(right);
        neighbors.entry(right).or_default().push(left);
    }

    let mut country_name_by_tag: HashMap<&str, &str> = HashMap::new();
    for country in &countries.0 {
        country_name_by_tag.insert(country.tag.as_str(), country.name.as_str());
    }

    let mut admin_name_by_id: HashMap<u32, &str> = HashMap::new();
    for area in &admin_areas.0 {
        admin_name_by_id.insert(area.id, area.name.as_str());
    }

    cache.countries = build_region_entries_from_country_map(
        &map.0,
        &neighbors,
        &country_map.0,
        &country_name_by_tag,
        &non_playable_provinces.0,
    );
    cache.admins = build_region_entries_from_admin_map(
        &map.0,
        &neighbors,
        &admin_map.0,
        &admin_name_by_id,
        &non_playable_provinces.0,
    );
    startup_state.ownership_ready = true;
}

fn update_world_labels(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Transform, &OrthographicProjection), With<Camera2d>>,
    province_font: Option<Res<ProvinceLabelFont>>,
    province_cache: Res<ProvinceLabelCache>,
    ownership_cache: Res<OwnershipLabelCache>,
    mut entities: ResMut<ActiveLabelEntities>,
) {
    let Some(province_font) = province_font else {
        return;
    };
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Ok((camera_transform, projection)) = camera_q.get_single() else {
        return;
    };

    let viewport = viewport_bounds(camera_transform.translation, projection.scale, window);
    let mut accepted_boxes: Vec<AcceptedLabelBounds> = Vec::new();
    let mut desired_labels: Vec<VisibleLabel> = Vec::new();

    let mut province_candidates: Vec<VisibleLabel> = Vec::new();
    if projection.scale <= PROVINCE_LABEL_SCALE_MAX {
        for entry in &province_cache.entries {
            match visible_label_from_geometry(
                LabelKey::Province(entry.province_id),
                entry.text.clone(),
                entry.geometry.clone(),
                LabelPriority::Province,
                projection.scale,
                [window.width(), window.height()],
                camera_transform.translation.x,
                viewport,
            ) {
                LabelCandidateOutcome::Visible(label) => {
                    province_candidates.push(label);
                }
                LabelCandidateOutcome::OffViewport => {}
            }
        }
    }
    province_candidates.sort_by(compare_visible_labels);
    for candidate in province_candidates {
        let bounds = collision_label_bounds(&candidate);
        accepted_boxes.push(AcceptedLabelBounds { bounds });
        desired_labels.push(candidate);
    }

    let mut country_candidates: Vec<VisibleLabel> = Vec::new();
    for entry in &ownership_cache.countries {
        match visible_label_from_geometry(
            entry.key.clone(),
            entry.text.clone(),
            entry.geometry.clone(),
            entry.priority,
            projection.scale,
            [window.width(), window.height()],
            camera_transform.translation.x,
            viewport,
        ) {
            LabelCandidateOutcome::Visible(label) => country_candidates.push(label),
            LabelCandidateOutcome::OffViewport => {}
        }
    }
    country_candidates.sort_by(compare_visible_labels);
    for candidate in country_candidates {
        let bounds = collision_label_bounds(&candidate);
        if overlapping_label(bounds, &accepted_boxes).is_some() {
            continue;
        }
        accepted_boxes.push(AcceptedLabelBounds { bounds });
        desired_labels.push(candidate);
    }

    let mut admin_candidates: Vec<VisibleLabel> = Vec::new();
    for entry in &ownership_cache.admins {
        match visible_label_from_geometry(
            entry.key.clone(),
            entry.text.clone(),
            entry.geometry.clone(),
            entry.priority,
            projection.scale,
            [window.width(), window.height()],
            camera_transform.translation.x,
            viewport,
        ) {
            LabelCandidateOutcome::Visible(label) => admin_candidates.push(label),
            LabelCandidateOutcome::OffViewport => {}
        }
    }
    admin_candidates.sort_by(compare_visible_labels);
    for candidate in admin_candidates {
        let bounds = collision_label_bounds(&candidate);
        if overlapping_label(bounds, &accepted_boxes).is_some() {
            continue;
        }
        accepted_boxes.push(AcceptedLabelBounds { bounds });
        desired_labels.push(candidate);
    }

    let desired_keys: HashSet<LabelKey> = desired_labels
        .iter()
        .map(|label| label.key.clone())
        .collect();
    let stale_keys: Vec<LabelKey> = entities
        .0
        .keys()
        .filter(|key| !desired_keys.contains(*key))
        .cloned()
        .collect();
    for key in stale_keys {
        if let Some(entity) = entities.0.remove(&key) {
            commands.entity(entity).despawn();
        }
    }

    for label in desired_labels {
        let transform = Transform {
            translation: Vec3::new(label.center[0], label.center[1], label_z(label.priority)),
            rotation: Quat::from_rotation_z(label.angle),
            scale: Vec3::splat(label.font_world_size / LABEL_FONT_BASE_SIZE),
        };
        let text_component = Text2d::new(label.text.clone());
        let text_font = TextFont {
            font: province_font.0.clone(),
            font_size: LABEL_FONT_BASE_SIZE,
            ..default()
        };
        let text_color = TextColor(Color::srgba(0.05, 0.05, 0.05, 0.92));
        if let Some(entity) = entities.0.get(&label.key).copied() {
            commands.entity(entity).insert((
                text_component,
                text_font,
                text_color,
                transform,
                Visibility::Visible,
            ));
        } else {
            let entity = commands
                .spawn((
                    text_component,
                    text_font,
                    text_color,
                    transform,
                    Visibility::Visible,
                    MapLabelMarker,
                ))
                .id();
            entities.0.insert(label.key, entity);
        }
    }
}

fn update_label_loading_progress(
    startup_state: Res<LabelStartupState>,
    mut progress: ResMut<LoadingProgress>,
) {
    if startup_state.province_ready && startup_state.ownership_ready {
        progress.labels = LoadingStage::Ready;
    } else {
        progress.labels = LoadingStage::Working {
            label: "正在构建标签缓存".to_string(),
            progress: 0.6,
        };
    }
}

fn build_region_entries_from_country_map(
    map: &shared::map::MapData,
    neighbors: &HashMap<u32, Vec<u32>>,
    country_map: &HashMap<u32, String>,
    country_names: &HashMap<&str, &str>,
    non_playable_provinces: &HashSet<u32>,
) -> Vec<RegionLabelEntry> {
    let mut province_ids_by_tag: HashMap<&str, Vec<u32>> = HashMap::new();
    for (&province_id, tag) in country_map {
        if non_playable_provinces.contains(&province_id) {
            continue;
        }
        province_ids_by_tag
            .entry(tag.as_str())
            .or_default()
            .push(province_id);
    }

    let mut entries = Vec::new();
    for (tag, province_ids) in province_ids_by_tag {
        let Some(name) = country_names.get(tag).copied() else {
            continue;
        };
        let components = merge_nearby_components(
            map,
            connected_components(&province_ids, neighbors),
            NEARBY_COMPONENT_GAP,
        );
        for (component_index, component) in components.into_iter().enumerate() {
            let Some((points, centroid)) = component_points_and_centroid(map, &component) else {
                continue;
            };
            let single_char = name.chars().count() == 1;
            let Some(geometry) = compute_label_geometry(&points, centroid, single_char) else {
                continue;
            };
            entries.push(RegionLabelEntry {
                key: LabelKey::CountryPart(tag.to_owned(), component_index as u32),
                text: name.to_owned(),
                geometry,
                priority: LabelPriority::Country,
            });
        }
    }
    entries
}

fn build_region_entries_from_admin_map(
    map: &shared::map::MapData,
    neighbors: &HashMap<u32, Vec<u32>>,
    admin_map: &HashMap<u32, u32>,
    admin_names: &HashMap<u32, &str>,
    non_playable_provinces: &HashSet<u32>,
) -> Vec<RegionLabelEntry> {
    let mut province_ids_by_admin: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&province_id, &admin_id) in admin_map {
        if non_playable_provinces.contains(&province_id) {
            continue;
        }
        province_ids_by_admin
            .entry(admin_id)
            .or_default()
            .push(province_id);
    }

    let mut entries = Vec::new();
    for (admin_id, province_ids) in province_ids_by_admin {
        let Some(name) = admin_names.get(&admin_id).copied() else {
            continue;
        };
        let components = merge_nearby_components(
            map,
            connected_components(&province_ids, neighbors),
            NEARBY_COMPONENT_GAP,
        );
        for (component_index, component) in components.into_iter().enumerate() {
            let Some((points, centroid)) = component_points_and_centroid(map, &component) else {
                continue;
            };
            let single_char = name.chars().count() == 1;
            let Some(geometry) = compute_label_geometry(&points, centroid, single_char) else {
                continue;
            };
            entries.push(RegionLabelEntry {
                key: LabelKey::AdminPart(admin_id, component_index as u32),
                text: name.to_owned(),
                geometry,
                priority: LabelPriority::Admin,
            });
        }
    }
    entries
}

fn connected_components(province_ids: &[u32], neighbors: &HashMap<u32, Vec<u32>>) -> Vec<Vec<u32>> {
    let allowed: HashSet<u32> = province_ids.iter().copied().collect();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut components = Vec::new();

    for &province_id in province_ids {
        if visited.contains(&province_id) {
            continue;
        }
        let mut queue = VecDeque::new();
        let mut component = Vec::new();
        queue.push_back(province_id);
        visited.insert(province_id);

        while let Some(current) = queue.pop_front() {
            component.push(current);
            if let Some(adjacents) = neighbors.get(&current) {
                for &neighbor in adjacents {
                    if allowed.contains(&neighbor) && visited.insert(neighbor) {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        components.push(component);
    }
    components
}

fn merge_nearby_components(
    map: &shared::map::MapData,
    components: Vec<Vec<u32>>,
    gap_threshold: f32,
) -> Vec<Vec<u32>> {
    if components.len() < 2 {
        return components;
    }

    let component_bounds: Vec<([f32; 2], [f32; 2])> = components
        .iter()
        .map(|component| component_bounds_for_provinces(map, component))
        .collect();

    let mut component_neighbors: Vec<Vec<usize>> = vec![Vec::new(); components.len()];
    for left in 0..components.len() {
        for right in (left + 1)..components.len() {
            if aabb_gap(component_bounds[left], component_bounds[right]) <= gap_threshold {
                component_neighbors[left].push(right);
                component_neighbors[right].push(left);
            }
        }
    }

    let mut visited = vec![false; components.len()];
    let mut merged = Vec::new();
    for start in 0..components.len() {
        if visited[start] {
            continue;
        }
        let mut queue = VecDeque::new();
        let mut merged_component = Vec::new();
        queue.push_back(start);
        visited[start] = true;

        while let Some(current) = queue.pop_front() {
            merged_component.extend(components[current].iter().copied());
            for &neighbor in &component_neighbors[current] {
                if !visited[neighbor] {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }

        merged_component.sort_unstable();
        merged_component.dedup();
        merged.push(merged_component);
    }

    merged
}

fn component_bounds_for_provinces(
    map: &shared::map::MapData,
    province_ids: &[u32],
) -> ([f32; 2], [f32; 2]) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for &province_id in province_ids {
        if let Some(province) = map.provinces.get(province_id as usize) {
            if let Some((aabb_min, aabb_max)) =
                bounds_from_points(&boundary_points(&province.boundary))
            {
                min_x = min_x.min(aabb_min[0]);
                max_x = max_x.max(aabb_max[0]);
                min_y = min_y.min(aabb_min[1]);
                max_y = max_y.max(aabb_max[1]);
            }
        }
    }
    if min_x == f32::MAX {
        ([0.0, 0.0], [0.0, 0.0])
    } else {
        ([min_x, min_y], [max_x, max_y])
    }
}

fn component_points_and_centroid(
    map: &shared::map::MapData,
    province_ids: &[u32],
) -> Option<(Vec<[f32; 2]>, [f32; 2])> {
    let mut points = Vec::new();
    let mut centroid_sum = [0.0_f32, 0.0_f32];
    let mut count = 0_u32;
    for &province_id in province_ids {
        let province = map.provinces.get(province_id as usize)?;
        points.extend(boundary_points(&province.boundary));
        centroid_sum[0] += province.centroid[0];
        centroid_sum[1] += province.centroid[1];
        count += 1;
    }
    if points.is_empty() || count == 0 {
        return None;
    }
    let divisor = count as f32;
    Some((
        points,
        [centroid_sum[0] / divisor, centroid_sum[1] / divisor],
    ))
}

fn visible_label_from_geometry(
    key: LabelKey,
    text: String,
    geometry: LabelGeometry,
    priority: LabelPriority,
    projection_scale: f32,
    window_size: [f32; 2],
    camera_x: f32,
    viewport: ([f32; 2], [f32; 2]),
) -> LabelCandidateOutcome {
    let Some(x_offset) =
        best_visible_x_offset(geometry.aabb_min, geometry.aabb_max, viewport, camera_x)
    else {
        return LabelCandidateOutcome::OffViewport;
    };
    let shifted_geometry = shift_geometry(&geometry, x_offset);
    let (bounds_width_units, bounds_height_units) = label_box_units(&shifted_geometry, &text);
    let base_font_world_size = fitted_font_world_size(&shifted_geometry, &text)
        .unwrap_or_else(|| fallback_font_world_size(&shifted_geometry));
    let font_world_size = base_font_world_size * label_fit_margin(priority);
    let font_pixels = font_world_size / projection_scale;
    if !label_within_viewport_budget(
        priority,
        font_pixels,
        bounds_width_units,
        bounds_height_units,
        window_size,
    ) {
        return LabelCandidateOutcome::OffViewport;
    }
    LabelCandidateOutcome::Visible(VisibleLabel {
        key,
        text,
        center: shifted_geometry.center,
        angle: label_angle(&shifted_geometry, priority),
        font_world_size,
        font_pixels,
        bounds_width_units,
        bounds_height_units,
        collision_padding_units: collision_padding_units(priority),
        priority,
    })
}

fn compute_label_geometry(
    points: &[[f32; 2]],
    centroid: [f32; 2],
    single_char: bool,
) -> Option<LabelGeometry> {
    if points.is_empty() {
        return None;
    }
    let (aabb_min, aabb_max) = bounds_from_points(points)?;
    let aabb_width = aabb_max[0] - aabb_min[0];
    let aabb_height = aabb_max[1] - aabb_min[1];
    let min_span = aabb_width.min(aabb_height).max(1e-3);

    let (axis_unit, mut angle) = principal_axis(points, centroid);
    angle = upright_angle(angle);
    let perp_unit = [-axis_unit[1], axis_unit[0]];

    let mut min_axis = f32::MAX;
    let mut max_axis = f32::MIN;
    let mut min_perp = f32::MAX;
    let mut max_perp = f32::MIN;
    for point in points {
        let rel_x = point[0] - centroid[0];
        let rel_y = point[1] - centroid[1];
        let axis_proj = rel_x * axis_unit[0] + rel_y * axis_unit[1];
        let perp_proj = rel_x * perp_unit[0] + rel_y * perp_unit[1];
        min_axis = min_axis.min(axis_proj);
        max_axis = max_axis.max(axis_proj);
        min_perp = min_perp.min(perp_proj);
        max_perp = max_perp.max(perp_proj);
    }
    let center = if single_char {
        centroid
    } else {
        [
            centroid[0]
                + axis_unit[0] * ((min_axis + max_axis) * 0.5)
                + perp_unit[0] * ((min_perp + max_perp) * 0.5),
            centroid[1]
                + axis_unit[1] * ((min_axis + max_axis) * 0.5)
                + perp_unit[1] * ((min_perp + max_perp) * 0.5),
        ]
    };

    Some(LabelGeometry {
        center,
        angle,
        axis_length: (max_axis - min_axis).max(1e-3),
        perp_span: (max_perp - min_perp).max(1e-3),
        min_span,
        aabb_min,
        aabb_max,
        single_char,
    })
}

fn fitted_font_world_size(geometry: &LabelGeometry, text: &str) -> Option<f32> {
    let (bounds_width_units, bounds_height_units) = label_box_units(geometry, text);
    let font_world_size = if geometry.single_char {
        geometry.min_span / bounds_height_units
    } else {
        let axis_fit = geometry.axis_length / bounds_width_units;
        let vertical_fit = geometry.perp_span / bounds_height_units;
        axis_fit.min(vertical_fit)
    };
    if font_world_size.is_finite() && font_world_size > 0.0 {
        Some(font_world_size)
    } else {
        None
    }
}

fn fallback_font_world_size(geometry: &LabelGeometry) -> f32 {
    geometry.min_span / label_height_units(geometry).max(1e-3)
}

fn label_box_units(geometry: &LabelGeometry, text: &str) -> (f32, f32) {
    let height_units = label_height_units(geometry);
    if geometry.single_char {
        (1.0, height_units)
    } else {
        let char_count = text.chars().count().max(1) as f32;
        (
            char_count * MULTI_CHAR_ADVANCE_UNITS + MULTI_CHAR_SIDE_PADDING_UNITS,
            height_units,
        )
    }
}

fn label_height_units(geometry: &LabelGeometry) -> f32 {
    let slenderness = geometry.axis_length / geometry.perp_span.max(1e-3);
    let thinness = ((slenderness - THIN_REGION_SLENDERNESS_START) / THIN_REGION_SLENDERNESS_RANGE)
        .clamp(0.0, 1.0);
    BASE_LABEL_HEIGHT_UNITS + THIN_REGION_EXTRA_HEIGHT_UNITS * thinness
}

fn label_within_viewport_budget(
    priority: LabelPriority,
    font_pixels: f32,
    bounds_width_units: f32,
    bounds_height_units: f32,
    window_size: [f32; 2],
) -> bool {
    if matches!(priority, LabelPriority::Province) {
        return true;
    }
    let label_width_pixels = font_pixels * bounds_width_units;
    let label_height_pixels = font_pixels * bounds_height_units;
    label_width_pixels <= window_size[0] * REGION_LABEL_MAX_VIEWPORT_WIDTH_FRACTION
        && label_height_pixels <= window_size[1] * REGION_LABEL_MAX_VIEWPORT_HEIGHT_FRACTION
}

fn label_angle(geometry: &LabelGeometry, priority: LabelPriority) -> f32 {
    match priority {
        LabelPriority::Province => geometry.angle,
        LabelPriority::Country | LabelPriority::Admin => {
            geometry.angle * region_axis_follow_factor(geometry.axis_length, geometry.perp_span)
        }
    }
}

fn region_axis_follow_factor(axis_length: f32, perp_span: f32) -> f32 {
    let slenderness = axis_length / perp_span.max(1e-3);
    if slenderness <= 1.0 {
        return 0.0;
    }
    if slenderness <= REGION_SQUAREISH_SLENDERNESS {
        let blend = (slenderness - 1.0) / (REGION_SQUAREISH_SLENDERNESS - 1.0);
        return REGION_SQUAREISH_TILT_FRACTION * blend;
    }
    if slenderness >= REGION_AXIS_FOLLOW_SLENDERNESS {
        return 1.0;
    }
    let blend = (slenderness - REGION_SQUAREISH_SLENDERNESS)
        / (REGION_AXIS_FOLLOW_SLENDERNESS - REGION_SQUAREISH_SLENDERNESS);
    REGION_SQUAREISH_TILT_FRACTION + (1.0 - REGION_SQUAREISH_TILT_FRACTION) * blend
}

fn label_fit_margin(priority: LabelPriority) -> f32 {
    match priority {
        LabelPriority::Province => PROVINCE_LABEL_FIT_MARGIN,
        LabelPriority::Country | LabelPriority::Admin => REGION_LABEL_FIT_MARGIN,
    }
}

fn collision_padding_units(priority: LabelPriority) -> f32 {
    match priority {
        LabelPriority::Province => PROVINCE_COLLISION_PADDING_UNITS,
        LabelPriority::Country | LabelPriority::Admin => REGION_COLLISION_PADDING_UNITS,
    }
}

fn principal_axis(points: &[[f32; 2]], centroid: [f32; 2]) -> ([f32; 2], f32) {
    let mut cov_xx = 0.0_f32;
    let mut cov_xy = 0.0_f32;
    let mut cov_yy = 0.0_f32;
    for point in points {
        let dx = point[0] - centroid[0];
        let dy = point[1] - centroid[1];
        cov_xx += dx * dx;
        cov_xy += dx * dy;
        cov_yy += dy * dy;
    }
    let angle = 0.5 * (2.0 * cov_xy).atan2(cov_xx - cov_yy);
    ([angle.cos(), angle.sin()], angle)
}

fn upright_angle(angle: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let half_pi = std::f32::consts::FRAC_PI_2;
    let mut adjusted = angle;
    if adjusted > half_pi {
        adjusted -= pi;
    } else if adjusted < -half_pi {
        adjusted += pi;
    }
    adjusted
}

fn boundary_points(boundary: &[Vec<[f32; 2]>]) -> Vec<[f32; 2]> {
    boundary
        .iter()
        .flat_map(|ring| ring.iter().copied())
        .collect()
}

fn bounds_from_points(points: &[[f32; 2]]) -> Option<([f32; 2], [f32; 2])> {
    let first = points.first().copied()?;
    let mut min_x = first[0];
    let mut max_x = first[0];
    let mut min_y = first[1];
    let mut max_y = first[1];
    for point in points.iter().skip(1) {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }
    Some(([min_x, min_y], [max_x, max_y]))
}

fn viewport_bounds(
    camera_translation: Vec3,
    projection_scale: f32,
    window: &Window,
) -> ([f32; 2], [f32; 2]) {
    let half_width = projection_scale * window.width() * 0.5;
    let half_height = projection_scale * window.height() * 0.5;
    (
        [
            camera_translation.x - half_width,
            camera_translation.y - half_height,
        ],
        [
            camera_translation.x + half_width,
            camera_translation.y + half_height,
        ],
    )
}

fn best_visible_x_offset(
    aabb_min: [f32; 2],
    aabb_max: [f32; 2],
    viewport: ([f32; 2], [f32; 2]),
    camera_x: f32,
) -> Option<f32> {
    let offsets = [0.0_f32, -MAP_WIDTH, MAP_WIDTH];
    let mut best = None;
    let mut best_distance = f32::MAX;
    for offset in offsets {
        let shifted_min = [aabb_min[0] + offset, aabb_min[1]];
        let shifted_max = [aabb_max[0] + offset, aabb_max[1]];
        if !aabb_intersects(shifted_min, shifted_max, viewport.0, viewport.1) {
            continue;
        }
        let center_x = (shifted_min[0] + shifted_max[0]) * 0.5;
        let distance = (center_x - camera_x).abs();
        if distance < best_distance {
            best_distance = distance;
            best = Some(offset);
        }
    }
    best
}

fn shift_geometry(geometry: &LabelGeometry, x_offset: f32) -> LabelGeometry {
    let mut shifted = geometry.clone();
    shifted.center[0] += x_offset;
    shifted.aabb_min[0] += x_offset;
    shifted.aabb_max[0] += x_offset;
    shifted
}

fn label_bounds(
    center: [f32; 2],
    width_units: f32,
    height_units: f32,
    font_world_size: f32,
    angle: f32,
) -> ([f32; 2], [f32; 2]) {
    let width = font_world_size * width_units;
    let height = font_world_size * height_units;
    rotated_bounds(center, width, height, angle)
}

fn collision_label_bounds(label: &VisibleLabel) -> ([f32; 2], [f32; 2]) {
    label_bounds(
        label.center,
        label.bounds_width_units + label.collision_padding_units * 2.0,
        label.bounds_height_units + label.collision_padding_units,
        label.font_world_size,
        label.angle,
    )
}

fn rotated_bounds(center: [f32; 2], width: f32, height: f32, angle: f32) -> ([f32; 2], [f32; 2]) {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let sin_angle = angle.sin();
    let cos_angle = angle.cos();
    let corners = [
        [-half_width, -half_height],
        [half_width, -half_height],
        [half_width, half_height],
        [-half_width, half_height],
    ];
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for corner in corners {
        let rotated_x = corner[0] * cos_angle - corner[1] * sin_angle + center[0];
        let rotated_y = corner[0] * sin_angle + corner[1] * cos_angle + center[1];
        min_x = min_x.min(rotated_x);
        max_x = max_x.max(rotated_x);
        min_y = min_y.min(rotated_y);
        max_y = max_y.max(rotated_y);
    }
    ([min_x, min_y], [max_x, max_y])
}

fn overlapping_label(
    bounds: ([f32; 2], [f32; 2]),
    accepted: &[AcceptedLabelBounds],
) -> Option<&AcceptedLabelBounds> {
    accepted
        .iter()
        .find(|existing| aabb_intersects(bounds.0, bounds.1, existing.bounds.0, existing.bounds.1))
}

fn aabb_intersects(
    left_min: [f32; 2],
    left_max: [f32; 2],
    right_min: [f32; 2],
    right_max: [f32; 2],
) -> bool {
    left_min[0] <= right_max[0]
        && left_max[0] >= right_min[0]
        && left_min[1] <= right_max[1]
        && left_max[1] >= right_min[1]
}

fn compare_visible_labels(left: &VisibleLabel, right: &VisibleLabel) -> std::cmp::Ordering {
    left.priority
        .cmp(&right.priority)
        .then_with(|| {
            right
                .font_pixels
                .partial_cmp(&left.font_pixels)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| left.text.cmp(&right.text))
}

fn aabb_gap(left: ([f32; 2], [f32; 2]), right: ([f32; 2], [f32; 2])) -> f32 {
    let gap_x = if left.1[0] < right.0[0] {
        right.0[0] - left.1[0]
    } else if right.1[0] < left.0[0] {
        left.0[0] - right.1[0]
    } else {
        0.0
    };
    let gap_y = if left.1[1] < right.0[1] {
        right.0[1] - left.1[1]
    } else if right.1[1] < left.0[1] {
        left.0[1] - right.1[1]
    } else {
        0.0
    };
    (gap_x * gap_x + gap_y * gap_y).sqrt()
}

fn label_z(priority: LabelPriority) -> f32 {
    match priority {
        LabelPriority::Province => 1.7,
        LabelPriority::Country => 1.65,
        LabelPriority::Admin => 1.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(axis_length: f32, perp_span: f32, single_char: bool) -> LabelGeometry {
        LabelGeometry {
            center: [0.0, 0.0],
            angle: 0.0,
            axis_length,
            perp_span,
            min_span: axis_length.min(perp_span),
            aabb_min: [-axis_length * 0.5, -perp_span * 0.5],
            aabb_max: [axis_length * 0.5, perp_span * 0.5],
            single_char,
        }
    }

    #[test]
    fn thin_regions_require_more_vertical_clearance() {
        let wide = geometry(8.0, 4.0, false);
        let thin = geometry(8.0, 1.0, false);

        assert!(label_height_units(&thin) > label_height_units(&wide));
    }

    #[test]
    fn longer_labels_keep_full_width_requirement() {
        let geometry = geometry(20.0, 5.0, false);
        let short = fitted_font_world_size(&geometry, "吴");
        let long = fitted_font_world_size(&geometry, "婆罗洲丛林");

        assert!(short.is_some());
        assert!(long.is_some());
        assert!(long.unwrap() < short.unwrap());
    }

    #[test]
    fn province_labels_use_more_collision_padding_than_region_labels() {
        assert!(
            collision_padding_units(LabelPriority::Province)
                > collision_padding_units(LabelPriority::Country)
        );
        assert!(
            collision_padding_units(LabelPriority::Province)
                > collision_padding_units(LabelPriority::Admin)
        );
    }

    #[test]
    fn province_labels_keep_a_larger_readability_margin() {
        assert!(
            label_fit_margin(LabelPriority::Province) < label_fit_margin(LabelPriority::Country)
        );
        assert!(label_fit_margin(LabelPriority::Province) < label_fit_margin(LabelPriority::Admin));
    }

    #[test]
    fn squareish_region_labels_stay_close_to_horizontal() {
        let factor = region_axis_follow_factor(1.5, 1.0);

        assert!(factor > 0.0);
        assert!(factor < 0.35);
    }

    #[test]
    fn elongated_region_labels_follow_their_axis() {
        let factor = region_axis_follow_factor(4.0, 1.0);

        assert_eq!(factor, 1.0);
    }

    #[test]
    fn oversized_region_labels_are_hidden() {
        assert!(!label_within_viewport_budget(
            LabelPriority::Country,
            48.0,
            10.0,
            1.4,
            [800.0, 600.0],
        ));
    }

    #[test]
    fn province_labels_ignore_viewport_budget() {
        assert!(label_within_viewport_budget(
            LabelPriority::Province,
            200.0,
            20.0,
            2.0,
            [800.0, 600.0],
        ));
    }
}
