# Cloud Layer Rendering for Procedural Planets

Research document covering cloud density generation, climate modulation, rendering from space,
coverage slider design, and reference implementations for a 2D shell cloud layer in a WGSL
fragment shader.

---

## 1. Cloud Density Map Generation

### 1.1 Noise Architecture: What Produces Natural Cloud Shapes

The industry-standard approach (Horizon Zero Dawn / Frostbite / Skybolt) builds cloud density
from layered noise, but the key insight is that **pure fBm does not look like clouds**. Clouds
have billowy rounded tops and flat bases with sharp-edged voids between them. Two noise
modifications achieve this:

**Perlin-Worley hybrid**: The base shape uses Perlin noise modulated by inverted Worley noise.
Worley F1 produces cell-like voids; inverting it (1 - F1) creates rounded blobs. Blending
Perlin with inverted Worley gives organic billowy shapes with clear sky gaps between cloud
masses.

```wgsl
// Perlin-Worley base: rounded cloud masses with clear gaps
fn cloud_base_noise(p: vec3<f32>) -> f32 {
    let perlin = snoise(p);
    let worley = 1.0 - worley_f1(p);  // invert: blobs instead of cells
    return remap(perlin, worley * 0.625 - 1.0, 1.0, 0.0, 1.0);
}
```

**If Worley noise is unavailable** (our case -- we only have simplex/snoise), the alternative
is domain-warped fBm. Inigo Quilez's domain warping feeds noise into itself, creating organic
swirling shapes that avoid the blobby uniformity of plain fBm:

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

### 1.2 Noise Parameters for Space-View Clouds

From analysis of Earth cloud imagery and existing implementations:

| Parameter      | Value           | Rationale                                          |
|----------------|-----------------|-----------------------------------------------------|
| Base frequency | 4.0 - 6.0      | Gives ~6-10 major cloud systems visible from space   |
| Octaves        | 4 - 6           | More than terrain (for wispy edges) but fewer than 8 |
| Lacunarity     | 2.0 - 2.2       | Standard doubling; 2.2 reduces alignment artifacts   |
| Gain           | 0.5 - 0.55      | Standard; higher gain = more wispy detail            |
| Domain warp    | 0.5 - 0.8       | Strength of warp displacement; too high = smeared    |

**Octave count matters**: 4 octaves gives chunky masses, 6 gives wispy edges. For space view,
5 is a good default. Each octave costs one snoise call.

### 1.3 Thresholding Without Cliff Effects

The raw noise produces values in roughly [-1, 1]. Converting this to cloud density requires
thresholding, but naive `max(noise - threshold, 0.0)` creates hard edges. Two approaches solve this:

**Approach A: Quilez remap** (the classic, from "Dynamic 2D Clouds" article)
```wgsl
// Threshold + remap to [0,1]. Equivalent to a saturate(MAD).
fn cloud_density_from_noise(noise_val: f32, threshold: f32) -> f32 {
    // threshold: higher = less cloud coverage
    // sharpness: controls how quickly density ramps up above threshold
    let sharpness = 1.0 / (1.0 - threshold);  // auto-scale
    return clamp((noise_val - threshold) * sharpness, 0.0, 1.0);
}
```

**Approach B: Schneider remap** (Horizon Zero Dawn)
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

The Schneider remap is preferred because it naturally produces **lighter small clouds and
denser large clouds** -- when coverage is low, the remap squeezes density range, making
isolated clouds thinner. When coverage is high, most of the noise passes through, making
large cloud masses opaque.

---

## 2. Climate Modulation: Breaking Latitude Bands

### 2.1 The Core Problem

Multiplying cloud noise by moisture directly (`cloud = noise * moisture`) creates visible
latitude bands because the Hadley cell moisture function has strong latitudinal structure:
- ITCZ band at equator (high moisture)
- Subtropical dry bands at ~25-30 degrees
- Polar front at ~60 degrees

### 2.2 Solution: Noise Drives Threshold, Not Amplitude

The key insight from Schneider/HZD: **climate data should control the coverage threshold,
not be multiplied with density**. This is the `remap(base, 1.0 - coverage, 1.0, 0.0, 1.0)`
technique.

Instead of: `density = noise * moisture` (creates bands)

