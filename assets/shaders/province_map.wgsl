// Province color lookup shader.
// UV stores raw texel coordinates (col, row) as floats.
// Texture pixel at (col, row) holds the RGBA8 color for the province at
// array-index pid = row * TEX_WIDTH + col.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct ProvinceMapParams {
    terrain_focus: f32,
    _padding: vec3<f32>,
}

@group(2) @binding(0) var political_texture: texture_2d<f32>;
@group(2) @binding(1) var terrain_texture: texture_2d<f32>;
@group(2) @binding(2) var<uniform> map_params: ProvinceMapParams;

fn political_alpha(x: f32) -> f32 {
    let clamped = clamp(x, 0.0, 1.0);
    return 0.1 + 0.9 / (1.0 + exp(4.0 * (clamped - 0.5)));
}

fn political_visibility(political: vec4<f32>) -> f32 {
    let neutral = vec3<f32>(0.55, 0.55, 0.55);
    let chroma = length(political.rgb - neutral);
    return clamp(chroma * 4.0, 0.0, 1.0);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let col = u32(round(in.uv.x));
    let row = u32(round(in.uv.y));
    let political = textureLoad(political_texture, vec2<u32>(col, row), 0);
    let terrain = textureLoad(terrain_texture, vec2<u32>(col, row), 0);
    let alpha =
        political_alpha(map_params.terrain_focus) * political_visibility(political) * political.a;
    return vec4<f32>(mix(terrain.rgb, political.rgb, alpha), political.a);
}
