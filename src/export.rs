use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;
use crate::planet::{DerivedProperties, PlanetParams};
use crate::plates::{generate_plates, PlateGenParams};
use crate::terrain_compute::{ErosionPipeline, TerrainComputePipeline, TerrainGenParams, TectonicTerrain};

// ============ Constants ============

pub const DEFAULT_EXPORT_RESOLUTION: u32 = 4096;
pub const TILE_SIZE: u32 = 512;

// ============ Config ============

pub struct ExportConfig {
    pub face_resolution: u32,
    pub tile_size: u32,
    pub output_dir: PathBuf,
    pub planet_name: String,
    pub erosion_iterations: u32,
    pub season: f32,
}

// ============ Progress ============

#[derive(Clone, Debug)]
pub enum ExportProgress {
    Progress { message: String, fraction: f32 },
    Complete,
    Error(String),
}

// ============ Tile Coordinator ============

pub struct TileCoordinator {
    pub face_resolution: u32,
    pub tile_size: u32,
    pub tiles_per_axis: u32,
}

impl TileCoordinator {
    pub fn new(face_resolution: u32, tile_size: u32) -> Self {
        assert_eq!(
            face_resolution % tile_size,
            0,
            "face_resolution must be a multiple of tile_size"
        );
        Self {
            face_resolution,
            tile_size,
            tiles_per_axis: face_resolution / tile_size,
        }
    }

    pub fn total_tiles(&self) -> u32 {
        6 * self.tiles_per_axis * self.tiles_per_axis
    }

    pub fn tiles_per_face(&self) -> u32 {
        self.tiles_per_axis * self.tiles_per_axis
    }
}

// ============ Map Pipeline Param Structs ============

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct NormalMapParams {
    pub resolution: u32,
    pub height_scale: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RoughnessMapParams {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub ocean_fraction: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub _pad0: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct AoMapParams {
    pub face: u32,
    pub full_resolution: u32,
    pub ao_strength: f32,
    pub ocean_level: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub resolution: u32,
    pub _pad0: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct AlbedoMapParams {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub ocean_fraction: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub _pad0: u32,
}

// ============ Generic Map Compute Pipeline ============

struct MapPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl MapPipeline {
    fn new(gpu: &GpuContext, shader_source: &str, label: &str) -> Self {
        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(label),
                    entries: &[
                        // binding 0: input heightmap (read-only)
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 1: output map (read-write)
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // binding 2: params uniform
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });

        Self {
            pipeline,
            bind_group_layout,
        }
    }

    fn dispatch_tile(
        &self,
        gpu: &GpuContext,
        heightmap_buffer: &wgpu::Buffer,
        params_bytes: &[u8],
        tile_size: u32,
        output_element_bytes: usize,
    ) -> Vec<u8> {
        let total_pixels = (tile_size * tile_size) as usize;
        let output_size = (total_pixels * output_element_bytes) as u64;

        let params_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("map params"),
                    contents: params_bytes,
                    usage: wgpu::BufferUsages::UNIFORM,
                });

        let output_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("map output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let staging_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("map staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("map bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: heightmap_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params_buffer.as_entire_binding(),
                },
            ],
        });

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("map compute encoder"),
            });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("map compute pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((tile_size + 15) / 16, (tile_size + 15) / 16, 1);
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, output_size);
        gpu.queue.submit(Some(encoder.finish()));

        staging_buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |_| {});
        let _ = gpu.device.poll(wgpu::PollType::Wait);

        let mapped = staging_buffer.slice(..).get_mapped_range();
        let result = mapped.to_vec();
        drop(mapped);
        staging_buffer.unmap();

        result
    }
}

// ============ Tiled Terrain Generation ============