Do: `density = remap(noise, 1.0 - moisture_coverage, 1.0, 0.0, 1.0)`

This means:
- In high-moisture (ITCZ) regions: threshold is low, so most noise values produce clouds
- In dry (subtropical) regions: threshold is high, only the strongest noise peaks form clouds
- But the **shape** of each cloud is still determined by the noise, not by latitude

### 2.3 Additional Band-Breaking Techniques

**A. Domain warping the coverage map itself:**
Apply noise displacement to the position used for moisture lookup, so the climate zones
themselves become wavy rather than straight latitude lines.

```wgsl
// Warp the position used for climate lookup
let climate_warp = vec3<f32>(
    snoise(sphere_pos * 2.0 + vec3<f32>(200.0, 0.0, 0.0)),
    snoise(sphere_pos * 2.0 + vec3<f32>(0.0, 300.0, 0.0)),
    snoise(sphere_pos * 2.0 + vec3<f32>(0.0, 0.0, 400.0))
) * 0.15;
let warped_pos = normalize(sphere_pos + climate_warp);
let moisture_for_clouds = compute_moisture(warped_pos, height);
```

**B. Curl noise for flow-like patterns:**
Jan Wedekind's approach: compute the gradient of a noise potential field on the sphere surface,
then rotate 90 degrees to get divergence-free flow vectors. Advecting cloud noise along these
vectors creates swirling, cyclone-like patterns that naturally break latitude bands.

For our 2D shell case, a simplified version: use curl-like displacement by sampling noise
gradients and rotating them:

```wgsl
// Simplified curl-like displacement on sphere
fn curl_displacement(p: vec3<f32>, strength: f32) -> vec3<f32> {
    let eps = 0.01;
    let n = normalize(p);  // sphere normal
    // Approximate gradient of noise potential
    let dx = snoise(p + vec3<f32>(eps, 0.0, 0.0)) - snoise(p - vec3<f32>(eps, 0.0, 0.0));
    let dy = snoise(p + vec3<f32>(0.0, eps, 0.0)) - snoise(p - vec3<f32>(0.0, eps, 0.0));
    let dz = snoise(p + vec3<f32>(0.0, 0.0, eps)) - snoise(p - vec3<f32>(0.0, 0.0, eps));
    let grad = vec3<f32>(dx, dy, dz) / (2.0 * eps);
    // Cross product with normal = tangential curl
    let curl = cross(n, grad);
    return curl * strength;
}
```

**C. Latitude power softening:**
Instead of using moisture directly, compress the latitude influence with a power curve:

```wgsl
// Soften latitude influence: 70% noise, 30% climate
let climate_factor = 0.3 + 0.7 * moisture_normalized;
// Or equivalently:
let cloud_coverage = mix(global_coverage, moisture_normalized, 0.3);
```

### 2.4 Recommended Blend Strategy

The strongest approach combines (A) and the threshold technique from 2.2:

1. Compute a **cloud-specific coverage map** from moisture, with domain warping applied
2. Normalize moisture to [0, 1] and blend with a global coverage parameter
3. Use the Schneider remap: `remap(noise, 1.0 - coverage, 1.0, 0.0, 1.0)`
4. This gives climate-correlated cloud distribution without visible banding

```wgsl
fn cloud_coverage_at(sphere_pos: vec3<f32>, height: f32, global_coverage: f32) -> f32 {
    // Warp position for climate lookup to break bands
    let warp = vec3<f32>(
        snoise(sphere_pos * 2.5 + vec3<f32>(200.0, 0.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 300.0, 0.0)),
        snoise(sphere_pos * 2.5 + vec3<f32>(0.0, 0.0, 400.0))
    ) * 0.12;
    let warped_pos = normalize(sphere_pos + warp);

    // Get moisture (0-400 range in current code)
    let moisture = compute_moisture(warped_pos, height);
    let moisture_norm = clamp(moisture / 300.0, 0.0, 1.0);  // normalize

    // Blend global coverage with climate: climate nudges but doesn't dominate
    // At global_coverage=0.5: dry regions get ~0.35, wet regions get ~0.65
    return mix(global_coverage, moisture_norm, 0.35);
}
```

---

## 3. Cloud Rendering From Space (2D Shell)

