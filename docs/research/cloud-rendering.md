# Cloud Rendering and Atmosphere Deep-Dive

_Consolidated from cloud-layer-rendering.md and atmosphere-ocean-cloud-rendering.md_
_Research date: 2026-03-28 / Consolidated: 2026-04-02_

---

## Executive Summary

This document consolidates implementation-ready cloud rendering and atmosphere techniques for
Planet Gen's procedural planet pipeline. It covers noise generation (domain-warped fBm as a
Worley substitute), climate-driven coverage modulation that avoids latitude banding, the
CDF-compensated coverage slider, Beer-Lambert transmittance and Henyey-Greenstein phase
functions for lighting, self-shadowing, temporal reprojection for performance, cloud shadow
casting, and a phased implementation strategy from 2D shell preview to full volumetric. It
also includes Hillaire's production-ready atmosphere pipeline details and space-vs-ground
camera handling -- topics only summarized in final.md.

Content already in final.md (cloud types table, Bruneton precompute overview, basic
Rayleigh/Mie equations, planet-type scattering parameters) is not repeated here.

---

## 1. Cloud Density Map Generation

### 1.1 Domain-Warped fBm as Worley Substitute (Quilez Technique)

Pure fBm does not look like clouds -- it lacks the billowy rounded tops and sharp-edged voids
between cloud masses. The industry standard (Horizon Zero Dawn, Frostbite) uses Perlin-Worley
hybrids, but when only simplex noise is available (our case), **domain-warped fBm** achieves
similar organic, swirling shapes.

