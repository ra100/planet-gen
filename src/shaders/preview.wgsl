// Preview renderer: draws a lit sphere colored by heightmap data.
// Uses a fullscreen quad with raymarching against a unit sphere.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    ocean_level: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var height_tex: texture_cube<f32>;
@group(0) @binding(2) var height_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Fullscreen triangle (3 vertices, no vertex buffer needed)
@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(idx) / 2) * 4.0 - 1.0;
    let y = f32(i32(idx) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

// Ray-sphere intersection — camera at z=-2.2 for a larger planet in viewport
fn intersect_sphere(uv: vec2<f32>) -> vec3<f32> {
    let ro = vec3<f32>(0.0, 0.0, -2.2);
    let rd = normalize(vec3<f32>((uv - 0.5) * 2.0, 1.5));

    let b = dot(ro, rd);
    let c = dot(ro, ro) - 1.0;
    let disc = b * b - c;

    if (disc < 0.0) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    let t = -b - sqrt(disc);
    if (t < 0.0) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }

    return ro + t * rd;
}

fn height_to_color(h: f32, sea_level: f32) -> vec3<f32> {
    // Height relative to sea level
    let land_h = h - sea_level;

    if (h < sea_level) {
        // Ocean: deeper = darker blue
        let depth = (sea_level - h) / max(sea_level + 1.0, 0.5);
        let deep = vec3<f32>(0.02, 0.05, 0.25);
        let shallow = vec3<f32>(0.08, 0.2, 0.55);
        return mix(shallow, deep, clamp(depth, 0.0, 1.0));
    }

    // Land height normalized to [0, 1] above sea level
    let max_land = 1.0 - sea_level;
    let t = clamp(land_h / max(max_land, 0.01), 0.0, 1.0);

    if (t < 0.05) {
        // Beach/coast
        return mix(vec3<f32>(0.76, 0.7, 0.5), vec3<f32>(0.2, 0.5, 0.1), t / 0.05);
    } else if (t < 0.3) {
        // Lowland green
        return mix(vec3<f32>(0.2, 0.5, 0.1), vec3<f32>(0.3, 0.55, 0.15), (t - 0.05) / 0.25);
    } else if (t < 0.55) {
        // Highland brown
        return mix(vec3<f32>(0.4, 0.35, 0.15), vec3<f32>(0.5, 0.4, 0.25), (t - 0.3) / 0.25);
    } else if (t < 0.8) {
        // Mountain gray
        return mix(vec3<f32>(0.5, 0.48, 0.45), vec3<f32>(0.7, 0.68, 0.65), (t - 0.55) / 0.25);
    } else {
        // Snow
        return mix(vec3<f32>(0.8, 0.8, 0.82), vec3<f32>(0.95, 0.95, 0.98), (t - 0.8) / 0.2);
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hit = intersect_sphere(in.uv);

    if (length(hit) < 0.01) {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0); // background
    }

    let normal = normalize(hit);
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Sample height from cubemap with linear filtering (normalized to [-1, 1])
    let height = textureSample(height_tex, height_sampler, rotated).r;

    // Color from height with ocean level
    let base_color = height_to_color(height, uniforms.ocean_level);

    // Diffuse lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let ambient = 0.15;
    let lit_color = base_color * (ambient + (1.0 - ambient) * ndotl);

    return vec4<f32>(lit_color, 1.0);
}