### 3.1 Avoiding Flat White Appearance

The biggest pitfall in 2D cloud rendering is flat white discs. Three techniques add depth:

**A. Density-dependent opacity (Beer-Lambert on a shell):**
Even without volumetric raymarching, approximate optical depth from the 2D density value:

```wgsl
let optical_depth = density * cloud_thickness_param;  // e.g. 3.0-6.0
let transmittance = exp(-optical_depth);
let cloud_alpha = 1.0 - transmittance;
```

This gives thin wispy clouds that are semi-transparent and dense cores that are opaque --
much more natural than linear opacity.

**B. Self-shadowing approximation (Quilez technique):**
Sample the cloud density at a position offset toward the light source. The difference
approximates how much cloud the light traverses before reaching this point:

```wgsl
// Fake self-shadowing for 2D cloud shell
fn cloud_shadow(sphere_pos: vec3<f32>, light_dir: vec3<f32>, density: f32) -> f32 {
    // Sample density at a position shifted toward the light
    let shadow_offset = 0.03;  // small offset along light direction
    let shadow_pos = normalize(sphere_pos + light_dir * shadow_offset);
    let shadow_density = sample_cloud_density(shadow_pos);

    // More density toward light = darker shadow
    let shadow = exp(-shadow_density * 2.5);
    return shadow;
}
```

This creates dark undersides on clouds facing away from the sun and bright tops on the
sunward side -- the single most important depth cue for clouds from space.

**C. Multi-sample shadow (better quality, higher cost):**
Take 2-3 samples along the light direction for smoother self-shadowing:

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

### 3.2 Cloud Color and Brightness

Clouds are not pure white. From space, cloud brightness varies with:

```wgsl
fn cloud_color(density: f32, shadow: f32, cos_theta: f32) -> vec3<f32> {
    // Base cloud albedo: slightly warm white
    let base_color = vec3<f32>(0.95, 0.95, 0.93);

    // Shadow darkens toward blue-grey (scattered sky light in shadow)
    let shadow_color = vec3<f32>(0.55, 0.58, 0.65);

    // Blend based on shadow term
    var color = mix(shadow_color, base_color, shadow);

    // Henyey-Greenstein forward scattering: bright edges when backlit
    let g = 0.7;
    let hg = (1.0 - g * g) / pow(1.0 + g * g - 2.0 * g * cos_theta, 1.5);
    let silver_lining = hg * density * 0.15;
    color += vec3<f32>(silver_lining);

    // Beer-Powder: thin cloud edges glow brighter when backlit
    let powder = 1.0 - exp(-density * 4.0);
    let beer = exp(-density * 2.0);
    let beer_powder = beer * mix(1.0, powder, 0.5);

    color *= mix(0.8, 1.0, beer_powder);

    return color;
}
```

### 3.3 Compositing Clouds Over the Planet Surface

```wgsl
// In the main fragment shader, after computing surface_color:
let cloud_density = compute_cloud_density(sphere_pos, global_coverage);
let cloud_optical = cloud_density * 4.0;
let cloud_alpha = 1.0 - exp(-cloud_optical);

let shadow = cloud_shadow(sphere_pos, light_dir, cloud_density);
let cos_theta = dot(normalize(sphere_pos), light_dir);
let cloud_col = cloud_color(cloud_density, shadow, cos_theta);

// Clouds block surface and are lit independently
let final_color = mix(surface_color, cloud_col, cloud_alpha);

// Clouds also cast shadow on surface below (optional)
let surface_shadow = exp(-cloud_density * 1.5);
let shadowed_surface = surface_color * mix(0.6, 1.0, surface_shadow);
let final_color = mix(shadowed_surface, cloud_col, cloud_alpha);
```

---

## 4. Coverage Slider Implementation

### 4.1 The Cliff Problem

A naive coverage slider that directly thresholds noise creates a non-linear visual response:
- 0.0 to 0.3: almost no visible clouds (noise rarely exceeds high threshold)
- 0.3 to 0.5: rapid explosion of cloud coverage
- 0.5 to 0.7: most of the planet covered
- 0.7 to 1.0: barely any change (already nearly full coverage)

This happens because noise values follow an approximately Gaussian distribution. The CDF
of a Gaussian is an S-curve, so linear threshold changes produce S-curve area changes.

