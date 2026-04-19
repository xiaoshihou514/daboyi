/// Color functions for province map rendering.
use shared::map::MapProvince;

/// Deterministic RGBA color for a country owner tag.
/// Uses FNV-1a hash — stable across runs unlike DefaultHasher.
pub fn owner_color_rgba(tag: &str) -> [f32; 4] {
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;
    let mut h = FNV_OFFSET;
    for byte in tag.bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(FNV_PRIME);
    }
    // Map hash bytes to visually distinct mid-range colors (avoid very dark/light).
    let r = f32::from(u8::try_from((h >> 0) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    let g = f32::from(u8::try_from((h >> 8) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    let b = f32::from(u8::try_from((h >> 16) & 0xFF).unwrap()) / 255.0 * 0.55 + 0.20;
    [r, g, b, 1.0]
}

pub fn brighten(c: [f32; 4]) -> [f32; 4] {
    [
        (c[0] + 0.25).min(1.0),
        (c[1] + 0.25).min(1.0),
        (c[2] + 0.25).min(1.0),
        c[3],
    ]
}

pub fn dim(c: [f32; 4], factor: f32) -> [f32; 4] {
    [
        c[0] * factor,
        c[1] * factor,
        c[2] * factor,
        c[3],
    ]
}

/// Terrain mode: color by province topography.
pub fn terrain_province_color(topography: &str) -> [f32; 4] {
    match topography {
        "mountains" => [0.420, 0.357, 0.306, 1.0],  // dark muted brown
        "hills" => [0.608, 0.545, 0.447, 1.0],        // tan
        "plateau" => [0.690, 0.627, 0.502, 1.0],      // light tan
        "wetlands" => [0.420, 0.545, 0.420, 1.0],     // muted teal-green
        "desert" | "sparse_desert" | "dunes" => [0.784, 0.659, 0.431, 1.0], // sandy
        "flatland" | "farmland" => [0.545, 0.667, 0.482, 1.0],              // green
        "ocean_wasteland" => [0.039, 0.165, 0.416, 1.0], // ocean color
        "dune_wasteland" => [0.788, 0.659, 0.431, 1.0],  // lighter sand
        "mesa_wasteland" => [0.608, 0.420, 0.278, 1.0],  // reddish
        "mountain_wasteland" => [0.369, 0.286, 0.224, 1.0], // dark brown
        _ if topography.contains("wasteland") => [0.545, 0.482, 0.420, 1.0], // grayish brown
        _ => [0.604, 0.604, 0.545, 1.0],
    }
}

/// Point-in-polygon test (ray casting algorithm).
pub fn point_in_polygon(px: f32, py: f32, ring: &[[f32; 2]]) -> bool {
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

pub fn point_in_province(px: f32, py: f32, mp: &MapProvince) -> bool {
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
