// Linear reflectance, shared by the globe and texture export. All spatial
// fields live on the sphere so region boundaries cannot follow cube faces.
fn material_ramp(a: f32, b: f32, x: f32) -> f32 {
    let t = clamp((x - a) / (b - a), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

struct SurfaceProvince {
    sand: vec3<f32>,
    soil: vec3<f32>,
    rock: vec3<f32>,
    variation: f32,
    grain: f32,
}

fn surface_province(p: vec3<f32>, seed: u32) -> SurfaceProvince {
    let offset = noise_seed_offset(seed, 71u);
    let warp = vec3<f32>(
        snoise(p * 2.1 + offset),
        snoise(p * 2.1 + offset + vec3<f32>(17.0, 0.0, 0.0)),
        snoise(p * 2.1 + offset + vec3<f32>(0.0, 29.0, 0.0)),
    );
    let province = p * 1.8 + warp * 0.24 + offset;
    let iron = material_ramp(-0.04, 0.50, snoise(province));
    let pale = material_ramp(-0.20, 0.40, snoise(province + vec3<f32>(31.0, 7.0, 11.0)));
    let mafic = material_ramp(0.30, 0.68, snoise(province + vec3<f32>(5.0, 43.0, 2.0)));
    var material: SurfaceProvince;
    material.sand = mix(vec3<f32>(0.50, 0.36, 0.20), vec3<f32>(0.38, 0.15, 0.065), iron);
    material.sand = mix(material.sand, vec3<f32>(0.76, 0.70, 0.56), pale);
    material.sand = mix(material.sand, vec3<f32>(0.095, 0.084, 0.071), mafic * 0.7);
    material.soil = mix(vec3<f32>(0.19, 0.135, 0.075), vec3<f32>(0.25, 0.087, 0.036), iron);
    material.soil = mix(material.soil, vec3<f32>(0.35, 0.30, 0.21), pale * 0.65);
    material.rock = mix(vec3<f32>(0.19, 0.18, 0.155), vec3<f32>(0.29, 0.17, 0.105), iron * 0.6);
    material.rock = mix(material.rock, vec3<f32>(0.43, 0.42, 0.37), pale);
    material.rock = mix(material.rock, vec3<f32>(0.085, 0.09, 0.088), mafic);
    material.variation = snoise(p * 19.0 + warp * 1.6 + offset);
    material.grain = snoise(p * 83.0 + offset) * 0.65 + snoise(p * 211.0 + offset) * 0.35;
    return material;
}

// A local incision proxy, not a catchment simulation. Only existing terrain
// troughs can receive wet channels; exposed slopes and dry basins cannot.
fn surface_drainage(land_height: f32, temp: f32, moisture: f32, valley: f32) -> f32 {
    let runoff = material_ramp(45.0, 130.0, moisture) * material_ramp(0.0, 8.0, temp);
    let lowland = 1.0 - material_ramp(0.06, 0.30, land_height);
    return material_ramp(0.25, 0.85, valley) * runoff * lowland;
}

fn surface_land_albedo(
    p: vec3<f32>, seed: u32, mean_temp: f32, moisture: f32,
    seasonal_temp: f32, land_height: f32, slope: f32, valley: f32,
    ocean_fraction: f32,
) -> vec3<f32> {
    let province = surface_province(p, seed);
    let water_supply = material_ramp(0.02, 0.30, ocean_fraction);
    let wetness = max(moisture * (1.0 + province.variation * 0.12), 0.0);
    let warmth = material_ramp(-12.0, 7.0, mean_temp);
    let tropical = material_ramp(16.0, 28.0, mean_temp);
    let growth = material_ramp(18.0, 85.0, wetness) * warmth * water_supply;
    let canopy = material_ramp(55.0, 145.0, wetness) * material_ramp(-3.0, 13.0, mean_temp);
    let bare_sand = material_ramp(45.0, 10.0, wetness) * material_ramp(0.0, 20.0, mean_temp);
    var substrate = mix(province.soil, province.sand, bare_sand);
    substrate *= mix(1.0, 0.64, material_ramp(40.0, 170.0, wetness));
    let dry_grass = mix(vec3<f32>(0.23, 0.205, 0.085), vec3<f32>(0.29, 0.245, 0.105), tropical);
    let meadow = mix(vec3<f32>(0.105, 0.14, 0.065), vec3<f32>(0.085, 0.135, 0.044), tropical);
    let grass = mix(dry_grass, meadow, material_ramp(30.0, 100.0, wetness));
    let conifer = vec3<f32>(0.029, 0.058, 0.038);
    let deciduous = vec3<f32>(0.043, 0.091, 0.035);
    let rainforest = vec3<f32>(0.023, 0.062, 0.030);
    var forest = mix(conifer, deciduous, material_ramp(0.0, 14.0, mean_temp));
    forest = mix(forest, rainforest, tropical);
    forest *= 1.0 + province.variation * 0.18 + province.grain * 0.06;
    var vegetation = mix(grass, forest, canopy);
    let winter = material_ramp(0.0, 18.0, mean_temp - seasonal_temp);
    vegetation = mix(vegetation, vegetation * vec3<f32>(1.20, 0.89, 0.70), winter * (1.0 - tropical) * 0.45);
    var color = mix(substrate, vegetation, growth);
    let tundra = mix(vec3<f32>(0.19, 0.185, 0.12), vec3<f32>(0.10, 0.125, 0.084), material_ramp(25.0, 100.0, wetness));
    color = mix(tundra, color, warmth);
    color = mix(substrate, color, water_supply);

    // The treeline follows temperature; rock exposure also needs relief. This
    // leaves cold flat lowlands as tundra instead of painting them as mountains.
    let highland = material_ramp(0.10, 0.48, land_height);
    let treeless = 1.0 - material_ramp(-5.0, 10.0, mean_temp + province.variation * 2.0);
    let steep = material_ramp(0.8, 3.5, slope);
    let exposure = clamp(highland * treeless * 0.9 + steep * highland * 0.6, 0.0, 1.0);
    let rock = province.rock * (1.0 + province.variation * 0.13 + province.grain * 0.09);
    color = mix(color, rock, exposure);

    let drainage = surface_drainage(land_height, seasonal_temp, wetness, valley) * water_supply;
    let riparian = mix(vec3<f32>(0.055, 0.085, 0.033), forest, canopy);
    color = mix(color, riparian, drainage * 0.65);
    let channel = material_ramp(0.68, 0.97, drainage);
    let sediment = mix(vec3<f32>(0.018, 0.045, 0.041), province.soil * 0.40, material_ramp(120.0, 260.0, wetness));
    color = mix(color, sediment, channel);

    let beach = (1.0 - material_ramp(0.0, 0.004, land_height))
        * (1.0 - material_ramp(0.35, 1.6, slope)) * material_ramp(-3.0, 10.0, seasonal_temp);
    let damp_sand = mix(province.sand * 0.52, province.sand, material_ramp(0.0, 0.0025, land_height));
    color = mix(color, damp_sand, beach * water_supply);
    return clamp(color * (1.0 + province.grain * 0.035), vec3<f32>(0.005), vec3<f32>(0.85));
}

fn surface_ocean_albedo(p: vec3<f32>, seed: u32, depth: f32, temp: f32) -> vec3<f32> {
    let province = surface_province(p, seed);
    let warm = material_ramp(8.0, 25.0, temp);
    let shallow_cold = vec3<f32>(0.027, 0.075, 0.09);
    let shallow_warm = vec3<f32>(0.035, 0.19, 0.17);
    let shelf = mix(shallow_cold, shallow_warm, warm);
    let bottom = mix(shelf, province.sand * vec3<f32>(0.12, 0.30, 0.28), 0.30);
    let coastal = mix(bottom, vec3<f32>(0.009, 0.047, 0.095), material_ramp(0.002, 0.038, depth));
    // Only shallow water reveals the seabed. Abyssal relief must not read as
    // blue terrain; open-ocean reflectance saturates quickly with depth.
    return mix(coastal, vec3<f32>(0.006, 0.020, 0.052), material_ramp(0.035, 0.15, depth));
}