fn generate_terrain_tiled(
    gpu: &GpuContext,
    terrain_pipeline: &TerrainComputePipeline,
    plates_buffer: &wgpu::Buffer,
    coordinator: &TileCoordinator,
    num_plates: u32,
    seed: u32,
    amplitude: f32,
    frequency: f32,
    octaves: u32,
    gain: f32,
    lacunarity: f32,
    progress: &mut ProgressTracker,
    cancel: &AtomicBool,
) -> Result<TectonicTerrain, String> {
    let res = coordinator.face_resolution;
    let tile_size = coordinator.tile_size;
    let tiles_per_axis = coordinator.tiles_per_axis;

    let mut faces: [Vec<f32>; 6] = Default::default();

    for face_idx in 0..6u32 {
        let mut face_data = vec![0.0f32; (res * res) as usize];

        for ty in 0..tiles_per_axis {
            for tx in 0..tiles_per_axis {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }

                let offset_x = tx * tile_size;
                let offset_y = ty * tile_size;

                let params = TerrainGenParams {
                    face: face_idx,
                    resolution: tile_size,
                    num_plates,
                    seed,
                    amplitude,
                    frequency,
                    octaves,
                    gain,
                    lacunarity,
                    tile_offset_x: offset_x,
                    tile_offset_y: offset_y,
                    full_resolution: res,
                    mountain_scale: 1.0,
                    boundary_width: 0.10,
                    warp_strength: 1.0,
                    detail_scale: 1.0,
                    surface_gravity: 9.81,
                    tectonics_factor: 0.85,
                    surface_age: 0.2,
                    _pad0: 0,
                };

                let tile_data = terrain_pipeline.dispatch_tile(gpu, plates_buffer, &params);

                // Copy tile into face
                for row in 0..tile_size {
                    let src_start = (row * tile_size) as usize;
                    let dst_start = ((offset_y + row) * res + offset_x) as usize;
                    face_data[dst_start..dst_start + tile_size as usize]
                        .copy_from_slice(&tile_data[src_start..src_start + tile_size as usize]);
                }

                progress.advance(&format!("Generating terrain face {face_idx}"));
            }
        }

        faces[face_idx as usize] = face_data;
    }

    Ok(TectonicTerrain {
        faces,
        resolution: res,
    })
}

// ============ Tiled Map Generation ============

fn generate_map_tiled<P: Pod>(
    gpu: &GpuContext,
    pipeline: &MapPipeline,
    heightmap_buffer: &wgpu::Buffer,
    coordinator: &TileCoordinator,
    make_params: impl Fn(u32, u32) -> P,
    output_element_bytes: usize,
    progress: &mut ProgressTracker,
    map_name: &str,
    face: u32,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let full_res = coordinator.face_resolution;
    let tile_size = coordinator.tile_size;
    let tiles_per_axis = coordinator.tiles_per_axis;
    let row_bytes = full_res as usize * output_element_bytes;
    let mut face_data = vec![0u8; full_res as usize * full_res as usize * output_element_bytes];

    for ty in 0..tiles_per_axis {
        for tx in 0..tiles_per_axis {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }

            let offset_x = tx * tile_size;
            let offset_y = ty * tile_size;
            let params = make_params(offset_x, offset_y);

            let tile_data = pipeline.dispatch_tile(
                gpu,
                heightmap_buffer,
                bytemuck::bytes_of(&params),
                tile_size,
                output_element_bytes,
            );

            // Copy tile into face data
            let tile_row_bytes = tile_size as usize * output_element_bytes;
            for row in 0..tile_size {
                let src_start = row as usize * tile_row_bytes;
                let dst_row = (offset_y + row) as usize;
                let dst_start = dst_row * row_bytes + offset_x as usize * output_element_bytes;
                face_data[dst_start..dst_start + tile_row_bytes]
                    .copy_from_slice(&tile_data[src_start..src_start + tile_row_bytes]);
            }

            progress.advance(&format!("{map_name} face {face}"));
        }
    }

    Ok(face_data)
}

// ============ Progress Tracker ============

