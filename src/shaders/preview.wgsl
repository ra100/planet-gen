// Preview renderer: draws a lit sphere colored by heightmap data.
// Uses a fullscreen quad with raymarching against a unit sphere.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    _pad: f32,
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
    // Generate fullscreen triangle
    let x = f32(i32(idx) / 2) * 4.0 - 1.0;
    let y = f32(i32(idx) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

// Ray-sphere intersection: ray origin at (0,0,-3), direction toward pixel
fn intersect_sphere(uv: vec2<f32>) -> vec3<f32> {
    // Camera at z=-3, looking at origin
    let ro = vec3<f32>(0.0, 0.0, -3.0);
    let rd = normalize(vec3<f32>((uv - 0.5) * 2.0, 1.0));

    // Sphere at origin, radius 1
    let b = dot(ro, rd);
    let c = dot(ro, ro) - 1.0;
    let disc = b * b - c;

    if (disc < 0.0) {
        return vec3<f32>(0.0, 0.0, 0.0); // miss
    }

    let t = -b - sqrt(disc);
    if (t < 0.0) {
        return vec3<f32>(0.0, 0.0, 0.0); // behind camera
    }

    return ro + t * rd;
}

fn height_to_color(h: f32) -> vec3<f32> {
    // Simple height-based coloring:
    // deep ocean (< -0.2): dark blue
    // shallow ocean (-0.2 to 0): blue
    // lowland (0 to 0.2): green
    // highland (0.2 to 0.5): brown
    // mountain (0.5 to 0.8): gray
    // snow (> 0.8): white
    if (h < -0.2) {
        return vec3<f32>(0.05, 0.1, 0.4);
    } else if (h < 0.0) {
        return mix(vec3<f32>(0.05, 0.1, 0.4), vec3<f32>(0.1, 0.3, 0.6), (h + 0.2) / 0.2);
    } else if (h < 0.2) {
        return mix(vec3<f32>(0.2, 0.5, 0.1), vec3<f32>(0.3, 0.6, 0.15), h / 0.2);
    } else if (h < 0.5) {
        return mix(vec3<f32>(0.4, 0.35, 0.15), vec3<f32>(0.5, 0.4, 0.3), (h - 0.2) / 0.3);
    } else if (h < 0.8) {
        return mix(vec3<f32>(0.5, 0.5, 0.5), vec3<f32>(0.7, 0.7, 0.7), (h - 0.5) / 0.3);
    } else {
        return vec3<f32>(0.95, 0.95, 0.98);
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hit = intersect_sphere(in.uv);

    // Check if ray missed the sphere
    if (length(hit) < 0.01) {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0); // background
    }

    let normal = normalize(hit);

    // Apply rotation to get the sampling direction
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Sample height from cubemap (level 0, non-filterable R32Float)
    let height = textureSampleLevel(height_tex, height_sampler, rotated, 0.0).r;

    // Color from height
    let base_color = height_to_color(height);

    // Simple diffuse lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let ambient = 0.15;
    let lit_color = base_color * (ambient + (1.0 - ambient) * ndotl);

    return vec4<f32>(lit_color, 1.0);
}
