// Province color lookup shader.
// UV stores raw texel coordinates (col, row) as floats.
// Texture pixel at (col, row) holds the RGBA8 color for the province at
// array-index pid = row * TEX_WIDTH + col.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(2) @binding(0) var color_texture: texture_2d<f32>;

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let col = u32(round(in.uv.x));
    let row = u32(round(in.uv.y));
    return textureLoad(color_texture, vec2<u32>(col, row), 0);
}