struct ProgressTracker<'a> {
    tx: &'a Sender<ExportProgress>,
    current: u32,
    total: u32,
}

impl<'a> ProgressTracker<'a> {
    fn new(tx: &'a Sender<ExportProgress>, total: u32) -> Self {
        Self {
            tx,
            current: 0,
            total,
        }
    }

    fn advance(&mut self, message: &str) {
        self.current += 1;
        let fraction = self.current as f32 / self.total as f32;
        let _ = self.tx.send(ExportProgress::Progress {
            message: message.to_string(),
            fraction,
        });
    }
}

// ============ EXR Export ============

fn export_height_exr(face_data: &[f32], resolution: u32, path: &Path) -> Result<(), String> {
    let res = resolution as usize;
    exr::prelude::write_rgba_file(path, res, res, |x, y| {
        let h = face_data[y * res + x];
        (h, h, h, 1.0)
    })
    .map_err(|e| format!("EXR write error: {e}"))
}

// ============ PNG Export ============

fn export_png_rgba(data: &[f32], resolution: u32, path: &Path) -> Result<(), String> {
    let res = resolution;
    let mut img = image::RgbaImage::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let idx = (y * res + x) as usize * 4;
            let r = (data[idx].clamp(0.0, 1.0) * 255.0) as u8;
            let g = (data[idx + 1].clamp(0.0, 1.0) * 255.0) as u8;
            let b = (data[idx + 2].clamp(0.0, 1.0) * 255.0) as u8;
            let a = (data[idx + 3].clamp(0.0, 1.0) * 255.0) as u8;
            img.put_pixel(x, y, image::Rgba([r, g, b, a]));
        }
    }
    img.save(path).map_err(|e| format!("PNG write error: {e}"))
}

fn export_png_gray(data: &[f32], resolution: u32, path: &Path) -> Result<(), String> {
    let res = resolution;
    let mut img = image::GrayImage::new(res, res);
    for y in 0..res {
        for x in 0..res {
            let idx = (y * res + x) as usize;
            let v = (data[idx].clamp(0.0, 1.0) * 255.0) as u8;
            img.put_pixel(x, y, image::Luma([v]));
        }
    }
    img.save(path).map_err(|e| format!("PNG write error: {e}"))
}

// ============ CPU Mask Generation ============

fn generate_ocean_mask(heightmap: &[f32], ocean_level: f32) -> Vec<f32> {
    heightmap
        .iter()
        .map(|&h| if h < ocean_level { 1.0 } else { 0.0 })
        .collect()
}

fn cube_to_sphere(face: u32, u: f32, v: f32) -> [f32; 3] {
    let s = u * 2.0 - 1.0;
    let t = v * 2.0 - 1.0;
    let (x, y, z) = match face {
        0 => (1.0, -t, -s),
        1 => (-1.0, -t, s),
        2 => (s, 1.0, t),
        3 => (s, -1.0, -t),
        4 => (s, -t, 1.0),
        5 => (-s, -t, -1.0),
        _ => (0.0, 0.0, 1.0),
    };
    let len = (x * x + y * y + z * z).sqrt();
    [x / len, y / len, z / len]
}

