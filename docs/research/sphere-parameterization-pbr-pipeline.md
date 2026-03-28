# Sphere Parameterization and Projection Methods for Procedural Planet Generation

_Research date: 2026-03-28 (updated with extensive web research, second pass)_

---

## 1. Cube-to-Sphere Mapping

### 1.1 Naive Normalization (Gnomonic Projection)

The simplest cube-to-sphere mapping normalizes each cube vertex position to unit length.

**Formula:**

Given a point `p = (x, y, z)` on a unit cube face:

```
sphere_point = p / |p| = (x, y, z) / sqrt(x^2 + y^2 + z^2)
```

**Distortion analysis:**
- Vertices at face centers (e.g. `(1, 0, 0)`) do not move at all
- Vertices at cube corners (e.g. `(1, 1, 1)`) move the most -- distance from `sqrt(3)` to `1.0`
- Because normalization draws points towards the center, the more they move the more they bunch up
- Grid cells near face centers are mostly square; cells near cube corners are heavily distorted
- **Max/min area ratio: ~3.5-5.2:1** (worst among common methods)
- Neither equal-area nor conformal

**GPU pseudocode (GLSL):**

```glsl
vec3 cubeToSphere(vec3 p) {
    return normalize(p);
}
```

### 1.2 Analytic Mapping (Improved Uniform Distribution)

An analytic formula produces a more uniform vertex distribution by redistributing points away from corners.

**Formula:**

For a cube-face point `p = (x, y, z)` where one component is +/-1:

```
sphere_point = p * sqrt(1 - (p_yx^2 + p_zz^2) / 2 + (p_yx^2 * p_zz^2) / 3)
```

Where `p_yx = (y^2, x^2, x^2)` and `p_zz = (z^2, z^2, y^2)` (component-wise swizzles).

Expanded for the +X face where `x = 1`:

```
sx = 1.0 * sqrt(1.0 - y^2/2.0 - z^2/2.0 + y^2*z^2/3.0)
sy = y   * sqrt(1.0 - z^2/2.0 - 1.0/2.0 + z^2*1.0/3.0)
sz = z   * sqrt(1.0 - 1.0/2.0 - y^2/2.0 + 1.0*y^2/3.0)
```

**Distortion analysis:**
- Points are pulled toward the middle of square edges instead of corners
- Produces a more uniform vertex layout than simple normalization
- Still not perfectly uniform -- some bunching at edge midpoints
- **Max/min area ratio: ~1.57-1.8:1** (significant improvement over naive)

**GPU pseudocode (GLSL):**

```glsl
vec3 cubeToSphereAnalytic(vec3 p) {
    vec3 p2 = p * p;
    return p * sqrt(1.0 - (p2.yxx + p2.zzy) / 2.0 + (p2.yxx * p2.zzy) / 3.0);
}
```

### 1.3 Tangent-Adjusted Mapping

Pre-warps UV coordinates on each cube face using the tangent function so that the projection from cube to sphere produces uniform angular spacing.

**Formula:**

Forward transform (cube UV to adjusted UV):

```
w_adjusted = tan(w * pi/4)    where w in [-1, 1]
```

This maps equal-length subdivisions of the square edge to equal-angle subdivisions of the circumscribed arc. Then apply normalization:

```
sphere_point = normalize(face_normal + w_adjusted_u * right + w_adjusted_v * up)
```

Inverse transform (for texture lookup):

```
w_original = (4/pi) * atan(w_adjusted)
```

**Distortion analysis:**
- **Max/min area ratio: ~1.414:1 (sqrt(2))** -- far better than gnomonic
- Neither equal-area nor conformal, but a good practical compromise
- Used by Google's S2 geometry library as one of its projection options
- Very fast to compute on GPU (single `tan` or `atan` per axis)

**GPU pseudocode (GLSL):**

```glsl
vec3 cubeToSphereTangent(vec3 faceNormal, vec2 uv, vec3 right, vec3 up) {
    // Pre-warp UVs with tangent
    vec2 warped = tan(uv * 0.7853981633974483); // pi/4
    // Construct 3D point on face and normalize
    vec3 p = faceNormal + warped.x * right + warped.y * up;
    return normalize(p);
}

// Inverse: sphere to cube face UV
vec2 sphereToCubeTangent(vec2 uv) {
    return (4.0 / 3.14159265) * atan(uv);
}
```

### 1.4 Equal-Area Projection (Arvo / Inverse Lambert Azimuthal)

Constructs a perfectly area-preserving map from cube to sphere in two steps.

**Step 1 -- Map each cube face to a curved square:**

Each face of the cube `[-1,1]^2` is mapped to a domain bounded by a curved square using a nonlinear transformation that preserves area:

```
For face point (u, v):
    u' = u * sqrt(1 - v^2/3)
    v' = v * sqrt(1 - u^2/3)
```

(This maps the square `[-1,1]^2` to a curved region inscribed within a circle of radius `sqrt(2/3)`.)

