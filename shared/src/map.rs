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
    /// EU5 province tag (e.g. "stockholm", "hangzhou").
    pub tag: String,
    /// Human-readable name (same as tag).
    pub name: String,
    /// Terrain topography (e.g. "flatland", "hills", "mountains").
    pub topography: String,
    /// Vegetation type (e.g. "farmland", "forest", "desert").
    pub vegetation: String,
    /// Climate type (e.g. "continental", "tropical", "arid").
    pub climate: String,
    /// Primary raw material (e.g. "wheat", "iron", "silk").
    pub raw_material: String,
    /// Natural harbor suitability (0.0–1.0).
    pub harbor_suitability: f32,
    /// EU5-designed hex color for this province (#RRGGBB), as linear RGBA.
    /// Unique, stable, contrasting-with-neighbors. Used for political map mode.
    pub hex_color: [f32; 4],
    /// Sea zone this province's port connects to (from ports.gpkg), if any.
    pub port_sea_zone: Option<String>,
    /// Simplified polygon boundary rings.
    /// First ring is the outer boundary; subsequent rings are holes.
    /// Each ring is a list of [lon, lat] points.
    pub boundary: Vec<Vec<[f32; 2]>>,
    /// Pre-triangulated vertices (flat [x, y] in projected coords).
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
        bincode::deserialize(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Save to a bincode file.
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes =
            bincode::serialize(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, bytes)
    }
}

// ── Terrain geometry ──────────────────────────────────────────────────────────

/// Pre-triangulated non-playable terrain polygon (ocean, sea, lake, wasteland).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainPolygon {
    /// Pre-assigned RGBA color for this topography type.
    pub color: [f32; 4],
    /// Pre-triangulated vertices [lon, lat] in equirectangular coords.
    pub vertices: Vec<[f32; 2]>,
    /// Triangle indices into `vertices`.
    pub indices: Vec<u32>,
}

/// All terrain geometry, loaded from assets/terrain.bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainData {
    pub polygons: Vec<TerrainPolygon>,
}

impl TerrainData {
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        bincode::deserialize(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes =
            bincode::serialize(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, bytes)
    }
}

// ── Province adjacency (pre-computed border chains) ────────────────────────────

pub const ADJACENCY_CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedBorder {
    pub provinces: [u32; 2],
    pub chains: Vec<Vec<[f32; 2]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvinceAdjacencyCache {
    pub version: u32,
    pub province_count: u32,
    pub borders: Vec<CachedBorder>,
}

impl ProvinceAdjacencyCache {
    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes = bincode::serialize(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, bytes)
    }

    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        bincode::deserialize(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

// ── River geometry ────────────────────────────────────────────────────────────

/// A river graph node shared by one or more river edges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverNode {
    /// Node position in WGS84 degrees.
    pub position: [f32; 2],
}

/// A single river edge as an ordered sequence of [lon, lat] control points.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverEdge {
    /// Ordered [lon, lat] points in WGS84 degrees.
    pub points: Vec<[f32; 2]>,
    /// Approximate width class (0 = small tributary, 1 = medium, 2 = large river).
    pub width_class: u8,
    /// Start node id in `RiverData.nodes`.
    pub start_node: u32,
    /// End node id in `RiverData.nodes`.
    pub end_node: u32,
}

/// All river geometry, loaded from assets/rivers.bin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverData {
    pub nodes: Vec<RiverNode>,
    pub edges: Vec<RiverEdge>,
}

impl RiverData {
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        bincode::deserialize(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let bytes =
            bincode::serialize(self).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, bytes)
    }
}
