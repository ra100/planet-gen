use planet_gen::{
    export::{
        ExportConfig, ExportLayers, MAP_BATCH_SIZE, MAX_OWNED_LIVE_BYTES, TILE_SIZE,
        TileCoordinator, crop_region_bytes, emission_stencil_radius, erosion_resolution_for_export,
        estimated_export_preflight_bytes_with_erosion, estimated_peak_streaming_bytes,
        map_batch_fits, map_tile_fits_device, map_tile_owned_bytes, max_map_stencil_radius,
        reconstruct_8k_from_meso_delta, run_export, select_tile_size,
    },
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    terrain_compute::{TectonicTerrain, TerrainGenerationParams},
    weather::WeatherSnapshot,
};
use std::sync::{Arc, atomic::AtomicBool, mpsc::channel};

fn face_bytes(face: u8, resolution: u32) -> Vec<u8> {
    (0..resolution * resolution)
        .map(|index| face.wrapping_mul(32).wrapping_add(index as u8))
        .collect()
}

fn region_bytes(full: &[u8], resolution: u32, region: planet_gen::export::TileRegion) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((region.width * region.height) as usize);
    for y in region.origin_y..region.origin_y + region.height {
        let start = (y * resolution + region.origin_x) as usize;
        bytes.extend_from_slice(&full[start..start + region.width as usize]);
    }
    bytes
}

fn core_bytes(full: &[u8], resolution: u32, region: planet_gen::export::TileRegion) -> Vec<u8> {
    let mut bytes = Vec::with_capacity((region.crop_width * region.crop_height) as usize);
    for y in 0..region.crop_height {
        let start = ((region.origin_y + region.crop_y + y) * resolution
            + region.origin_x
            + region.crop_x) as usize;
        bytes.extend_from_slice(&full[start..start + region.crop_width as usize]);
    }
    bytes
}

fn terrain_params(seed: u32) -> TerrainGenerationParams {
    TerrainGenerationParams {
        seed,
        amplitude: 1.0,
        frequency: 1.2,
        octaves: 8,
        gain: 0.5,
        lacunarity: 2.0,
        mountain_scale: 1.0,
        boundary_width: 0.10,
        warp_strength: 1.0,
        detail_scale: 1.0,
        surface_gravity: 9.81,
        tectonics_factor: 0.85,
        surface_age: 0.2,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    }
}

#[test]
fn streamed_tile_halo_matches_monolithic_reference() {
    let coordinator = TileCoordinator::new(32, 8);
    let full = face_bytes(0, 32);

    for tile_y in 0..coordinator.tiles_per_axis {
        for tile_x in 0..coordinator.tiles_per_axis {
            let region = coordinator.region(tile_x, tile_y);
            assert_eq!(
                crop_region_bytes(region, &region_bytes(&full, 32, region), 1),
                core_bytes(&full, 32, region)
            );
        }
    }
}

#[test]
fn streamed_halo_crop_matches_adjacent_tiles_on_each_face() {
    let coordinator = TileCoordinator::new(32, 8);
    for face in 0..6 {
        let full = face_bytes(face, 32);
        for (left, right) in [
            (coordinator.region(0, 1), coordinator.region(1, 1)),
            (coordinator.region(1, 0), coordinator.region(1, 1)),
        ] {
            let left_crop = crop_region_bytes(left, &region_bytes(&full, 32, left), 1);
            let right_crop = crop_region_bytes(right, &region_bytes(&full, 32, right), 1);
            assert_eq!(left_crop, core_bytes(&full, 32, left));
            assert_eq!(right_crop, core_bytes(&full, 32, right));
        }
    }
}

#[test]
fn halo_is_derived_from_the_largest_map_stencil() {
    let region = TileCoordinator::new(64, 16).region(1, 1);
    let halo = max_map_stencil_radius(64);
    assert_eq!(region.crop_x, halo);
    assert_eq!(region.crop_y, halo);
    assert_eq!(region.width, 16 + 2 * halo);
    assert_eq!(region.height, 16 + 2 * halo);
    assert_eq!(emission_stencil_radius(8192), 81);
}

#[test]
fn limit_constrained_tiles_keep_the_fixed_halo() {
    let mut limits = wgpu::Limits::default();
    limits.max_buffer_size = 134_217_728;
    limits.max_storage_buffer_binding_size = 134_217_728;
    let tile_size = select_tile_size(8192, 2048, &limits, 16).unwrap();
    assert_eq!(tile_size, 1024);
    let region = TileCoordinator::new(8192, tile_size).region(1, 1);
    assert_eq!(region.crop_x, max_map_stencil_radius(8192));
    assert_eq!(region.crop_y, max_map_stencil_radius(8192));
}

#[test]
fn default_tile_size_is_1024_and_device_fallback_remains_available() {
    assert_eq!(TILE_SIZE, 1024);
    assert_eq!(
        select_tile_size(2048, TILE_SIZE, &wgpu::Limits::default(), 16).unwrap(),
        TILE_SIZE
    );

    let mut limits = wgpu::Limits::default();
    limits.max_buffer_size = 32_000_000;
    limits.max_storage_buffer_binding_size = 32_000_000;
    assert_eq!(select_tile_size(2048, TILE_SIZE, &limits, 16).unwrap(), 512);
}

