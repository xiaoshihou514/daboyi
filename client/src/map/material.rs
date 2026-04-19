//! Custom Material2d for the province map.
//!
//! Uses a 2-D lookup texture (Rgba8Unorm, nearest filter) where each texel
//! holds the RGBA colour for one province.  The mesh's UV_0 attribute stores
//! raw texel coordinates `[col, row]` so the fragment shader can do a direct
//! `textureLoad` — no sampler, no normalisation.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef};
use bevy::sprite::Material2d;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct ProvinceMapMaterial {
    #[texture(0)]
    pub color_texture: Handle<Image>,
}

impl Material2d for ProvinceMapMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/province_map.wgsl".into()
    }
}