**Step 2 -- Inverse Lambert azimuthal equal-area projection:**

Map the curved square point to the sphere:

```
r^2 = u'^2 + v'^2
z = 1 - r^2
x = u' * sqrt(2 - r^2)
y = v' * sqrt(2 - r^2)
```

Rotated to the appropriate face orientation.

**Distortion analysis:**
- **Area distortion: 1.0:1 (perfectly equal-area)**
- Angular distortion is present but bounded
- Max angular distortion occurs at face corners
- More expensive than tangent mapping (extra `sqrt` operations)

**GPU pseudocode (GLSL):**

```glsl
vec3 cubeToSphereEqualArea(vec3 faceNormal, vec2 uv, vec3 right, vec3 up) {
    // Step 1: Map to curved square
    float u2 = uv.x * uv.x;
    float v2 = uv.y * uv.y;
    float u_prime = uv.x * sqrt(1.0 - v2 / 3.0);
    float v_prime = uv.y * sqrt(1.0 - u2 / 3.0);

    // Step 2: Inverse Lambert azimuthal
    float r2 = u_prime * u_prime + v_prime * v_prime;
    float sz = 1.0 - r2;
    float scale = sqrt(2.0 - r2);
    float sx = u_prime * scale;
    float sy = v_prime * scale;

    return sx * right + sy * up + sz * faceNormal;
}
```

### 1.5 Quadrilateralized Spherical Cube (QSC / COBE)

