use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod map;

/// A country in the map editor: has a display name, short tag, RGBA color, and optional capital.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorCountry {
    /// Short unique identifier (e.g. "CHN", "ABC"). Used as key in assignments.
    pub tag: String,
    /// Display name shown in the UI and on the map.
    pub name: String,
    /// RGBA color in [0,1] range.
    pub color: [f32; 4],
    /// Province ID of the capital (for star/label rendering), if any.
    pub capital_province: Option<u32>,
}

/// One node in the administrative-area tree (ADM1, ADM2, …).
/// The tree is stored flat; `parent_id` encodes hierarchy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminArea {
    /// Stable numeric ID assigned when created.
    pub id: u32,
    /// Display name.
    pub name: String,
    /// The country this area belongs to.
    pub country_tag: String,
    /// Parent area id. `None` = top-level ADM1 of the country.
    pub parent_id: Option<u32>,
    /// Explicit color override; inherits from parent/country chain if `None`.
    pub color: Option<[f32; 4]>,
}

/// The on-disk save format for a coloring file (JSON).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColoringFile {
    pub countries: Vec<EditorCountry>,
    /// province_id → country_tag
    pub assignments: HashMap<u32, String>,
    #[serde(default)]
    pub admin_areas: Vec<AdminArea>,
    /// province_id → admin_area_id
    #[serde(default)]
    pub admin_assignments: HashMap<u32, u32>,
}