fn generate_ice_mask(
    heightmap: &[f32],
    face: u32,
    resolution: u32,
    axial_tilt_rad: f32,
    base_temp_c: f32,
    ocean_level: f32,
    ocean_fraction: f32,
) -> Vec<f32> {
    let res = resolution as usize;
    let mut mask = vec![0.0f32; res * res];
    let _ice_moisture_threshold = 15.0 + 40.0 * (1.0 - ocean_fraction);

    for y in 0..res {
        for x in 0..res {
            let u = x as f32 / (res - 1) as f32;
            let v = y as f32 / (res - 1) as f32;
            let pos = cube_to_sphere(face, u, v);

            // Temperature with tilt
            let ct = axial_tilt_rad.cos();
            let st = axial_tilt_rad.sin();
            let tilted_y = pos[1] * ct + pos[2] * st;
            let effective_lat = tilted_y.clamp(-1.0, 1.0).asin();
            let lat_deg = effective_lat.abs() * 180.0 / std::f32::consts::PI;
            let lat_norm = lat_deg / 90.0;
            let temp_drop = 50.0 * (0.4 * lat_norm + 0.6 * lat_norm * lat_norm);
            let temp_offset = base_temp_c - 15.0;

            let idx = y * res + x;
            let height = heightmap[idx];
            let land_frac =
                ((height - ocean_level) / (1.0 - ocean_level).max(0.01)).clamp(0.0, 1.0);
            let elevation_km = land_frac * 5.0;
            let temp = 30.0 - temp_drop + temp_offset - elevation_km * 6.5;

            let is_ocean = height < ocean_level;
            if is_ocean {
                mask[idx] = if temp < -2.0 { 1.0 } else { 0.0 };
            } else {
                mask[idx] = if temp < -15.0 { 1.0 } else { 0.0 };
            }
        }
    }
    mask
}

// ============ Main Export Orchestrator ============