Used by the COBE satellite mission (1975, Chan & O'Neill). Projects the sphere onto an inscribed cube using a curvilinear transformation that compensates for the distortion of the gnomonic projection.

**Properties:**
- Equal-area projection
- Limited angular distortion (~22 degrees max at face edges)
- 6 faces, each with `2^(2N)` bins at depth N
- Total bins: `6 * 2^(2N)`, requiring `2N + 3` bits for addressing
- COBE parameterization: average error 4.7 arcsec, RMS 6.6 arcsec, peak 24 arcsec
- Implemented in PROJ library (`+proj=qsc`)

### 1.6 Outerra / Adjusted Spherical Cube

The Dimitrijevic & Lambers comparison paper [S-NEW1] evaluates an "Outerra" spherical cube map projection, which is an approximately equal-area projection based on sphere representation in Cartesian coordinates. Key findings from their evaluation:

- The **adjusted spherical cube** achieves best angular distortion when using geocentric latitude and best areal distortion when using approximated authalic latitude
- Properties improve by 0.8% to 1.3% compared to geodesic latitude
- **Optimal rotation** of the base cube to minimize angular distortion over Earth's landmasses: 17 deg longitude, -10 deg latitude, 32 deg rotation around the perpendicular axis
- For Discrete Global Grid Systems (DGGS), distortion-optimized cube orientations reduce area distortion to near zero across continental plates [S-NEW2]

### 1.7 Comparison Table

| Method | Max/Min Area Ratio | Conformal? | Equal-Area? | GPU Cost | Best For |
|--------|-------------------|------------|-------------|----------|----------|
| Naive (gnomonic) | ~3.5-5.2:1 | No | No | Cheapest (normalize) | Prototyping |
| Analytic | ~1.57-1.8:1 | No | No | Low (1 sqrt) | General purpose |
| Tangent-adjusted | ~1.414:1 | No | No | Low (1 tan/atan) | Real-time rendering |
| Equal-area (Lambert) | 1.0:1 | No | Yes | Medium (2 sqrt) | Scientific visualization |
| QSC (COBE) | 1.0:1 | No | Yes | Medium | Data storage, CMB maps |

---

## 2. HEALPix (Hierarchical Equal Area isoLatitude Pixelization)

### 2.1 Overview

HEALPix tessellates the sphere into curvilinear quadrilaterals with three key properties:
1. **Equal area**: all pixels at a given resolution cover the same surface area
2. **Isolatitude**: pixel centers lie on discrete circles of latitude with equal spacing on each ring
3. **Hierarchical**: recursive quadtree subdivision from 12 base pixels

Devised in 1997 by Krzysztof M. Gorski for cosmic microwave background satellite missions (WMAP, Planck).

### 2.2 Mathematical Formulas

**Resolution parameter:**

```
N_side = 2^k    (k = 0, 1, 2, ...)
```

**Total number of pixels:**

```
N_pix = 12 * N_side^2
```

**Pixel area (steradians):**

```
Omega_pix = 4 * pi / N_pix = pi / (3 * N_side^2)
```

**Angular resolution:**

```
theta_pix = sqrt(Omega_pix) = sqrt(pi / (3 * N_side^2))
```

| N_side | N_pix | Pixel Area (sr) | Angular Resolution |
|--------|-------|-----------------|-------------------|
| 1 | 12 | 1.0472 | ~58.6 deg |
| 2 | 48 | 0.2618 | ~29.3 deg |
| 4 | 192 | 0.06545 | ~14.7 deg |
| 8 | 768 | 0.01636 | ~7.3 deg |
| 64 | 49,152 | 2.557e-4 | ~0.92 deg |
| 512 | 3,145,728 | 3.996e-6 | ~0.11 deg |

**Ring structure:**
- **Polar caps**: `N_side - 1` rings with increasing pixel count per ring
- **Equatorial zone**: `2 * N_side + 1` rings, each with `4 * N_side` pixels
- Total rings: `4 * N_side - 1`

**Projection construction:**
- Equatorial region: Lambert cylindrical equal-area projection
- Polar caps: Collignon pseudo-cylindrical equal-area projection
- Transition at `theta = arccos(2/3)` (~41.8 deg from pole)

### 2.3 Benefits for Noise Sampling Uniformity

- Equal area guarantees each noise sample covers the same solid angle -- no polar oversampling
- Isolatitude property enables efficient row-by-row processing
- No singularities at poles (unlike lat/lon grids)
- Quadtree hierarchy maps naturally to GPU mipmap-like LOD

### 2.4 GPU Implementation Considerations

- HEALPix projection can be performed entirely within GPU shaders (demonstrated for Mars terrain rendering with MOLA/HRSC datasets)
- Unified DEM and imagery treatment in single shader pass
- LOD via quadtree subdivision with screen-space heuristics
- NESTED ordering scheme maps well to spatial locality on GPU
- Pixel index computation involves integer arithmetic suitable for compute shaders

**Pixel index to (theta, phi) conversion (RING scheme, equatorial zone):**

```glsl
// For pixel index p in equatorial zone
int ring = p / (4 * N_side) + N_side;  // ring index
int phi_idx = p % (4 * N_side);         // position on ring
float theta = acos(1.0 - float(ring * ring) / float(3 * N_side * N_side));
float phi = (float(phi_idx) + 0.5) * PI / (2.0 * float(N_side));
```

### 2.5 Limitations for Real-Time Rendering

- Pixel boundaries are curves, not straight lines -- harder to rasterize than triangles
- No native triangle mesh representation; must triangulate quadrilaterals
- Less ecosystem support in game engines compared to cube maps or icospheres
- Indexing math more complex than simple cube-face UV lookups

---

## 3. Icosphere / Geodesic Grids

### 3.1 Construction

Start with a regular icosahedron (20 equilateral triangles, 12 vertices, 30 edges). Subdivide recursively:

1. For each triangle, insert a new vertex at the midpoint of each edge
2. Project new vertices outward to the unit sphere
3. Connect the 4 resulting sub-triangles

**Vertex count after n subdivisions:**

```
V(n) = 10 * 4^n + 2
```

**Face count:**

```
F(n) = 20 * 4^n
```

**Edge count:**

```
E(n) = 30 * 4^n
```

| Subdivisions | Vertices | Faces | Edges |
|-------------|----------|-------|-------|
| 0 | 12 | 20 | 30 |
| 1 | 42 | 80 | 120 |
| 2 | 162 | 320 | 480 |
| 3 | 642 | 1,280 | 1,920 |
| 4 | 2,562 | 5,120 | 3,840 |
| 5 | 10,242 | 20,480 | 30,720 |
| 6 | 40,962 | 81,920 | 122,880 |

### 3.2 Subdivision Classes (Goldberg Polyhedra Dual)

Three classes of geodesic subdivision:

- **Class I (m,0)**: `T = m^2` -- axial subdivision, uniform edge division
- **Class II (m,m)**: `T = 3m^2` -- triacon subdivision
- **Class III (m,n)**: `T = m^2 + mn + n^2` -- general, potentially chiral if `m != n`

Where `T` is the triangulation number (face multiplier from icosahedron).

The dual of a geodesic polyhedron is a Goldberg polyhedron (hexagons + exactly 12 pentagons, per Euler's formula). Hex grids on spheres use this dual form.

### 3.3 Vertex Distribution Uniformity

- Icosphere has 6 triangles at most vertices, but 12 vertices retain the original icosahedral 5-fold symmetry
- Vertices are slightly denser at the 12 original icosahedron corners
- Overall much more uniform than UV sphere (which collapses at poles)
- **No polar singularities** -- the primary advantage over lat/lon grids

**Quantitative uniformity:**
- For a Class I subdivision, the ratio of maximum to minimum edge length converges to approximately 1.17:1 as subdivision increases
- Compare to UV sphere where the ratio approaches infinity at poles

### 3.4 GPU Implementation

**Terrain displacement on icosphere (vertex shader):**

```glsl
// For each icosphere vertex
vec3 dir = normalize(vertex_position);  // unit sphere direction
float height = fbm(dir * frequency);     // sample 3D noise at sphere position
vec3 displaced = dir * (radius + height * amplitude);
gl_Position = mvp * vec4(displaced, 1.0);
```

**LOD strategies:**
- Adaptive subdivision: split triangles based on screen-space edge length
- CDLOD (Continuous Distance-Dependent LOD) adapted for spherical patches
- Hardware tessellation: use tessellation shaders with hull/domain stages
- Mesh shaders: modern approach for procedural geometry (AMD work_graphs)

**Key consideration:** Icosphere patches are triangular, which complicates texture tiling versus quad-based cube faces. Triangle-based LOD requires careful T-junction elimination at different subdivision levels.

---

## 4. Pole Distortion Fixes

### 4.1 The Problem

Latitude-longitude (equirectangular) parameterization has two catastrophic issues:
- **Pole singularity**: grid lines converge to a point at poles; infinitely many texels map to a single point
- **Seam line**: the 0/360 degree longitude boundary creates a visible discontinuity unless handled

Cube sphere ameliorates both but introduces edge and corner artifacts at cube face boundaries.

### 4.2 Cube Sphere -- Face Edge Seams

**Problem:** Different noise samples or texture lookups on adjacent cube faces may not align perfectly at face edges.

**Fix 1 -- Overlap borders:**
- Extend each face texture by a few pixels beyond the face boundary
- Sample from the overlapping region of the adjacent face
- Blend in a narrow strip at the boundary

**Fix 2 -- Use 3D noise directly:**
- Sample a 3D noise function at the Cartesian coordinates of the sphere surface point
- Bypasses all parameterization entirely -- inherently seamless
- Works for any sphere representation (cube, ico, HEALPix)

```glsl
float seamlessNoise(vec3 spherePos) {
    return snoise(spherePos * noiseScale);
}
```

**Fix 3 -- Pre-distort UV grid:**
- Apply inverse distortion to the cube face grid to counteract normalization distortion
- Results in quads that better preserve area, angles, and side lengths on the sphere

### 4.3 Cube Sphere -- Corner Distortion

At cube corners, three faces meet and the mapping has its highest angular distortion (for gnomonic projection).

**Fix:** Use tangent-adjusted or equal-area mapping (Section 1.3/1.4) to reduce corner distortion from 3.5:1 area ratio down to 1.414:1 or 1.0:1.

### 4.4 Triplanar Mapping for Displacement

Projects textures from three orthogonal directions and blends based on the surface normal:

```glsl
vec3 triplanarNoise(vec3 worldPos, vec3 normal) {
    // Sample noise from three projections
    float nx = snoise(worldPos.yz * scale);
    float ny = snoise(worldPos.xz * scale);
    float nz = snoise(worldPos.xy * scale);

    // Blend weights from surface normal
    vec3 blend = abs(normal);
    blend = pow(blend, vec3(4.0));  // sharpen blending
    blend /= (blend.x + blend.y + blend.z);

    return nx * blend.x + ny * blend.y + nz * blend.z;
}
```

**Pros:** No seams, no UV dependence
**Cons:** 3x texture lookups; blending transitions can be visible on steep surfaces

---

## 5. Seamless Noise on Spheres

### 5.1 Method A: 3D Noise with Cartesian Coordinates

The simplest approach -- evaluate 3D noise at the sphere's Cartesian coordinates.

```glsl
// Convert sphere surface point to noise coordinate
vec3 noiseCoord = normalize(position) * noiseScale;
float n = snoise(noiseCoord);
```

**Pros:** Inherently seamless; no seam or pole artifacts
**Cons:** Noise features may cluster differently depending on noise function's lattice alignment with sphere axes

### 5.2 Method B: 4D Noise for Animated Spheres (Torus Trick)

For creating seamless 2D noise textures that tile on a sphere, map 2D texture coordinates to 4D noise space via the torus embedding:

```
For texture coordinate (s, t) in [0, 1]^2:
    theta = 2*pi*s    (longitude)
    phi   = pi*t      (latitude)

    // Map to 4D torus
    nx = cos(theta)
    ny = sin(theta)
    nz = cos(phi)
    nw = sin(phi)

    noise_value = snoise4D(nx * R, ny * R, nz * R, nw * R)
```

**Why 4D?** A torus in 3D space stretches when unwrapped to a plane. A 4D torus (product of two circles) embeds isometrically -- both axes loop without distortion. The domain loops around in unbroken circles back to the starting point in both U and V, creating truly seamless noise.

**GLSL implementation:**

```glsl
float seamlessSphereNoise(vec2 uv, float scale) {
    float theta = uv.x * 6.2831853;  // 2*pi
    float phi   = uv.y * 3.1415926;  // pi

    vec4 noisePos = vec4(
        cos(theta) * scale,
        sin(theta) * scale,
        cos(phi) * scale,
        sin(phi) * scale
    );

    return snoise(noisePos);  // 4D simplex noise
}
```

### 5.3 Method C: 3D Simplex Noise (Ashima/Gustavson)

The standard GLSL implementation by Ian McEwan and Stefan Gustavson:

```glsl
vec4 permute(vec4 x) { return mod(((x * 34.0) + 1.0) * x, 289.0); }
vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }

float snoise(vec3 v) {
    const vec2 C = vec2(1.0/6.0, 1.0/3.0);
    const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);

    vec3 i  = floor(v + dot(v, C.yyy));
    vec3 x0 = v - i + dot(i, C.xxx);

    vec3 g = step(x0.yzx, x0.xyz);
    vec3 l = 1.0 - g;
    vec3 i1 = min(g.xyz, l.zxy);
    vec3 i2 = max(g.xyz, l.zxy);

    vec3 x1 = x0 - i1 + C.xxx;
    vec3 x2 = x0 - i2 + 2.0 * C.xxx;
    vec3 x3 = x0 - 1.0 + 3.0 * C.xxx;

    i = mod(i, 289.0);
    vec4 p = permute(permute(permute(
        i.z + vec4(0.0, i1.z, i2.z, 1.0)) +
        i.y + vec4(0.0, i1.y, i2.y, 1.0)) +
        i.x + vec4(0.0, i1.x, i2.x, 1.0));

    float n_ = 0.142857142857;
    vec3 ns = n_ * D.wyz - D.xzx;
    vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
    vec4 x_ = floor(j * ns.z);
    vec4 y_ = floor(j - 7.0 * x_);
    vec4 x = x_ * ns.x + ns.yyyy;
    vec4 y = y_ * ns.x + ns.yyyy;
    vec4 h = 1.0 - abs(x) - abs(y);
    vec4 b0 = vec4(x.xy, y.xy);
    vec4 b1 = vec4(x.zw, y.zw);
    vec4 s0 = floor(b0) * 2.0 + 1.0;
    vec4 s1 = floor(b1) * 2.0 + 1.0;
    vec4 sh = -step(h, vec4(0.0));
    vec4 a0 = b0.xzyw + s0.xzyw * sh.xxyy;
    vec4 a1 = b1.xzyw + s1.xzyw * sh.zzww;
    vec3 p0 = vec3(a0.xy, h.x);
    vec3 p1 = vec3(a0.zw, h.y);
    vec3 p2 = vec3(a1.xy, h.z);
    vec3 p3 = vec3(a1.zw, h.w);
    vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
    p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
    vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
    m = m * m;
    return 42.0 * dot(m * m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
}
```

### 5.4 4D Simplex Noise (for torus trick and animation)

```glsl
// Key constants for 4D simplex
vec4 permute(vec4 x) { return mod(((x * 34.0) + 1.0) * x, 289.0); }
float permute(float x) { return floor(mod(((x * 34.0) + 1.0) * x, 289.0)); }
vec4 taylorInvSqrt(vec4 r) { return 1.79284291400159 - 0.85373472095314 * r; }

float snoise(vec4 v) {
    const vec2 C = vec2(0.138196601125010504,  // (5 - sqrt(5)) / 20
                        0.309016994374947451);  // (sqrt(5) - 1) / 4
    vec4 i  = floor(v + dot(v, C.yyyy));
    vec4 x0 = v - i + dot(i, C.xxxx);

    // Rank sorting for simplex corner offsets
    // (implementation at github.com/stegu/webgl-noise)
    // ...
    return 49.0 * (dot(m0*m0, vec3(/*...*/)) + dot(m1*m1, vec2(/*...*/)));
}
```

### 5.5 Fractal Brownian Motion on Spheres

Layer multiple octaves of noise for terrain:

```glsl
float fbm(vec3 pos, int octaves, float lacunarity, float gain) {
    float amplitude = 1.0;
    float frequency = 1.0;
    float value = 0.0;
    float maxValue = 0.0;

    for (int i = 0; i < octaves; i++) {
        value += amplitude * snoise(pos * frequency);
        maxValue += amplitude;
        amplitude *= gain;       // typically 0.5
        frequency *= lacunarity; // typically 2.0
    }

    return value / maxValue;  // normalize to [-1, 1]
}

// Apply to sphere
float terrainHeight(vec3 sphereDir) {
    return fbm(sphereDir * baseScale, 8, 2.0, 0.5);
}
```

### 5.6 Hybrid Multifractal (Musgrave)

Creates more realistic terrain with sharp ridges and smooth valleys:

```glsl
float hybridMultifractal(vec3 pos, float H, float lacunarity, int octaves, float offset) {
    float frequency = 1.0;
    float weight = 1.0;
    float signal = (snoise(pos) + offset) * pow(frequency, -H);
    float result = signal;

    for (int i = 1; i < octaves; i++) {
        frequency *= lacunarity;
        weight = clamp(signal * 2.0, 0.0, 1.0);
        signal = (snoise(pos * frequency) + offset) * pow(frequency, -H);
        signal *= weight;
        result += signal;
    }
    return result;
}
```

### 5.7 Gradient Rotation for Animated Noise

For 2D noise, gradients can be rotated by a varying angle for flow effects:

```glsl
// Rotate gradient by angle a
mat2 rot(float a) {
    float c = cos(a), s = sin(a);
    return mat2(c, -s, s, c);
}

// For 3D: rotate gradient around pseudo-random axis
// Comes cheaply in implementation -- see Gustavson 2011
```

### 5.8 GPU Performance Notes (from GPU Gems 2, Ch. 26)

Improved Perlin noise on GPU:
- Permutation table stored as 256x1 L8 texture
- 16 gradient vectors precomputed in a 1D texture
- Interpolation uses C2 continuous Hermite polynomial: `t * t * t * (t * (t * 6 - 15) + 10)`
- Unoptimized: 22 texture lookups, 81 instructions
- Optimized (pre-permuted 256x256 RGBA texture): **9 texture lookups, 53 instructions**

### 5.9 Comparison of Noise Approaches for Spheres

| Method | Seamless? | Dimensions | GPU Cost | Distortion | Best For |
|--------|-----------|------------|----------|------------|----------|
| 3D noise at xyz | Yes | 3D | Low | None | Direct vertex displacement |
| 4D torus trick | Yes | 4D | Medium | None | Equirect texture generation |
| Lat/lon + 2D noise | No (poles) | 2D | Lowest | Severe at poles | Quick prototypes only |
| Cube face 2D + blending | Mostly | 2D | Low | At face edges | Cubemap terrain |
| Triplanar | Yes | 3x 2D | 3x cost | Blend transitions | Detail texturing |

---

## 6. UV Unwrapping for Planet Textures

### 6.1 Equirectangular (Plate Carree)

The standard mapping: longitude to U, latitude to V.

```
u = (longitude + pi) / (2 * pi)
v = (latitude + pi/2) / pi
```

**Distortion:** Area distortion ratio = 1/cos(latitude), approaching infinity at poles. Pixels at equator represent ~111 km; at 80 deg latitude they represent ~19 km but cover the same texel area.

### 6.2 Cube Map Unwrapping

Six square faces, each with independent UV space.

**Pros:** Native GPU support for cubemap textures; equal treatment of all directions; well-understood LOD
**Cons:** Seams at face edges require filtering; face corner distortion (mitigated by tangent/equal-area mapping)

### 6.3 Octahedral Mapping

Maps the unit sphere to an octahedron, which unfolds into a square.

**Encoding (3D to 2D):**

```glsl
vec2 octEncode(vec3 n) {
    // Project onto octahedron
    n /= (abs(n.x) + abs(n.y) + abs(n.z));

    // Unfold bottom hemisphere
    if (n.z < 0.0) {
        n.xy = (1.0 - abs(n.yx)) * sign(n.xy);
    }

    return n.xy * 0.5 + 0.5;  // remap to [0,1]
}
```

**Decoding (2D to 3D):**

```glsl
vec3 octDecode(vec2 f) {
    f = f * 2.0 - 1.0;

    vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));

    float t = max(-n.z, 0.0);
    n.xy -= sign(n.xy) * t;

    return normalize(n);
}
```

**Distortion analysis:**
- Near-uniform mapping across most of the sphere
- Discontinuity along the "fold" edges of the lower hemisphere
- Max error: 0.04 deg with 24-bit precision
- 19 VALU ops for encoding (vs 10 for cubemap lookup)

**Performance comparison:**

| Property | Cubemap | Octahedral |
|----------|---------|------------|
| Encoding cost | 10 VALU ops | 19 VALU ops |
| Neighbor sampling | Complex (face transitions) | Simple 2D offsets (8 VALU) |
| Texture storage | 6 square faces | 1 square |
| GPU hardware support | Native `textureCube` | Standard 2D texture |
| Filtering | Hardware trilinear | Manual at edges |

### 6.4 Multi-Face Approaches

**Virtual texturing / megatextures:**
- Each cube/ico face gets its own texture tile
- Stream tiles on demand based on camera proximity
- Clipmap or virtual texture page table on GPU
- Eliminates need for single unwrapped texture

**Adaptive UV charts:**
- Split sphere into charts using LSCM (Least Squares Conformal Mapping) or ABF (Angle-Based Flattening)
- Minimize distortion within each chart
- Charts stored in texture atlas
- Used in offline rendering; too complex for procedural real-time

### 6.5 Fibonacci Sphere (Bonus Alternative)

Distributes points in a Fibonacci spiral down the sphere -- no UV mapping per se, but excellent point distribution for vertex-based terrain.

**Formula:**

```
For i = 0 to N-1:
    theta_i = 2*pi*i / phi          (golden angle increments)
    phi_i   = acos(1 - 2*(i + eps) / (N - 1 + 2*eps))

    x_i = cos(theta_i) * sin(phi_i)
    y_i = sin(theta_i) * sin(phi_i)
    z_i = cos(phi_i)
```

Where `phi = (1 + sqrt(5)) / 2` (golden ratio) and `eps ~= 0.36` for the offset Fibonacci lattice.

**Uniformity metric:** Normalized minimum nearest-neighbor distance `delta* ~= 3.35` (vs 3.09 for canonical, ~8.3% improvement).

**Limitations:** No regular grid structure; hard to texture; requires Delaunay triangulation for mesh; no natural LOD hierarchy.

---

## 7. Comparative Analysis and Recommendations

### 7.1 For Real-Time Planet Rendering (Games)

**Recommended: Cube sphere with tangent-adjusted mapping**
- Best balance of distortion (1.414:1) vs implementation simplicity
- Native GPU cubemap support for texturing
- Easy LOD via quadtree on each face
- Compatible with all standard noise functions via 3D sampling
- Google S2 library validates this at production scale

### 7.2 For Scientific Visualization

**Recommended: HEALPix or Equal-Area cube sphere**
- HEALPix if multi-resolution data fusion is needed (MOLA, satellite imagery)
- Equal-area cube sphere if simpler implementation is preferred
- Perfect area preservation critical for physical simulation

### 7.3 For Maximum Uniformity

**Recommended: Icosphere (geodesic grid)**
- Most uniform vertex distribution of mesh-based approaches
- Triangle patches handle LOD well with tessellation shaders
- 1.17:1 max edge length ratio (best practical uniformity)
- Consider Goldberg dual (hex grid) for simulation grids

### 7.4 For Procedural Noise Terrain

**Recommended: Any sphere representation + 3D simplex noise**
- Decouples noise from parameterization entirely
- Use FBM or hybrid multifractal for natural terrain
- Add 4D noise for temporal animation
- Triplanar mapping for detail textures

### 7.5 Overall Architecture Suggestion

```
                 +-----------------+
                 | Cube Sphere     |
                 | (tangent-adj.)  |
                 +--------+--------+
                          |
              +-----------+-----------+
              |                       |
     +--------v--------+    +--------v--------+
     | 3D Simplex Noise |    | Texture Atlas   |
     | (FBM + Hybrid    |    | (per-face tiles) |
     | Multifractal)    |    | virtual texturing|
     +---------+--------+    +--------+--------+
               |                       |
     +---------v-----------------------v--------+
     |          Vertex Displacement              |
     |  height = fbm(normalize(pos) * scale)     |
     |  + detail via triplanar mapping            |
     +-------------------------------------------+
               |
     +---------v-----------+
     | LOD: Quadtree per   |
     | face + tessellation  |
     | shaders              |
     +-----------------------+
```

---

## Sources

### Cube-to-Sphere Mapping
- [Cube Sphere -- Catlike Coding](https://catlikecoding.com/unity/tutorials/procedural-meshes/cube-sphere/)
- [Cube-to-sphere Projections for Procedural Texturing (JCGT 2018)](https://www.jcgt.org/published/0007/02/01/paper.pdf)
- [Making Worlds 1 -- Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/)
- [Wraparound Square Tile Maps on a Sphere -- Red Blob Games](https://www.redblobgames.com/x/1938-square-tiling-of-sphere/)
- [Survey of Cube Mapping Methods (Springer 2019)](https://link.springer.com/article/10.1007/s00371-019-01708-4)
- [Comparison of Spherical Cube Map Projections (Dimitrijevic & Lambers)](https://www.researchgate.net/publication/304777429_Comparison_Of_Spherical_Cube_Map_Projections_Used_In_Planet-Sized_Terrain_Rendering)
- [Uniform Spherical Grids via Equal Area Projection (Plonka)](https://num.math.uni-goettingen.de/plonka/pdfs/cubsphere3.pdf)
- [Quadrilateralized Spherical Cube -- Wikipedia](https://en.wikipedia.org/wiki/Quadrilateralized_spherical_cube)
- [QSC Projection -- PROJ Documentation](https://proj.org/en/stable/operations/projections/qsc.html)

### HEALPix
- [HEALPix Official Site](https://healpix.sourceforge.io/)
- [HEALPix -- Wikipedia](https://en.wikipedia.org/wiki/HEALPix)
- [HEALPix: A Framework for High-Resolution Analysis (Gorski 2005)](https://ui.adsabs.harvard.edu/abs/2005ApJ...622..759G/abstract)
- [Spherical Terrain Rendering using HEALPix Grid (Dagstuhl 2011)](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.VLUDS.2011.13)
- [HEALPix Primer -- JPL](https://healpix.jpl.nasa.gov/pdf/intro.pdf)
- [healpy Pixel Functions Documentation](https://healpy.readthedocs.io/en/latest/healpy_pix.html)

### Icosphere / Geodesic Grids
- [Procedural Planet Generation -- Nick Chavez](https://nicolaschavez.com/projects/procterrain/)
- [Geodesic Polyhedron -- Wikipedia](https://en.wikipedia.org/wiki/Geodesic_polyhedron)
- [Goldberg Polyhedron -- Wikipedia](https://en.wikipedia.org/wiki/Goldberg_polyhedron)
- [Geodesic and Goldberg Polyhedra Math -- BabylonJS](https://doc.babylonjs.com/guidedLearning/workshop/Geodesic_Math)
- [IcosphereGrid -- Unreal Engine 5 (GitHub)](https://github.com/AntonHedlund/IcosphereGrid)
- [Terrain LOD on Spherical Grids -- vterrain.org](http://vterrain.org/LOD/spherical.html)

### Seamless Noise
- [GLSL Noise Algorithms (Gonzalez Vivo)](https://gist.github.com/patriciogonzalezvivo/670c22f3966e662d2f83)
- [WebGL Noise Library (Gustavson)](https://stegu.github.io/webgl-noise/webdemo/)
- [Improved Perlin Noise -- GPU Gems 2, Ch. 26](https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-26-implementing-improved-perlin-noise)
- [Tiling Simplex Noise and Flow Noise (JCGT 2022)](https://jcgt.org/published/0011/01/02/paper-lowres.pdf)
- [libnoise Tutorial 8: Spherical Planetary Terrain](https://libnoise.sourceforge.net/tutorials/tutorial8.html)
- [Procedural Planet Rendering -- Jad Khoury](https://jadkhoury.github.io/terrain_blog.html)
- [Seamless Noise -- GameDev.net](https://www.gamedev.net/blog/33/entry-2138456-seamless-noise/)

### UV Mapping / Octahedral
- [Octahedron Normal Vector Encoding (Narkowicz)](https://knarkowicz.wordpress.com/2014/04/16/octahedron-normal-vector-encoding/)
- [Fetching from Cubes and Octahedrons -- AMD GPUOpen](https://gpuopen.com/learn/fetching-from-cubes-and-octahedrons/)
- [Octahedral Encoded Normals Analysis (Liam Tyler)](https://liamtyler.github.io/posts/octahedral_analysis/)
- [Square Equal-Area Map Projection with Low Angular Distortion (ACM 2021)](https://dl.acm.org/doi/fullHtml/10.1145/3460521)

### Fibonacci Sphere
- [Evenly Distributing Points on a Sphere -- Extreme Learning](https://extremelearning.com.au/how-to-evenly-distribute-points-on-a-sphere-more-effectively-than-the-canonical-fibonacci-lattice/)
- [Fibonacci Lattice -- John D. Cook](https://www.johndcook.com/blog/2023/08/12/fibonacci-lattice/)

### General GPU Terrain
- [GPU Gems 3 Ch.1: Complex Procedural Terrains on GPU](https://developer.nvidia.com/gpugems/gpugems3/part-i-geometry/chapter-1-generating-complex-procedural-terrains-using-gpu)
- [Procedural Generation with Work Graphs -- AMD GPUOpen](https://gpuopen.com/learn/work_graphs_mesh_nodes/work_graphs_mesh_nodes-procedural_generation/)

### Additional Sources (second pass)
- [S-NEW1] [Comparison of Spherical Cube Map Projections (Dimitrijevic & Lambers, DocsLib)](https://docslib.org/doc/860041/comparison-of-spherical-cube-map-projections-used-in-planet-sized-terrain-rendering) -- Evaluates gnomonic, adjusted, QSC, Outerra and HEALPix projections for planet rendering
- [S-NEW2] [Distortion Optimized Spherical Cube Mapping for DGGS (ResearchGate)](https://www.researchgate.net/publication/354495398_Distortion_Optimized_Spherical_Cube_Mapping_for_Discrete_Global_Grid_Systems) -- Optimal cube orientation for minimal distortion
- [S-NEW3] [Planet LOD Research -- Leah Lindner](https://leah-lindner.com/blog/2016/10/10/planetrenderer_week1/) -- Icosphere vs cube sphere for planet LOD rendering
- [S-NEW4] [Making Worlds 1: Of Spheres and Cubes -- Acko.net](https://acko.net/blog/making-worlds-1-of-spheres-and-cubes/) -- Chunked quadtree LOD on cube sphere faces
- [S-NEW5] [Procedural Planetary Surfaces -- Toni Sagrista](https://tonisagrista.com/blog/2021/procedural-planetary-surfaces/) -- 3D noise sampling on sphere with spherical-to-Cartesian conversion
- [S-NEW6] [PEAR: Equal Area Weather Forecasting on HEALPix (arXiv)](https://arxiv.org/html/2505.17720) -- ML weather model operating natively on HEALPix grid
- [S-NEW7] [HEALPix for hierarchical climate data -- nextGEMS](https://nextgems-h2020.eu/of-hierarchies-chunking-and-healpix/) -- HEALPix for high-resolution climate simulation output
- [S-NEW8] [Fixing Spheres and Planets -- GameDev.net](https://www.gamedev.net/blogs/entry/2269506-fixing-spheres-and-planets/) -- Area-distributing cube-sphere technique for game dev
- [S-NEW9] [Icosphere Generation Improvement -- Alexis Giard](https://www.alexisgiard.com/icosahedron-sphere-remastered/) -- Icosphere construction and subdivision optimization