The technique feeds noise into itself (Inigo Quilez's domain warping), breaking the uniform
blobby character of plain fBm:

```wgsl
// Domain-warped fBm as Worley substitute
fn cloud_noise_warped(p: vec3<f32>) -> f32 {
    let warp = vec3<f32>(
        snoise(p + vec3<f32>(0.0, 0.0, 0.0)),
        snoise(p + vec3<f32>(5.2, 1.3, 0.0)),
        snoise(p + vec3<f32>(0.0, 0.0, 0.0))  // 2D: z can be constant
    );
    return cloud_fbm(p + warp * 0.7);
}
```

For comparison, the Perlin-Worley base used by Guerrilla Games:

```wgsl
// Perlin-Worley base: rounded cloud masses with clear gaps
fn cloud_base_noise(p: vec3<f32>) -> f32 {
    let perlin = snoise(p);
    let worley = 1.0 - worley_f1(p);  // invert: blobs instead of cells
    return remap(perlin, worley * 0.625 - 1.0, 1.0, 0.0, 1.0);
}
```

### 1.2 Noise Parameters for Space-View Clouds

From analysis of Earth cloud imagery and existing implementations:

| Parameter      | Value      | Rationale                                            |
| -------------- | ---------- | ---------------------------------------------------- |
| Base frequency | 4.0 - 6.0  | Gives ~6-10 major cloud systems visible from space   |
| Octaves        | 4 - 6      | More than terrain (for wispy edges) but fewer than 8 |
| Lacunarity     | 2.0 - 2.2  | Standard doubling; 2.2 reduces alignment artifacts   |
| Gain           | 0.5 - 0.55 | Standard; higher gain = more wispy detail            |
| Domain warp    | 0.5 - 0.8  | Strength of warp displacement; too high = smeared    |

**Octave count matters**: 4 octaves gives chunky masses, 6 gives wispy edges. For space view,
5 is a good default. Each octave costs one snoise call.

### 1.3 Thresholding: Quilez vs Schneider Remap

Raw noise values (roughly [-1, 1]) must be converted to cloud density. Naive
`max(noise - threshold, 0.0)` creates hard edges. Two solutions:

**Quilez remap** (from "Dynamic 2D Clouds"):

```wgsl
fn cloud_density_from_noise(noise_val: f32, threshold: f32) -> f32 {
    let sharpness = 1.0 / (1.0 - threshold);  // auto-scale
    return clamp((noise_val - threshold) * sharpness, 0.0, 1.0);
}
```

**Schneider remap** (Horizon Zero Dawn) -- preferred because it naturally produces lighter
small clouds and denser large clouds:

```wgsl
fn remap(value: f32, old_min: f32, old_max: f32, new_min: f32, new_max: f32) -> f32 {
    return new_min + (clamp(value, old_min, old_max) - old_min)
           / (old_max - old_min) * (new_max - new_min);
}

// Usage: erode edges while preserving dense core
fn apply_coverage(base_cloud: f32, coverage: f32) -> f32 {
    return remap(base_cloud, 1.0 - coverage, 1.0, 0.0, 1.0) * coverage;
}
```

When coverage is low, the remap squeezes the density range, making isolated clouds thinner.
When coverage is high, most noise passes through, making large cloud masses opaque.

---

## 2. Climate Modulation: Breaking Latitude Bands

### 2.1 The Core Problem

Multiplying cloud noise by moisture (`cloud = noise * moisture`) creates visible latitude
bands because Hadley cell moisture has strong latitudinal structure: ITCZ at equator, dry
subtropics at ~25-30 degrees, polar front at ~60 degrees.

### 2.2 Noise Drives Threshold, Not Amplitude

The key Schneider/HZD insight: **climate data controls the coverage threshold, not the
amplitude**.

Instead of: `density = noise * moisture` (creates bands)

Do: `density = remap(noise, 1.0 - moisture_coverage, 1.0, 0.0, 1.0)`

- High-moisture (ITCZ): low threshold, most noise values produce clouds
- Dry (subtropical): high threshold, only strongest noise peaks form clouds
- Cloud **shape** is still determined by noise, not latitude

### 2.3 Band-Breaking Techniques

**A. Domain warping the coverage map itself:**

Apply noise displacement to the moisture lookup position, so climate zones become wavy:

```wgsl
let climate_warp = vec3<f32>(
    snoise(sphere_pos * 2.0 + vec3<f32>(200.0, 0.0, 0.0)),
    snoise(sphere_pos * 2.0 + vec3<f32>(0.0, 300.0, 0.0)),
    snoise(sphere_pos * 2.0 + vec3<f32>(0.0, 0.0, 400.0))
) * 0.15;
let warped_pos = normalize(sphere_pos + climate_warp);
let moisture_for_clouds = compute_moisture(warped_pos, height);
```

**B. Curl noise for flow-like patterns (Wedekind):**

Compute the gradient of a noise potential on the sphere surface, rotate 90 degrees for
divergence-free flow vectors. This creates swirling cyclone-like patterns that naturally
break latitude bands:

```wgsl
fn curl_displacement(p: vec3<f32>, strength: f32) -> vec3<f32> {
    let eps = 0.01;
    let n = normalize(p);
    let dx = snoise(p + vec3<f32>(eps, 0.0, 0.0)) - snoise(p - vec3<f32>(eps, 0.0, 0.0));
    let dy = snoise(p + vec3<f32>(0.0, eps, 0.0)) - snoise(p - vec3<f32>(0.0, eps, 0.0));
    let dz = snoise(p + vec3<f32>(0.0, 0.0, eps)) - snoise(p - vec3<f32>(0.0, 0.0, eps));
    let grad = vec3<f32>(dx, dy, dz) / (2.0 * eps);
    let curl = cross(n, grad);
    return curl * strength;
}
```

**C. Latitude power softening:**

```wgsl
// Soften latitude influence: 70% noise, 30% climate
let climate_factor = 0.3 + 0.7 * moisture_normalized;
// Or equivalently:
let cloud_coverage = mix(global_coverage, moisture_normalized, 0.3);
```

### 2.4 Recommended Blend Strategy

Combine domain warping (A) with threshold modulation (2.2):

1. Compute cloud-specific coverage from moisture, with domain warping applied
2. Normalize moisture to [0, 1] and blend with global coverage parameter
3. Apply Schneider remap: `remap(noise, 1.0 - coverage, 1.0, 0.0, 1.0)`

```wgsl
fn cloud_coverage_at(sphere_pos: vec3<f32>, height: f32, global_coverage: f32) -> f32 {
    let warp = vec3<f32>(
        snoise(sphere_pos * 2.5 + vec3<f32>(200.0, 0.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 300.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 0.0, 400.0))
    ) * 0.12;
    let warped_pos = normalize(sphere_pos + warp);

    let moisture = compute_moisture(warped_pos, height);
    let moisture_norm = clamp(moisture / 300.0, 0.0, 1.0);

    // Climate nudges but doesn't dominate
    // At global_coverage=0.5: dry regions get ~0.35, wet regions get ~0.65
    return mix(global_coverage, moisture_norm, 0.35);
}
```

---

## 3. CDF-Compensated Coverage Slider

### 3.1 The Cliff Problem

A naive coverage slider that directly thresholds noise has non-linear visual response:

- 0.0 to 0.3: almost no visible clouds (noise rarely exceeds high threshold)
- 0.3 to 0.5: rapid explosion of coverage
- 0.5 to 0.7: most of the planet covered
- 0.7 to 1.0: barely any change

This happens because simplex noise values follow an approximately Gaussian distribution. The
CDF of a Gaussian is an S-curve, so linear threshold changes produce S-curve area changes.

### 3.2 CDF-Compensated Remapping

Map linear coverage [0,1] to a noise threshold that produces approximately that visual
coverage fraction:

```wgsl
fn coverage_to_threshold(coverage: f32) -> f32 {
    // Approximate inverse Gaussian CDF for simplex noise
    // coverage=0 -> threshold=1.0 (no clouds)
    // coverage=0.5 -> threshold=~0.0 (noise median)
    // coverage=1 -> threshold=-1.0 (all clouds)
    return 1.0 - coverage * 2.0;

    // For better accuracy, use a polynomial approximation:
    // return 1.0 - 2.0 * pow(coverage, 0.7);
}
```

When using the Schneider remap (`remap(noise, 1-cov, 1, 0, 1) * cov`), the response is
already much more linear because the `* coverage` scales density down at low coverage and
the remap squeezes the density range.

### 3.3 Empirical Tuning (Recommended)

Use a power curve on the coverage parameter, then Schneider remap:

```wgsl
fn adjusted_coverage(slider_value: f32) -> f32 {
    return pow(slider_value, 0.8);  // slight expansion at low end
}
```

### 3.4 Full Coverage Pipeline

```wgsl
fn compute_cloud_density(sphere_pos: vec3<f32>, coverage_slider: f32) -> f32 {
    // 1. Adjust slider for linear visual response
    let coverage = pow(coverage_slider, 0.8);

    // 2. Get climate-modulated local coverage (section 2.4)
    let local_coverage = cloud_coverage_at(sphere_pos, height, coverage);

    // 3. Sample cloud noise with domain warping
    let p = sphere_pos * 5.0 + seed_offset * 3.0;
    let warp = vec3<f32>(
        snoise(p * 0.7 + vec3<f32>(31.7, 0.0, 0.0)),
        snoise(p * 0.7 + vec3<f32>(0.0, 47.3, 0.0)),
        snoise(p * 0.7 + vec3<f32>(0.0, 0.0, 73.1))
    ) * 0.6;

    // 4. Multi-octave fBm on warped position
    var noise = 0.0;
    var freq = 1.0;
    var amp = 1.0;
    let warped_p = p + warp;
    for (var i = 0; i < 5; i++) {
        noise += snoise(warped_p * freq) * amp;
        freq *= 2.1;
        amp *= 0.52;
    }
    noise = noise * 0.5 + 0.5;  // remap to [0,1]

    // 5. Apply coverage via Schneider remap
    let density = remap(noise, 1.0 - local_coverage, 1.0, 0.0, 1.0) * local_coverage;

    return max(density, 0.0);
}
```

---

## 4. Cloud Lighting: Beer-Lambert and Henyey-Greenstein

### 4.1 Beer-Lambert Transmittance

Even without volumetric raymarching, approximate optical depth from a 2D density value:

```wgsl
let optical_depth = density * cloud_thickness_param;  // e.g. 3.0-6.0
let transmittance = exp(-optical_depth);
let cloud_alpha = 1.0 - transmittance;
```

This gives thin wispy clouds that are semi-transparent and dense cores that are opaque --
far more natural than linear opacity.

### 4.2 Beer-Powder Effect (Schneider/Guerrilla)

Prevents dark cloud interiors by combining Beer attenuation with a "powder" term that
approximates increased brightness at thin edges where in-scattering exceeds out-scattering:

```wgsl
fn beer_powder(density: f32) -> f32 {
    let beer = exp(-density);
    let powder = 1.0 - exp(-density * 2.0);
    return beer * mix(1.0, powder, 0.5);
}
```

Full formulation used in ray marching:

```wgsl
float beerPowder = 2.0 * exp(-density) * (1.0 - exp(-2.0 * density));
```

### 4.3 Henyey-Greenstein Phase Function (Detail)

The HG phase function describes angular distribution of scattered light in clouds.
Parameters: g > 0 = forward scattering (silver lining), g < 0 = back scattering.
For clouds, g ~ 0.8 is typical for the primary forward lobe.

```wgsl
// Single-lobe HG
fn phase_hg(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(denom, 1.5));
}

// Dual-lobe: forward scatter (silver lining) + back scatter
// Typical: g1=0.8 (strong forward), g2=-0.5 (weak backward), blend=0.5
fn phase_dual(cos_theta: f32) -> f32 {
    return mix(phase_hg(cos_theta, 0.8), phase_hg(cos_theta, -0.5), 0.5);
}
```

**Physical meaning:** The dual-lobe combines forward scattering (creates the bright "silver
lining" around cloud edges when backlit) with backward scattering. The 0.5 blend is
empirical; production implementations sometimes weight forward more heavily (0.7/0.3).

### 4.4 Cloud Color and Brightness

```wgsl
fn cloud_color(density: f32, shadow: f32, cos_theta: f32) -> vec3<f32> {
    let base_color = vec3<f32>(0.95, 0.95, 0.93);       // slightly warm white
    let shadow_color = vec3<f32>(0.55, 0.58, 0.65);      // blue-grey (sky light)

    var color = mix(shadow_color, base_color, shadow);

    // HG forward scattering: bright edges when backlit
    let g = 0.7;
    let hg = (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * cos_theta, 1.5);
    let silver_lining = hg * density * 0.15;
    color += vec3<f32>(silver_lining);

    // Beer-Powder: thin edges glow brighter when backlit
    let powder = 1.0 - exp(-density * 4.0);
    let beer = exp(-density * 2.0);
    let beer_powder = beer * mix(1.0, powder, 0.5);
    color *= mix(0.8, 1.0, beer_powder);

    return color;
}
```

---

## 5. Cloud Self-Shadowing

The single most important depth cue for clouds from space. Even a single sample offset toward
the light transforms flat white discs into three-dimensional cloud masses.

### 5.1 Single-Sample Shadow (Cheap)

```wgsl
fn cloud_shadow(sphere_pos: vec3<f32>, light_dir: vec3<f32>, density: f32) -> f32 {
    let shadow_offset = 0.03;
    let shadow_pos = normalize(sphere_pos + light_dir * shadow_offset);
    let shadow_density = sample_cloud_density(shadow_pos);
    let shadow = exp(-shadow_density * 2.5);
    return shadow;
}
```

### 5.2 Multi-Sample Shadow (Better Quality)

2-3 samples along the light direction for smoother self-shadowing:

```wgsl
fn cloud_shadow_multisample(pos: vec3<f32>, light_dir: vec3<f32>) -> f32 {
    var shadow_density = 0.0;
    let steps = 3;
    for (var i = 1; i <= steps; i++) {
        let t = f32(i) * 0.02;
        let sample_pos = normalize(pos + light_dir * t);
        shadow_density += sample_cloud_density(sample_pos);
    }
    return exp(-shadow_density * 1.5);
}
```

### 5.3 Compositing Clouds Over the Planet Surface

```wgsl
// After computing surface_color:
let cloud_density = compute_cloud_density(sphere_pos, global_coverage);
let cloud_optical = cloud_density * 4.0;
let cloud_alpha = 1.0 - exp(-cloud_optical);

let shadow = cloud_shadow(sphere_pos, light_dir, cloud_density);
let cos_theta = dot(normalize(sphere_pos), light_dir);
let cloud_col = cloud_color(cloud_density, shadow, cos_theta);

// Clouds also cast shadow on surface below
let surface_shadow = exp(-cloud_density * 1.5);
let shadowed_surface = surface_color * mix(0.6, 1.0, surface_shadow);
let final_color = mix(shadowed_surface, cloud_col, cloud_alpha);
```

---

## 6. Cloud Shadow Casting on Ground

### 6.1 Ray-Marched Shadow (Guerrilla Approach)

For each ground pixel, march a ray toward the sun through the cloud layer. Sample cloud
density along the ray; compute transmittance via Beer-Lambert. Expensive but accurate; can
be done at reduced resolution.

### 6.2 Beer Shadow Maps (BSM)

Render the cloud layer from the light's perspective into a shadow map. Store optical depth
instead of binary shadow. A single `tex3D` lookup gets the volumetric shadow.

### 6.3 Cascaded Deep Opacity Maps

Similar to cascaded shadow maps but store opacity at multiple depths. Covers different
frustum splits with varying resolution.

---

## 7. Volumetric Ray Marching (Full Implementation)

For future upgrade beyond the 2D shell approach.

### 7.1 Ray Marching Pseudocode

```glsl
vec4 raymarchClouds(vec3 rayOrigin, vec3 rayDir) {
    // Intersect ray with cloud shell (e.g., 1500m - 4000m altitude)
    vec2 tMinMax = intersectShell(rayOrigin, rayDir, cloudBottomRadius, cloudTopRadius);

    float transmittance = 1.0;
    vec3 scatteredLight = vec3(0.0);
    float t = tMinMax.x;

    // Bayer 4x4 dither offset to reduce banding
    t += bayerOffset * stepSize;

    for (int i = 0; i < MAX_STEPS; i++) {  // 64-128 steps
        vec3 pos = rayOrigin + rayDir * t;
        float density = sampleCloudDensity(pos);

        if (density > 0.0) {
            // Light march toward sun (6 samples in cone)
            float lightOpticalDepth = lightMarch(pos, sunDir, 6);

            // Beer-Lambert attenuation
            float lightTransmittance = exp(-lightOpticalDepth);

            // Dual-lobe HG phase function
            float phase = mix(phaseHG(cosTheta, 0.8),
                            phaseHG(cosTheta, -0.5), 0.5);

            // Beer-Powder effect
            float beerPowder = 2.0 * exp(-density) * (1.0 - exp(-2.0 * density));

            vec3 luminance = sunColor * lightTransmittance * phase * beerPowder;
            luminance += ambientColor;

            float stepTransmittance = exp(-density * stepSize * extinction);
            scatteredLight += luminance * transmittance * (1.0 - stepTransmittance);
            transmittance *= stepTransmittance;

            if (transmittance < 0.01) break;  // early exit
        }
        t += stepSize;
        if (t > tMinMax.y) break;
    }
    return vec4(scatteredLight, 1.0 - transmittance);
}
```

### 7.2 Noise-Based Cloud Density (HZD 3D Textures)

| Texture          | Resolution | Channels | Content                                          |
| ---------------- | ---------- | -------- | ------------------------------------------------ |
| Base shape       | 128^3      | RGBA     | R: Perlin-Worley, GBA: Worley at increasing freq |
| Detail erosion   | 32^3       | RGB      | Worley at increasing frequencies                 |
| Curl noise (2D)  | 128x128    | RGB      | Non-divergent noise for turbulence               |
| Weather map (2D) | 512x512    | RGB      | R: coverage, G: cloud type, B: wetness           |

```glsl
float sampleCloudDensity(vec3 pos) {
    float heightFrac = getHeightFraction(pos);

    vec3 weather = texture(weatherMap, pos.xz * weatherScale).rgb;
    float coverage = weather.r;

    vec4 baseNoise = texture(baseShapeNoise, pos * baseScale);
    float baseShape = remap(baseNoise.r, -(1.0 - baseNoise.g * 0.625
                            - baseNoise.b * 0.25 - baseNoise.a * 0.125), 1.0, 0.0, 1.0);

    float densityHeightGradient = getDensityForCloudType(heightFrac, weather.g);
    baseShape *= densityHeightGradient;

    float baseDensity = remap(baseShape, 1.0 - coverage, 1.0, 0.0, 1.0);
    baseDensity *= coverage;

    // Detail erosion (only if base > 0 -- "cheap vs expensive" optimization)
    if (baseDensity > 0.0) {
        vec3 detailNoise = texture(detailNoise3D, pos * detailScale).rgb;
        float detailFBM = detailNoise.r * 0.625 + detailNoise.g * 0.25
                        + detailNoise.b * 0.125;
        float detailMod = mix(detailFBM, 1.0 - detailFBM,
                             clamp(heightFrac * 10.0, 0.0, 1.0));
        baseDensity = remap(baseDensity, detailMod * 0.35, 1.0, 0.0, 1.0);
    }
    return max(baseDensity, 0.0);
}
```

---

## 8. Temporal Reprojection Optimization

### 8.1 The Core Performance Problem

Full-resolution ray marching with 64-128 steps + 6 light samples per step is far too
expensive for real-time rendering.

### 8.2 Guerrilla's Temporal Reprojection

- Render clouds at **1/16th resolution** per frame (4x4 checkerboard)
- Use a **4x4 Bayer matrix** pattern to offset ray starting positions, cycling through all
  16 positions over 16 frames
- Reconstruct full resolution by reprojecting previous frames using motion vectors
- 1D Halton sequence of 8 values for additional temporal jittering
- Blend factor: ~75% of the 16th frame to integrate Halton samples
- Invalidated pixels (disocclusion) fall back to noisier single-frame samples

### 8.3 Upsampling Strategies

| Approach                          | Quality                   | Cost                 |
| --------------------------------- | ------------------------- | -------------------- |
| Nearest-neighbor (1/16)           | Low, blocky               | Cheapest             |
| Bilateral upsampling              | Good, preserves edges     | Moderate             |
| Temporal accumulation (Guerrilla) | High, full-res appearance | Needs motion vectors |
| Half-resolution + bilateral       | Good compromise           | ~50% cost saving     |

---

## 9. Hillaire Atmosphere Pipeline (Production Detail)

This section expands on final.md's brief mention of Hillaire 2020 with implementation-ready
specifics.

### 9.1 LUT Pipeline (4 Passes)

| LUT                  | PC Size   | Mobile Size | Steps | PC Cost     | Mobile Cost |
| -------------------- | --------- | ----------- | ----- | ----------- | ----------- |
| Transmittance        | 256 x 64  | 256 x 64    | 40    | 0.01 ms     | 0.53 ms     |
| Multi-Scattering     | 32 x 32   | 32 x 32     | 20    | 0.07 ms     | 0.12 ms     |
| Sky-View             | 200 x 100 | 96 x 50     | 30/8  | 0.05 ms     | 0.27 ms     |
| Aerial Perspective   | 32x32x32  | 32x32x16    | 30/8  | 0.04 ms     | 0.11 ms     |
| **On-screen render** | 1280x720  | --          | --    | **0.14 ms** | --          |
| **Total**            | --        | --          | --    | **0.31 ms** | **~1.0 ms** |

### 9.2 Key Innovations

- **Multi-scattering LUT:** Approximates infinite scattering orders via geometric series:
  `F_ms = 1 / (1 - f_ms)` where `f_ms` is the fraction of light scattered per bounce,
  evaluated with 64 uniformly distributed directions.
- **Sky-View LUT:** Stores distant sky parameterized by latitude/longitude for current camera
  position. When viewed from space, it wastes texels on empty space, so the technique falls
  back to per-pixel ray marching.
- **Aerial Perspective LUT:** Stores in-scattering (RGB) + transmittance (A) as a
  view-frustum-aligned 3D texture with 32 depth slices over 32 km.

### 9.3 Single Scattering with Multi-Scattering Approximation

```
L_scat(camera, x, v) = sigma_s(x) * sum_i [
    T(camera,x) * S(x,l_i) * p(v,l_i) + Psi_ms
] * E_i
```

Where `Psi_ms = L_2nd_order * F_ms`.

---

## 10. Space vs Ground Camera Handling

For a procedural planet renderer, the camera can be anywhere from ground level to deep space.

- **From ground:** Sky-View LUT works well; standard parameterization covers the full
  hemisphere.
- **From space:** Much of the Sky-View LUT is wasted on black space; the technique switches
  to direct ray marching on screen pixels that intersect the atmosphere shell.
- **Transition:** Needs smooth blending between LUT-based and ray-march modes at the
  atmosphere boundary.
- **Planet curvature:** Both Bruneton and Hillaire handle spherical geometry natively
  (parameterized by radius `r` from planet center).

---

## 11. Frame Budget and Performance

### 11.1 Budget Breakdown (Targeting 60fps = 16.6 ms)

| System                        | Typical Cost (PC)  | Notes                                    |
| ----------------------------- | ------------------ | ---------------------------------------- |
| Atmosphere LUTs (Hillaire)    | 0.17 ms precompute | Updated per frame                        |
| Atmosphere on-screen          | 0.14 ms            | Sky-View LUT sample + aerial perspective |
| Cloud ray marching (1/16 res) | 1.5-2.0 ms         | 64 steps + 6 light samples               |
| Cloud temporal reproject      | 0.1-0.3 ms         | Motion vectors + blend                   |
| Cloud shadows                 | 0.2-0.5 ms         | BSM or reduced-res ray march             |
| **Total atmosphere+clouds**   | **~2-3 ms**        | Leaves ~13 ms for terrain, objects, post |

### 11.2 Resolution Tradeoffs

| Technique           | Low                | Medium             | High              | Ultra               |
| ------------------- | ------------------ | ------------------ | ----------------- | ------------------- |
| Cloud ray march     | 1/16 res, 32 steps | 1/16 res, 64 steps | 1/4 res, 64 steps | Full res, 128 steps |
| Atmosphere Sky-View | 96x50              | 128x64             | 200x100           | 200x100             |
| Atmosphere Aerial   | 32x32x16           | 32x32x32           | 64x64x32          | 64x64x64            |

### 11.3 2D Shell Cost Estimate (Planet Gen Preview)

For our 2D shell approach (no volumetric ray marching):

| Phase                  | snoise Calls | Description          |
| ---------------------- | ------------ | -------------------- |
| Phase 1: Basic density | 8            | 5 fBm + 3 warp       |
| Phase 2: + Climate mod | 11           | + 3 climate warp     |
| Phase 3: + Self-shadow | 12-14        | + 1-3 shadow samples |

At 14 snoise calls per cloud fragment, comparable to existing terrain pipeline (8-12 octave
fBm). Reducing to 4 fBm octaves + 1 shadow sample keeps it under 10 calls.

---

## 12. Phased Implementation Strategy

### Phase 1: Basic Cloud Density (Minimum Viable)

1. Add `cloud_coverage` uniform (0.0 to 1.0 slider)
2. Domain-warped 5-octave fBm at frequency 5.0 on sphere surface
3. Schneider remap with coverage parameter for thresholding
4. Beer-Lambert opacity: `alpha = 1.0 - exp(-density * 4.0)`
5. Simple white color with NdotL lighting
6. Composite over surface with `mix(surface, cloud_color, cloud_alpha)`

**Cost**: ~8 snoise calls per fragment

### Phase 2: Climate Modulation

1. Compute coverage map from existing moisture data
2. Domain-warp the moisture lookup position to break latitude bands
3. Blend global coverage with climate: `mix(global, climate, 0.35)`
4. Use local coverage in Schneider remap

**Cost**: +3 snoise calls = 11 total

### Phase 3: Depth and Lighting

1. Self-shadowing: 1-3 samples offset toward light direction
2. Cloud color: shadow tint (blue-grey), lit tint (warm white)
3. Optional: Henyey-Greenstein for silver lining effect
4. Optional: Beer-Powder for bright thin edges

**Cost**: +1-3 snoise calls = 12-14 total

### Phase 4: Full Volumetric (Future)

1. Transition from 2D shell to ray marching through cloud shell
2. Add 3D noise textures (base shape + detail erosion)
3. Implement temporal reprojection at 1/16th resolution
4. Add Beer Shadow Maps for ground shadows

---

## 13. References

### Authoritative Sources

| Source                                         | Key Contribution                                                               | URL                                                                                                                                                                                  |
| ---------------------------------------------- | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Andrew Schneider, Guerrilla Games (2015, 2017) | Perlin-Worley noise, remap coverage, Beer-Powder, height-gradient cloud typing | [SIGGRAPH Slides](https://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf)                           |
| Inigo Quilez -- "Dynamic 2D Clouds"            | 2D fBm threshold/remap, self-shadowing, domain warping                         | [iquilezles.org](https://iquilezles.org/articles/dynclouds/)                                                                                                                         |
| Skybolt Engine                                 | Planetary-scale volumetric clouds, inner/outer shell raymarching               | [prograda.com](https://prograda.com/2021/07/28/rendering-planetwide-volumetric-clouds-in-skybolt/)                                                                                   |
| Jan Wedekind                                   | Curl noise on sphere for flow-like cyclone patterns                            | [wedesoft.de](https://www.wedesoft.de/software/2023/03/20/procedural-global-cloud-cover/)                                                                                            |
| JP Grenier                                     | Practical HZD-style cloud implementation notes                                 | [jpgrenier.org](https://www.jpgrenier.org/clouds.html)                                                                                                                               |
| Sebastien Hillaire (Epic, 2020)                | Scalable atmosphere, multi-scattering LUT, Sky-View LUT                        | [Paper](https://onlinelibrary.wiley.com/doi/full/10.1111/cgf.14050) / [Slides](https://blog.selfshadow.com/publications/s2020-shading-course/hillaire/s2020_pbs_hillaire_slides.pdf) |
| Sakmary (2023)                                 | Real-time atmosphere + clouds in Vulkan                                        | [CESCG Paper](https://cescg.org/wp-content/uploads/2023/04/Sakmary-Real-time-Rendering-of-Atmosphere-and-Clouds-in-Vulkan.pdf)                                                       |

### Shadertoy References

- **Clouds by Quilez**: [shadertoy.com/view/XslGRr](https://www.shadertoy.com/view/XslGRr) -- Volumetric raymarched with self-shadowing
- **Planet Shadertoy**: [shadertoy.com/view/4tjGRh](https://www.shadertoy.com/view/4tjGRh) -- Full planet with atmosphere, cloud shell
- **Volumetric Cloud**: [shadertoy.com/view/3sffzj](https://www.shadertoy.com/view/3sffzj)
- **Real-time PBR Volumetric Clouds**: [shadertoy.com/view/MstBWs](https://www.shadertoy.com/view/MstBWs)

### Additional Resources

- [pixelsnafu cloud resources gist](https://gist.github.com/pixelsnafu/e3904c49cbd8ff52cb53d95ceda3980e)
- [webgpu-sky-atmosphere](https://github.com/JolifantoBambla/webgpu-sky-atmosphere) -- WebGPU atmosphere implementation
- [glsl-atmosphere (minimal)](https://github.com/wwwtyro/glsl-atmosphere)
- [GameDev.net Planet Atmosphere](https://www.gamedev.net/blogs/entry/2276727-planet-rendering-atmosphere/)
- [Vertex Fragment upsampling](https://www.vertexfragment.com/ramblings/volumetric-cloud-upsampling/)
- [PBR Book Phase Functions](https://pbr-book.org/3ed-2018/Volume_Scattering/Phase_Functions)
- [NVIDIA Approximate Mie](https://research.nvidia.com/labs/rtr/approximate-mie/publications/approximate-mie.pdf)
- [Alan Zucconi atmospheric scattering tutorial](https://www.alanzucconi.com/2017/10/10/atmospheric-scattering-7/)
