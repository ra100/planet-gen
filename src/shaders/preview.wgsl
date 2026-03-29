// Preview renderer: draws a lit sphere with procedural terrain.
// Computes fBm noise directly in the fragment shader — no cubemap,
// no face seams. Terrain params passed as uniforms.

struct Uniforms {
    rotation: mat4x4<f32>,
    light_dir: vec3<f32>,
    ocean_level: f32,
    // Terrain params
    seed_offset: vec3<f32>,
    frequency: f32,
    lacunarity: f32,
    gain: f32,
    amplitude: f32,
    octaves: u32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
}

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(idx) / 2) * 4.0 - 1.0;
    let y = f32(i32(idx) % 2) * 4.0 - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>(x, -y) * 0.5 + 0.5;
    return out;
}

// Ray-sphere intersection — sphere fills ~85% of viewport
fn intersect_sphere(uv: vec2<f32>) -> vec3<f32> {
    // Map UV so sphere fills viewport with small margin
    let ndc = (uv - 0.5) * 2.0 / 0.85;
    // Simple orthographic-like projection for clean sphere
    let r2 = dot(ndc, ndc);
    if (r2 > 1.0) {
        return vec3<f32>(0.0, 0.0, 0.0); // miss
    }
    let z = sqrt(1.0 - r2);
    return vec3<f32>(ndc.x, ndc.y, z);
}

fn fbm_preview(pos: vec3<f32>) -> f32 {
    var value = 0.0;
    var freq = uniforms.frequency;
    var amp = uniforms.amplitude;
    let p = pos + uniforms.seed_offset;

    for (var i = 0u; i < uniforms.octaves; i++) {
        value += amp * snoise(p * freq);
        freq *= uniforms.lacunarity;
        amp *= uniforms.gain;
    }

    return value;
}

fn height_to_color(h: f32, sea_level: f32) -> vec3<f32> {
    if (h < sea_level) {
        let depth = (sea_level - h) / max(sea_level + 1.0, 0.5);
        let deep = vec3<f32>(0.02, 0.05, 0.25);
        let shallow = vec3<f32>(0.08, 0.2, 0.55);
        return mix(shallow, deep, clamp(depth, 0.0, 1.0));
    }

    let max_land = 1.0 - sea_level;
    let t = clamp((h - sea_level) / max(max_land, 0.01), 0.0, 1.0);

    if (t < 0.05) {
        return mix(vec3<f32>(0.76, 0.7, 0.5), vec3<f32>(0.2, 0.5, 0.1), t / 0.05);
    } else if (t < 0.3) {
        return mix(vec3<f32>(0.2, 0.5, 0.1), vec3<f32>(0.3, 0.55, 0.15), (t - 0.05) / 0.25);
    } else if (t < 0.55) {
        return mix(vec3<f32>(0.4, 0.35, 0.15), vec3<f32>(0.5, 0.4, 0.25), (t - 0.3) / 0.25);
    } else if (t < 0.8) {
        return mix(vec3<f32>(0.5, 0.48, 0.45), vec3<f32>(0.7, 0.68, 0.65), (t - 0.55) / 0.25);
    } else {
        return mix(vec3<f32>(0.8, 0.8, 0.82), vec3<f32>(0.95, 0.95, 0.98), (t - 0.8) / 0.2);
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hit = intersect_sphere(in.uv);

    if (length(hit) < 0.01) {
        return vec4<f32>(0.02, 0.02, 0.05, 1.0);
    }

    let normal = normalize(hit);

    // Rotate the sampling direction (not the sphere)
    let rotated = (uniforms.rotation * vec4<f32>(normal, 0.0)).xyz;

    // Compute terrain height directly from 3D noise — no cubemap, no seams
    let raw_height = fbm_preview(rotated);

    // Normalize to approximately [-1, 1] (fBm with amplitude 1.0 and gain 0.5
    // has theoretical max sum of geometric series = amp / (1 - gain) = 2.0)
    let height = clamp(raw_height / (uniforms.amplitude * 1.5), -1.0, 1.0);

    let base_color = height_to_color(height, uniforms.ocean_level);

    // Diffuse + ambient lighting
    let light = normalize(uniforms.light_dir);
    let ndotl = max(dot(normal, light), 0.0);
    let ambient = 0.15;
    let lit_color = base_color * (ambient + (1.0 - ambient) * ndotl);

    return vec4<f32>(lit_color, 1.0);
}
