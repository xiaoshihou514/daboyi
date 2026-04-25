//! Custom Material2d for the province map.
//!
//! Uses a 2-D lookup texture (Rgba8Unorm, nearest filter) where each texel
//! holds the RGBA colour for one province.  The mesh's UV_0 attribute stores
//! raw texel coordinates `[col, row]` so the fragment shader can do a direct
//! `textureLoad` — no sampler, no normalisation.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};
use bevy::sprite::{AlphaMode2d, Material2d};

#[allow(dead_code)] // Used in shader code
#[derive(Clone, Copy, ShaderType)]
pub struct ProvinceMapParams {
    pub terrain_focus: f32,
    pub _padding: Vec3,
}

impl Default for ProvinceMapParams {
    fn default() -> Self {
        Self {
            terrain_focus: 0.0,
            _padding: Vec3::ZERO,
        }
    }
}

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct ProvinceMapMaterial {
    #[texture(0)]
    pub political_texture: Handle<Image>,
    #[texture(1)]
    pub terrain_texture: Handle<Image>,
    #[uniform(2)]
    pub params: ProvinceMapParams,
}

impl Material2d for ProvinceMapMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/province_map.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}