pub fn run_export(
    gpu: &GpuContext,
    config: &ExportConfig,
    params: &PlanetParams,
    derived: &DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: (f32, f32, u32, f32, f32),
    progress_tx: &Sender<ExportProgress>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let coordinator = TileCoordinator::new(config.face_resolution, config.tile_size);

    // Create output directory
    let planet_dir = config.output_dir.join(&config.planet_name);
    std::fs::create_dir_all(&planet_dir).map_err(|e| format!("Failed to create output dir: {e}"))?;

    let effective_ocean = derived.ocean_fraction * (1.0 - water_loss);
    let ocean_level = -1.0 + 2.0 * effective_ocean;
    let (amplitude, frequency, octaves, gain, lacunarity) = terrain_params;

    // Compute total steps for progress:
    // terrain tiles + erosion(6) + map tiles(3 maps * tiles_per_face * 6) + file exports(6*6)
    let tiles_per_face = coordinator.tiles_per_face();
    let total_steps = coordinator.total_tiles() // terrain generation
        + 6 // erosion
        + 4 * 6 * tiles_per_face // normal + roughness + albedo + ao tiles
        + 6 * 7; // file writes (6 faces * 7 map types)
    let mut progress = ProgressTracker::new(progress_tx, total_steps);

    // --- Phase 1: Generate plates ---
    let plates = generate_plates(&PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale,
        num_plates_override: 0,
    });

    // --- Phase 2: Generate terrain (tiled) ---
    let terrain_pipeline = TerrainComputePipeline::new(gpu);
    let plates_buffer = terrain_pipeline.create_plates_buffer(gpu, &plates);

    let mut terrain = generate_terrain_tiled(
        gpu,
        &terrain_pipeline,
        &plates_buffer,
        &coordinator,
        plates.len() as u32,
        params.seed,
        amplitude,
        frequency,
        octaves,
        gain,
        lacunarity,
        &mut progress,
        cancel,
    )?;

    // --- Phase 3: Erosion ---
    let erosion_pipeline = ErosionPipeline::new(gpu);
    for face in 0..6u32 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        progress.advance(&format!("Eroding face {face}"));
    }
    erosion_pipeline.erode(
        gpu,
        &mut terrain,
        config.erosion_iterations,
        ocean_level,
    );

    // --- Phase 4: Create map pipelines ---
    let normal_pipeline = MapPipeline::new(
        gpu,
        include_str!("shaders/normal_map.wgsl"),
        "normal map",
    );
    let roughness_pipeline = MapPipeline::new(
        gpu,
        &format!(
            "{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/roughness_map.wgsl"),
        ),
        "roughness map",
    );
    let albedo_pipeline = MapPipeline::new(
        gpu,
        &format!(
            "{}\n{}\n{}",
            include_str!("shaders/cube_sphere.wgsl"),
            include_str!("shaders/noise.wgsl"),
            include_str!("shaders/albedo_map.wgsl"),
        ),
        "albedo map",
    );
    let ao_pipeline = MapPipeline::new(
        gpu,
        include_str!("shaders/ao_map.wgsl"),
        "ao map",
    );

    // --- Phase 5: Generate maps and export per face ---
    for face in 0..6u32 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }

        let face_data = &terrain.faces[face as usize];

        // Upload full-face heightmap to GPU as read-only storage
        let heightmap_buffer =
            gpu.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("export heightmap"),
                    contents: bytemuck::cast_slice(face_data),
                    usage: wgpu::BufferUsages::STORAGE,
                });

        let tile_size = coordinator.tile_size;
        let full_res = coordinator.face_resolution;

        // -- Height EXR --
        progress.advance(&format!("Exporting height face {face}"));
        export_height_exr(
            face_data,
            full_res,
            &planet_dir.join(format!("face{face}_height.exr")),
        )?;

        // -- Normal map (tiled) --
        let normal_bytes = generate_map_tiled(
            gpu,
            &normal_pipeline,
            &heightmap_buffer,
            &coordinator,
            |offset_x, offset_y| NormalMapParams {
                resolution: tile_size,
                height_scale: 50.0,
                tile_offset_x: offset_x,
                tile_offset_y: offset_y,
                full_resolution: full_res,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            },
            16, // vec4<f32> = 16 bytes
            &mut progress,
            "Normal",
            face,
            cancel,
        )?;
        progress.advance(&format!("Exporting normal face {face}"));
        let normal_floats: &[f32] = bytemuck::cast_slice(&normal_bytes);
        export_png_rgba(
            normal_floats,
            full_res,
            &planet_dir.join(format!("face{face}_normal.png")),
        )?;

        // -- Roughness map (tiled) --
        let roughness_bytes = generate_map_tiled(
            gpu,
            &roughness_pipeline,
            &heightmap_buffer,
            &coordinator,
            |offset_x, offset_y| RoughnessMapParams {
                face,
                resolution: tile_size,
                seed: params.seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                ocean_fraction: effective_ocean,
                tile_offset_x: offset_x,
                tile_offset_y: offset_y,
                full_resolution: full_res,
                _pad0: 0,
                _pad1: 0,
                _pad2: 0,
            },
            4, // f32 = 4 bytes
            &mut progress,
            "Roughness",
            face,
            cancel,
        )?;
        progress.advance(&format!("Exporting roughness face {face}"));
        let roughness_floats: &[f32] = bytemuck::cast_slice(&roughness_bytes);
        export_png_gray(
            roughness_floats,
            full_res,
            &planet_dir.join(format!("face{face}_roughness.png")),
        )?;

        // -- Albedo map (tiled) --
        let albedo_bytes = generate_map_tiled(
            gpu,
            &albedo_pipeline,
            &heightmap_buffer,
            &coordinator,
            |offset_x, offset_y| AlbedoMapParams {
                face,
                resolution: tile_size,
                seed: params.seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                ocean_fraction: effective_ocean,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                season: config.season,
                tile_offset_x: offset_x,
                tile_offset_y: offset_y,
                full_resolution: full_res,
                _pad0: 0,
            },
            16, // vec4<f32>
            &mut progress,
            "Albedo",
            face,
            cancel,
        )?;
        progress.advance(&format!("Exporting albedo face {face}"));
        let albedo_floats: &[f32] = bytemuck::cast_slice(&albedo_bytes);
        export_png_rgba(
            albedo_floats,
            full_res,
            &planet_dir.join(format!("face{face}_albedo.png")),
        )?;

        // -- Ocean mask (CPU) --
        progress.advance(&format!("Exporting ocean mask face {face}"));
        let ocean_mask = generate_ocean_mask(face_data, ocean_level);
        export_png_gray(
            &ocean_mask,
            full_res,
            &planet_dir.join(format!("face{face}_ocean_mask.png")),
        )?;

        // -- Ice mask (CPU) --
        progress.advance(&format!("Exporting ice mask face {face}"));
        let ice_mask = generate_ice_mask(
            face_data,
            face,
            full_res,
            params.axial_tilt_deg.to_radians(),
            derived.base_temperature_c,
            ocean_level,
            effective_ocean,
        );
        export_png_gray(
            &ice_mask,
            full_res,
            &planet_dir.join(format!("face{face}_ice_mask.png")),
        )?;

        // -- AO map (tiled) --
        let ao_bytes = generate_map_tiled(
            gpu,
            &ao_pipeline,
            &heightmap_buffer,
            &coordinator,
            |offset_x, offset_y| AoMapParams {
                face,
                full_resolution: full_res,
                ao_strength: 30.0,
                ocean_level,
                tile_offset_x: offset_x,
                tile_offset_y: offset_y,
                resolution: tile_size,
                _pad0: 0,
            },
            4, // f32 = 4 bytes
            &mut progress,
            "AO",
            face,
            cancel,
        )?;
        progress.advance(&format!("Exporting AO face {face}"));
        let ao_floats: &[f32] = bytemuck::cast_slice(&ao_bytes);
        export_png_gray(
            ao_floats,
            full_res,
            &planet_dir.join(format!("face{face}_ao.png")),
        )?;
    }

    let _ = progress_tx.send(ExportProgress::Complete);
    Ok(planet_dir)
}

