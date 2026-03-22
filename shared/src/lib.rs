use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod conv;
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

/// The on-disk save format for a coloring file (JSON).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColoringFile {
    pub countries: Vec<EditorCountry>,
    /// province_id → country_tag
    pub assignments: HashMap<u32, String>,
}
