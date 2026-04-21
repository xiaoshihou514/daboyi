#import bevy_sprite::{
    mesh2d_functions as mesh_functions,
    mesh2d_view_bindings::view,
}

struct BorderMaterial {
    proj_scale: f32,
    _padding: vec3<f32>,
};

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) offset: vec2<f32>,
    @location(2) tier: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) tier: f32,
};

@group(2) @binding(0) var<uniform> material: BorderMaterial;

const COUNTRY_BORDER_COLOR: vec4<f32> = vec4<f32>(0.02, 0.02, 0.02, 0.95);
const ADMIN_BORDER_COLOR: vec4<f32> = vec4<f32>(0.02, 0.02, 0.02, 0.55);
const PROVINCE_BORDER_COLOR: vec4<f32> = vec4<f32>(0.04, 0.04, 0.04, 0.22);

fn zoom_factor(proj_scale: f32, zoomed_out_scale: f32, zoomed_in_scale: f32) -> f32 {
    return clamp(
        (zoomed_out_scale - proj_scale) / (zoomed_out_scale - zoomed_in_scale),
        0.0,
        1.0,
    );
}

fn smoothstep_unit(x: f32) -> f32 {
    let t = clamp(x, 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn lerp(start: f32, end: f32, t: f32) -> f32 {
    return start + (end - start) * t;
}

fn border_half_width_world(tier: f32, proj_scale: f32) -> f32 {
    let zoom_in = smoothstep_unit(zoom_factor(proj_scale, 0.07, 0.004));
    if tier < 0.5 {
        return lerp(0.6, 2.4, zoom_in) * proj_scale;
    }
    if tier < 1.5 {
        return lerp(0.35, 1.4, zoom_in) * proj_scale;
    }
    let reveal = smoothstep_unit(zoom_factor(proj_scale, 0.035, 0.008));
    return lerp(0.0, 0.8, reveal) * proj_scale;
}

fn border_color(tier: f32, proj_scale: f32) -> vec4<f32> {
    let zoom_in = smoothstep_unit(zoom_factor(proj_scale, 0.07, 0.004));
    if tier < 0.5 {
        return COUNTRY_BORDER_COLOR;
    }
    if tier < 1.5 {
        return vec4<f32>(
            ADMIN_BORDER_COLOR.rgb,
            lerp(0.35, ADMIN_BORDER_COLOR.a, zoom_in),
        );
    }
    let reveal = smoothstep_unit(zoom_factor(proj_scale, 0.035, 0.008));
    return vec4<f32>(
        PROVINCE_BORDER_COLOR.rgb,
        PROVINCE_BORDER_COLOR.a * reveal,
    );
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let expanded_local = vec4<f32>(
        vertex.position.xy + vertex.offset * border_half_width_world(vertex.tier, material.proj_scale),
        vertex.position.z,
        1.0,
    );
    let world_position = mesh_functions::mesh2d_position_local_to_world(
        world_from_local,
        expanded_local,
    );
    out.position = mesh_functions::mesh2d_position_world_to_clip(world_position);
    out.tier = vertex.tier;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = border_color(in.tier, material.proj_scale);
    return color;
}