// ============ Background Thread Launcher ============

pub struct ExportHandle {
    pub progress_rx: std::sync::mpsc::Receiver<ExportProgress>,
    pub cancel: Arc<AtomicBool>,
    pub thread: std::thread::JoinHandle<()>,
}

pub fn spawn_export(
    gpu: Arc<GpuContext>,
    config: ExportConfig,
    params: PlanetParams,
    derived: DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: (f32, f32, u32, f32, f32),
) -> ExportHandle {
    let (tx, rx) = std::sync::mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();

    let thread = std::thread::spawn(move || {
        match run_export(
            &gpu,
            &config,
            &params,
            &derived,
            continental_scale,
            water_loss,
            terrain_params,
            &tx,
            &cancel_clone,
        ) {
            Ok(_) => {
                let _ = tx.send(ExportProgress::Complete);
            }
            Err(e) => {
                let _ = tx.send(ExportProgress::Error(e));
            }
        }
    });

    ExportHandle {
        progress_rx: rx,
        cancel,
        thread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::GpuContext;

    #[test]
    fn test_tile_coordinator() {
        let coord = TileCoordinator::new(8192, 512);
        assert_eq!(coord.tiles_per_axis, 16);
        assert_eq!(coord.tiles_per_face(), 256);
        assert_eq!(coord.total_tiles(), 1536);
    }

    #[test]
    fn test_tile_coordinator_4k() {
        let coord = TileCoordinator::new(4096, 512);
        assert_eq!(coord.tiles_per_axis, 8);
        assert_eq!(coord.tiles_per_face(), 64);
        assert_eq!(coord.total_tiles(), 384);
    }

    #[test]
    fn test_tiled_terrain_matches_direct() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
        });

        // Generate directly at 64x64
        let direct = pipeline.generate(&gpu, &plates, 64, 42, 1.0, 1.2, 8, 0.5, 2.0, 1.0, 0.10, 1.0, 1.0, 9.81, 0.85, 0.2);

        // Generate tiled at 64x64 (2x2 tiles of 32)
        let plates_buffer = pipeline.create_plates_buffer(&gpu, &plates);
        let coord = TileCoordinator::new(64, 32);
        let (tx, _rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);
        let mut progress = ProgressTracker::new(&tx, coord.total_tiles());

        let tiled = generate_terrain_tiled(
            &gpu,
            &pipeline,
            &plates_buffer,
            &coord,
            plates.len() as u32,
            42,
            1.0,
            1.2,
            8,
            0.5,
            2.0,
            &mut progress,
            &cancel,
        )
        .unwrap();

        // Compare all faces
        for face in 0..6 {
            assert_eq!(direct.faces[face].len(), tiled.faces[face].len());
            for (i, (d, t)) in direct.faces[face]
                .iter()
                .zip(tiled.faces[face].iter())
                .enumerate()
            {
                assert!(
                    (d - t).abs() < 1e-5,
                    "face {face} pixel {i}: direct={d} tiled={t}"
                );
            }
        }
    }

    #[test]
    fn test_export_small_resolution() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let tmp_dir = std::env::temp_dir().join("planet_gen_test_export");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        let config = ExportConfig {
            face_resolution: 64,
            tile_size: 32,
            output_dir: tmp_dir.clone(),
            planet_name: "test_planet".into(),
            erosion_iterations: 2,
            season: 0.5,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);

        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            1.0,  // continental_scale
            0.0,  // water_loss
            (1.0, 1.2, 8, 0.5, 2.0), // terrain params
            &tx,
            &cancel,
        );

        assert!(result.is_ok(), "Export failed: {:?}", result.err());

        let planet_dir = tmp_dir.join("test_planet");
        assert!(planet_dir.join("face0_height.exr").exists());
        assert!(planet_dir.join("face0_albedo.png").exists());
        assert!(planet_dir.join("face0_normal.png").exists());
        assert!(planet_dir.join("face0_roughness.png").exists());
        assert!(planet_dir.join("face0_ocean_mask.png").exists());
        assert!(planet_dir.join("face0_ice_mask.png").exists());
        assert!(planet_dir.join("face5_height.exr").exists());

        // Check last progress was Complete
        let mut last = None;
        while let Ok(p) = rx.try_recv() {
            last = Some(p);
        }
        assert!(matches!(last, Some(ExportProgress::Complete)));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    fn test_cancel_stops_export() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let tmp_dir = std::env::temp_dir().join("planet_gen_test_cancel");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        let config = ExportConfig {
            face_resolution: 64,
            tile_size: 32,
            output_dir: tmp_dir.clone(),
            planet_name: "cancel_test".into(),
            erosion_iterations: 2,
            season: 0.5,
        };

        let (tx, _rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(true); // Pre-cancelled

        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            1.0,
            0.0,
            (1.0, 1.2, 8, 0.5, 2.0),
            &tx,
            &cancel,
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cancelled");

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[test]
    #[ignore] // Run manually: cargo test perf_benchmark -- --ignored --nocapture
    fn perf_benchmark_2k() {
        use std::time::Instant;

        let gpu = GpuContext::new().expect("GPU init failed");
        let tmp_dir = std::env::temp_dir().join("planet_gen_bench");
        let _ = std::fs::remove_dir_all(&tmp_dir);

        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        let config = ExportConfig {
            face_resolution: 2048,
            tile_size: 512,
            output_dir: tmp_dir.clone(),
            planet_name: "benchmark".into(),
            erosion_iterations: 10,
            season: 0.5,
        };

        let (tx, _rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);

        let start = Instant::now();
        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            1.0,
            0.0,
            (1.0, 1.2, 8, 0.5, 2.0),
            &tx,
            &cancel,
        );
        let elapsed = start.elapsed();

        assert!(result.is_ok(), "Benchmark export failed: {:?}", result.err());
        println!("2K export completed in {:.2}s", elapsed.as_secs_f64());
        println!(
            "GPU: {}",
            gpu.adapter_name()
        );

        // At 2K, should complete well under 30s even on modest hardware
        assert!(
            elapsed.as_secs() < 60,
            "2K export took too long: {:.1}s (target: <60s)",
            elapsed.as_secs_f64()
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
