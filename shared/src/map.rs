use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

/// All map geometry data, loaded from assets/map.bin.
/// Indices into `provinces` are province IDs used in GameState.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub provinces: Vec<MapProvince>,
}

/// Geometry + metadata for one province polygon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapProvince {
    /// Sequential province ID (index in MapData.provinces).
    pub id: u32,
    /// GADM identifier, e.g. "DEU.2.3.1_1".
    pub gadm_id: String,
    /// Human-readable name from GADM (e.g. "Kreuzberg").
    pub name: String,
    /// ISO 3166-1 alpha-3 country code (e.g. "DEU").
    pub country_code: String,
    /// Simplified polygon boundary rings.
    /// First ring is the outer boundary; subsequent rings are holes.
    /// Each ring is a list of [lon, lat] points.
    pub boundary: Vec<Vec<[f32; 2]>>,
    /// Pre-triangulated vertices (flat [x, y] in Mercator-projected coords).
    pub vertices: Vec<[f32; 2]>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u32>,
    /// Centroid [x, y] for label placement.
    pub centroid: [f32; 2],
}

impl MapData {
    /// Load from a bincode file.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        bincode::deserialize(&bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save to a bincode file.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes = bincode::serialize(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, bytes)
    }
}