### 4.2 Solution: CDF-Compensated Remapping

To make the slider feel linear, we need to invert the noise CDF. For simplex noise (which
is approximately Gaussian with mean ~0 and std ~0.35):

```wgsl
// Map linear coverage [0,1] to noise threshold that produces
// approximately that visual coverage fraction
fn coverage_to_threshold(coverage: f32) -> f32 {
    // Approximate inverse Gaussian CDF for simplex noise
    // coverage=0 -> threshold=1.0 (no clouds)
    // coverage=0.5 -> threshold=~0.0 (noise median)
    // coverage=1 -> threshold=-1.0 (all clouds)
    return 1.0 - coverage * 2.0;  // Linear in noise space

    // For better accuracy, use a polynomial approximation:
    // return 1.0 - 2.0 * pow(coverage, 0.7);
    // The power < 1.0 stretches the low-coverage end
}
```

However, when using the Schneider remap approach (`remap(noise, 1-cov, 1, 0, 1) * cov`),
the response is already much more linear because:
- The `* coverage` at the end scales overall density down at low coverage
- The remap squeezes the density range, making small clouds thinner
- Together these produce a reasonably linear visual response

### 4.3 Empirical Tuning Approach (Recommended)

Rather than exact CDF inversion, use a power curve on the coverage parameter:

```wgsl
fn adjusted_coverage(slider_value: f32) -> f32 {
    // Power curve makes low/high ends more responsive
    // Tuned empirically: 0.3 input -> ~30% visible cloud area
    return pow(slider_value, 0.8);  // slight expansion at low end
}
```

Then use the Schneider remap with this adjusted coverage. The combination gives a perceptually
linear slider without needing to know the exact noise distribution.

### 4.4 Full Coverage Pipeline

```wgsl
fn compute_cloud_density(sphere_pos: vec3<f32>, coverage_slider: f32) -> f32 {
    // 1. Adjust slider for linear visual response
    let coverage = pow(coverage_slider, 0.8);

    // 2. Get climate-modulated local coverage (section 2.4)
    let local_coverage = cloud_coverage_at(sphere_pos, height, coverage);

    // 3. Sample cloud noise with domain warping
    let p = sphere_pos * 5.0 + seed_offset * 3.0;  // base frequency
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

## 5. Reference Implementations and Key Sources

### 5.1 Authoritative References

**Andrew Schneider, Guerrilla Games (2015, 2017)** -- "The Real-time Volumetric Cloudscapes
of Horizon Zero Dawn"
- PDF: https://advances.realtimerendering.com/s2015/The%20Real-time%20Volumetric%20Cloudscapes%20of%20Horizon%20-%20Zero%20Dawn%20-%20ARTR.pdf
- Introduced: Perlin-Worley noise, remap-based coverage, Beer-Powder lighting,
  height-gradient cloud typing
- The remap function for coverage is THE key technique for natural cloud appearance

**Inigo Quilez -- "Dynamic 2D Clouds"**
- URL: https://iquilezles.org/articles/dynclouds/
- Key technique: 2D fBm cloud layers with threshold/remap for coverage control
- Self-shadowing via sampling density offset toward light direction
- Domain warping for organic shapes
- Most directly applicable to our 2D shell approach

**Skybolt Engine -- "Rendering Planetwide Volumetric Clouds"**
- URL: https://prograda.com/2021/07/28/rendering-planetwide-volumetric-clouds-in-skybolt/
- Planetary-scale cloud rendering with inner/outer shell raymarching
- 2D global coverage map + tiling Worley modulation + 3D volume detail
- Multi-scale approach: global coverage (low freq) + procedural detail (high freq)

**Jan Wedekind -- "Procedural Generation of Global Cloud Cover"**
- URL: https://www.wedesoft.de/software/2023/03/20/procedural-global-cloud-cover/
- Curl noise on a sphere surface for flow-like, cyclone patterns
- Gradient projection onto sphere + 90-degree rotation = divergence-free flow
- Excellent for breaking latitude bands and creating organic planetary patterns

**JP Grenier -- Volumetric Clouds Blog**
- URL: https://www.jpgrenier.org/clouds.html
- Practical implementation of HZD-style clouds
- Finding good sampling scales is noted as the hardest part
- Adding extra coverage at lower altitudes fixes oddly-shaped distant clouds

### 5.2 Shadertoy References

- **Clouds by Inigo Quilez**: https://www.shadertoy.com/view/XslGRr
  (Volumetric raymarched clouds with self-shadowing)
- **Planet Shadertoy**: https://www.shadertoy.com/view/4tjGRh
  (Full planet with atmosphere, cloud shell)
- **Volumetric Cloud**: https://www.shadertoy.com/view/3sffzj
- **Real-time PBR Volumetric Clouds**: https://www.shadertoy.com/view/MstBWs

### 5.3 Beer-Lambert and Phase Functions (for reference)

```wgsl
// Beer-Lambert: light absorption through medium
fn beer_lambert(density: f32, distance: f32) -> f32 {
    return exp(-density * distance);
}