#[test]
fn current_8k_streaming_ledger_fails_before_illegal_allocation() {
    assert!(estimated_peak_streaming_bytes(8192, 16) > MAX_OWNED_LIVE_BYTES);
}

#[test]
fn eight_k_export_selects_2048_meso_erosion() {
    assert_eq!(erosion_resolution_for_export(8192), 2048);
    assert_eq!(erosion_resolution_for_export(4096), 4096);
}

#[test]
fn eight_k_meso_erosion_preflight_stays_within_owned_live_budget() {
    let layers = ExportLayers {
        emission: false,
        ..ExportLayers::default()
    };
    let bytes =
        estimated_export_preflight_bytes_with_erosion(8192, &layers, 25, &wgpu::Limits::default())
            .unwrap();
    assert!(bytes <= MAX_OWNED_LIVE_BYTES);
}

#[test]
fn meso_reconstruction_is_deterministic_and_preserves_full_dimensions() {
    let full = TectonicTerrain {
        faces: std::array::from_fn(|face| vec![face as f32; 64]),
        resolution: 8,
    };
    let meso = TectonicTerrain {
        faces: std::array::from_fn(|face| vec![face as f32 * 2.0; 4]),
        resolution: 2,
    };
    let eroded = TectonicTerrain {
        faces: std::array::from_fn(|face| vec![face as f32 * 2.0 - 0.25; 4]),
        resolution: 2,
    };
    let first = reconstruct_8k_from_meso_delta(&full, &meso, &eroded).unwrap();
    let second = reconstruct_8k_from_meso_delta(&full, &meso, &eroded).unwrap();
    assert_eq!(first.resolution, 8);
    assert_eq!(first.faces, second.faces);
    assert_eq!(first.faces[0].len(), 64);
}

#[test]
fn map_batch_foundation_is_memory_bounded() {
    assert!((8..=16).contains(&MAP_BATCH_SIZE));
}

#[test]
fn map_batch_rejects_resource_cap_before_push() {
    assert!(map_batch_fits(64, 64, 128));
    assert!(!map_batch_fits(64, 65, 128));
    assert!(!map_batch_fits(u64::MAX, 1, u64::MAX));
}

#[test]
fn map_tile_preflight_rejects_overflow_and_device_limit_before_allocation() {
    assert!(map_tile_owned_bytes(u64::MAX, 1, 1, 1, 1).is_err());
    let mut limits = wgpu::Limits::default();
    limits.max_buffer_size = 64;
    limits.max_storage_buffer_binding_size = 64;
    assert!(map_tile_fits_device(65, 1, &limits).is_err());
    assert!(map_tile_fits_device(64, 64, &limits).is_ok());
}

#[test]
fn streamed_layers_release_previous_face_buffers() {
    let gpu = Arc::new(GpuContext::new().expect("GPU init failed"));
    let root = std::env::temp_dir().join(format!("planet-gen-streaming-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let config = ExportConfig {
        face_resolution: 32,
        tile_size: 16,
        output_dir: root.clone(),
        planet_name: "streaming".into(),
        erosion_iterations: 0,
        layers: ExportLayers::default(),
        weather: WeatherSnapshot {
            seed: params.seed,
            ..WeatherSnapshot::default()
        },
        night_lights: 0.0,
    };
    let (progress, _) = channel();
    let result = run_export(
        &gpu,
        &config,
        &params,
        &derived,
        1.0,
        0.0,
        terrain_params(params.seed),
        &progress,
        &AtomicBool::new(false),
    );
    let output = result.expect("streamed export failed");
    for name in [
        "height.exr",
        "normal.exr",
        "roughness.png",
        "albedo.png",
        "ao.png",
        "water_mask.png",
        "clouds.exr",
        "emission.exr",
    ] {
        assert!(output.join(name).is_file(), "missing {name}");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn tile_local_map_inputs_match_monolithic_heightmap() {
    let resolution = 32;
    let coordinator = TileCoordinator::new(resolution, 8);
    let heightmap = face_bytes(0, resolution);
    for tile_y in 0..coordinator.tiles_per_axis {
        for tile_x in 0..coordinator.tiles_per_axis {
            let region = coordinator.region(tile_x, tile_y);
            let cropped =
                crop_region_bytes(region, &region_bytes(&heightmap, resolution, region), 1);
            assert_eq!(cropped, core_bytes(&heightmap, resolution, region));
        }
    }
}

#[test]
fn invalid_tile_sizes_are_export_errors() {
    let gpu = Arc::new(GpuContext::new().expect("GPU init failed"));
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    for tile_size in [0, 12] {
        let config = ExportConfig {
            face_resolution: 32,
            tile_size,
            output_dir: std::env::temp_dir(),
            planet_name: "invalid-tile".into(),
            erosion_iterations: 0,
            layers: ExportLayers::default(),
            weather: WeatherSnapshot {
                seed: params.seed,
                ..WeatherSnapshot::default()
            },
            night_lights: 0.0,
        };
        let (progress, _) = channel();
        assert!(
            run_export(
                &gpu,
                &config,
                &params,
                &derived,
                1.0,
                0.0,
                terrain_params(params.seed),
                &progress,
                &AtomicBool::new(false),
            )
            .is_err()
        );
    }
}