// Beer-Powder: prevents dark cloud interiors (Schneider/Guerrilla)
fn beer_powder(density: f32) -> f32 {
    let beer = exp(-density);
    let powder = 1.0 - exp(-density * 2.0);
    return beer * mix(1.0, powder, 0.5);
}

// Henyey-Greenstein phase function
fn phase_hg(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = 1.0 + g2 - 2.0 * g * cos_theta;
    return (1.0 - g2) / (4.0 * 3.14159 * pow(denom, 1.5));
}

// Dual-lobe: forward scatter (silver lining) + back scatter
fn phase_dual(cos_theta: f32) -> f32 {
    return mix(phase_hg(cos_theta, 0.8), phase_hg(cos_theta, -0.5), 0.5);
}
```

---

## 6. Recommended Implementation Strategy for Planet-Gen

Given our constraints (WGSL fragment shader, simplex noise only, 2D shell, existing climate
data), the recommended approach:

### Phase 1: Basic Cloud Density (Minimum Viable)

1. Add `cloud_coverage` uniform (0.0 to 1.0 slider)
2. Domain-warped 5-octave fBm at frequency 5.0 on sphere surface
3. Schneider remap with coverage parameter for thresholding
4. Beer-Lambert opacity: `alpha = 1.0 - exp(-density * 4.0)`
5. Simple white color with NdotL lighting
6. Composite over surface with `mix(surface, cloud_color, cloud_alpha)`

**Cost**: ~5 snoise calls (fBm) + 3 snoise calls (warp) = 8 snoise per fragment

### Phase 2: Climate Modulation

1. Compute coverage map from existing moisture data
2. Domain-warp the moisture lookup position to break latitude bands
3. Blend global coverage with climate: `mix(global, climate, 0.35)`
4. Use this local coverage in the Schneider remap

**Cost**: +3 snoise calls for climate warp = 11 total

### Phase 3: Depth and Lighting

1. Self-shadowing: 1-3 samples offset toward light direction
2. Cloud color: shadow tint (blue-grey), lit tint (warm white)
3. Optional: Henyey-Greenstein for silver lining effect
4. Optional: Beer-Powder for bright thin edges

**Cost**: +1-3 snoise calls for shadow samples = 12-14 total

### Performance Note

At 14 snoise calls per cloud fragment, this is comparable to the existing terrain pipeline
(8-12 octave fBm). Clouds only need to be evaluated for visible sphere fragments that are
not culled, so the total cost is manageable. If performance is tight, reducing cloud fBm
to 4 octaves and using 1 shadow sample keeps it under 10 calls.

---

## 7. Key Takeaways

1. **Do not multiply noise by climate data** -- use climate to control the coverage threshold
   via remap. This is the single most important technique for avoiding latitude bands.

2. **Domain warping is essential** -- both for organic cloud shapes (warping the noise itself)
   and for breaking climate bands (warping the moisture lookup position).

3. **Self-shadowing is the #1 depth cue** -- even a single sample offset toward the light
   transforms flat white discs into three-dimensional cloud masses.

4. **Beer-Lambert opacity** instead of linear opacity prevents the "flat alpha" look where
   thin and thick clouds have similar appearance.

5. **The Schneider remap** (`remap(noise, 1-cov, 1, 0, 1) * cov`) naturally produces lighter
   small clouds and denser large clouds, giving a more linear coverage slider response
   than naive thresholding.
