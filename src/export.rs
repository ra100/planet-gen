use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;
use crate::healpix_terrain::{self, HealpixTerrainParams};
use crate::planet::{DerivedProperties, PlanetParams};
use crate::plate_sim::{self, PlateSimParams};
use crate::terrain_compute::ErosionPipeline;

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

// ============ Equirectangular Export Functions ============

// ============ CPU Mask Generation ============

fn generate_ocean_mask(heightmap: &[f32], ocean_level: f32) -> Vec<f32> {
    heightmap
        .iter()
        .map(|&h| if h < ocean_level { 1.0 } else { 0.0 })
        .collect()
}


/// Inverse of cube_to_sphere: given a 3D direction, find which cube face and UV.
fn direction_to_face_uv(dx: f32, dy: f32, dz: f32) -> (usize, f32, f32) {
    let ax = dx.abs();
    let ay = dy.abs();
    let az = dz.abs();

    let (face, s, t) = if ax >= ay && ax >= az {
        if dx > 0.0 {
            (0, -dz / ax, -dy / ax)     // +X
        } else {
            (1, dz / ax, -dy / ax)      // -X
        }
    } else if ay >= ax && ay >= az {
        if dy > 0.0 {
            (2, dx / ay, dz / ay)        // +Y
        } else {
            (3, dx / ay, -dz / ay)       // -Y
        }
    } else if dz > 0.0 {
        (4, dx / az, -dy / az)           // +Z
    } else {
        (5, -dx / az, -dy / az)          // -Z
    };

    let u = ((s + 1.0) * 0.5).clamp(0.0, 1.0);
    let v = ((t + 1.0) * 0.5).clamp(0.0, 1.0);
    (face, u, v)
}

/// Convert 6 cubemap faces to a single equirectangular image with bilinear interpolation.
/// `channels` is 1 for grayscale, 4 for RGBA.
fn cubemap_to_equirect(
    faces: &[Vec<f32>; 6],
    face_res: u32,
    channels: usize,
) -> (Vec<f32>, u32, u32) {
    let eq_w = (face_res * 2) as usize;
    let eq_h = face_res as usize;
    let res = face_res as usize;
    let mut result = vec![0.0f32; eq_w * eq_h * channels];

    for y in 0..eq_h {
        let lat = std::f32::consts::PI * (0.5 - y as f32 / (eq_h - 1).max(1) as f32);
        for x in 0..eq_w {
            let lon = 2.0 * std::f32::consts::PI * (x as f32 / eq_w as f32) - std::f32::consts::PI;

            let dx = lat.cos() * lon.sin();
            let dy = lat.sin();
            let dz = lat.cos() * lon.cos();

            let (face, u, v) = direction_to_face_uv(dx, dy, dz);

            // Bilinear interpolation
            let fx = u * (res - 1) as f32;
            let fy = v * (res - 1) as f32;
            let ix = (fx as usize).min(res - 2);
            let iy = (fy as usize).min(res - 2);
            let frac_x = fx - ix as f32;
            let frac_y = fy - iy as f32;

            let fd = &faces[face];
            for c in 0..channels {
                let tl = fd[(iy * res + ix) * channels + c];
                let tr = fd[(iy * res + ix + 1) * channels + c];
                let bl = fd[((iy + 1) * res + ix) * channels + c];
                let br = fd[((iy + 1) * res + ix + 1) * channels + c];

                let top = tl + (tr - tl) * frac_x;
                let bot = bl + (br - bl) * frac_x;
                result[(y * eq_w + x) * channels + c] = top + (bot - top) * frac_y;
            }
        }
    }

    (result, eq_w as u32, eq_h as u32)
}

/// Write RGBA EXR with DWAB compression (lossy, ~80% quality).
fn export_equirect_exr_rgba(data: &[f32], width: u32, height: u32, path: &Path) -> Result<(), String> {
    use exr::prelude::*;
    let w = width as usize;
    let h = height as usize;

    let channels = SpecificChannels::rgba(|Vec2(x, y)| {
        let idx = (y * w + x) * 4;
        (data[idx], data[idx + 1], data[idx + 2], data[idx + 3])
    });

    Image::from_encoded_channels(
        (w, h),
        Encoding {
            compression: Compression::ZIP16,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    )
    .write()
    .to_file(path)
    .map_err(|e| format!("EXR write error: {e}"))
}

/// Write single-channel EXR as RGB (all same value) with DWAB compression.
fn export_equirect_exr_gray(data: &[f32], width: u32, height: u32, path: &Path) -> Result<(), String> {
    use exr::prelude::*;
    let w = width as usize;
    let h = height as usize;

    let channels = SpecificChannels::rgba(|Vec2(x, y)| {
        let v = data[y * w + x];
        (v, v, v, 1.0)
    });

    Image::from_encoded_channels(
        (w, h),
        Encoding {
            compression: Compression::ZIP16,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    )
    .write()
    .to_file(path)
    .map_err(|e| format!("EXR write error: {e}"))
}

// ============ Main Export Orchestrator ============

pub fn run_export(
    gpu: &GpuContext,
    config: &ExportConfig,
    params: &PlanetParams,
    derived: &DerivedProperties,
    water_loss: f32,
    plate_params: &PlateSimParams,
    healpix_params: &HealpixTerrainParams,
    progress_tx: &Sender<ExportProgress>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let coordinator = TileCoordinator::new(config.face_resolution, config.tile_size);

    // Create output directory
    let planet_dir = config.output_dir.join(&config.planet_name);
    std::fs::create_dir_all(&planet_dir).map_err(|e| format!("Failed to create output dir: {e}"))?;

    let effective_ocean = derived.ocean_fraction * (1.0 - water_loss);
    let ocean_level = -0.5 + 1.7 * effective_ocean;

    let tiles_per_face = coordinator.tiles_per_face();
    let total_steps = 2 // plate sim + terrain gen
        + 6 // erosion
        + 4 * 6 * tiles_per_face // normal + roughness + albedo + ao tiles
        + 6 * 7; // file writes (6 faces * 7 map types)
    let mut progress = ProgressTracker::new(progress_tx, total_steps);

    // --- Phase 1: Plate simulation on HEALPix ---
    progress.advance("Simulating plate tectonics");
    let sim = plate_sim::simulate(plate_params);

    if cancel.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }

    // --- Phase 2: Generate terrain (HEALPix → cubemap) ---
    progress.advance("Generating terrain");
    let mut terrain = healpix_terrain::generate(&sim, healpix_params, config.face_resolution);

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

    // --- Phase 5: Generate all maps per face, store in memory ---
    let tile_size = coordinator.tile_size;
    let full_res = coordinator.face_resolution;

    let mut all_heights: [Vec<f32>; 6] = Default::default();
    let mut all_normals: [Vec<f32>; 6] = Default::default();
    let mut all_roughness: [Vec<f32>; 6] = Default::default();
    let mut all_albedo: [Vec<f32>; 6] = Default::default();
    let mut all_ao: [Vec<f32>; 6] = Default::default();
    let mut all_ocean: [Vec<f32>; 6] = Default::default();

    for face in 0..6u32 {
        if cancel.load(Ordering::Relaxed) { return Err("Cancelled".into()); }

        let face_data = &terrain.faces[face as usize];
        let heightmap_buffer = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export heightmap"),
            contents: bytemuck::cast_slice(face_data),
            usage: wgpu::BufferUsages::STORAGE,
        });

        all_heights[face as usize] = face_data.clone();

        let normal_bytes = generate_map_tiled(
            gpu, &normal_pipeline, &heightmap_buffer, &coordinator,
            |ox, oy| NormalMapParams {
                resolution: tile_size, height_scale: 50.0,
                tile_offset_x: ox, tile_offset_y: oy,
                full_resolution: full_res, _pad0: 0, _pad1: 0, _pad2: 0,
            }, 16, &mut progress, "Normal", face, cancel,
        )?;
        all_normals[face as usize] = bytemuck::cast_slice::<u8, f32>(&normal_bytes).to_vec();

        let roughness_bytes = generate_map_tiled(
            gpu, &roughness_pipeline, &heightmap_buffer, &coordinator,
            |ox, oy| RoughnessMapParams {
                face, resolution: tile_size, seed: params.seed,
                base_temp_c: derived.base_temperature_c, ocean_level,
                ocean_fraction: effective_ocean,
                tile_offset_x: ox, tile_offset_y: oy,
                full_resolution: full_res, _pad0: 0, _pad1: 0, _pad2: 0,
            }, 4, &mut progress, "Roughness", face, cancel,
        )?;
        all_roughness[face as usize] = bytemuck::cast_slice::<u8, f32>(&roughness_bytes).to_vec();

        let albedo_bytes = generate_map_tiled(
            gpu, &albedo_pipeline, &heightmap_buffer, &coordinator,
            |ox, oy| AlbedoMapParams {
                face, resolution: tile_size, seed: params.seed,
                base_temp_c: derived.base_temperature_c, ocean_level,
                ocean_fraction: effective_ocean,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                season: config.season,
                tile_offset_x: ox, tile_offset_y: oy,
                full_resolution: full_res, _pad0: 0,
            }, 16, &mut progress, "Albedo", face, cancel,
        )?;
        all_albedo[face as usize] = bytemuck::cast_slice::<u8, f32>(&albedo_bytes).to_vec();

        let ao_bytes = generate_map_tiled(
            gpu, &ao_pipeline, &heightmap_buffer, &coordinator,
            |ox, oy| AoMapParams {
                face, full_resolution: full_res, ao_strength: 30.0, ocean_level,
                tile_offset_x: ox, tile_offset_y: oy,
                resolution: tile_size, _pad0: 0,
            }, 4, &mut progress, "AO", face, cancel,
        )?;
        all_ao[face as usize] = bytemuck::cast_slice::<u8, f32>(&ao_bytes).to_vec();

        all_ocean[face as usize] = generate_ocean_mask(face_data, ocean_level);

        progress.advance(&format!("Face {face} maps generated"));
    }

    // --- Phase 6: Convert cubemap to equirectangular and export ---
    progress.advance("Converting height to equirectangular...");
    let (eq_h, eq_w, eq_ht) = cubemap_to_equirect(&all_heights, full_res, 1);
    export_equirect_exr_gray(&eq_h, eq_w, eq_ht, &planet_dir.join("height.exr"))?;

    progress.advance("Converting albedo to equirectangular...");
    let (eq_a, _, _) = cubemap_to_equirect(&all_albedo, full_res, 4);
    export_equirect_exr_rgba(&eq_a, eq_w, eq_ht, &planet_dir.join("albedo.exr"))?;

    progress.advance("Converting normal to equirectangular...");
    let (eq_n, _, _) = cubemap_to_equirect(&all_normals, full_res, 4);
    export_equirect_exr_rgba(&eq_n, eq_w, eq_ht, &planet_dir.join("normal.exr"))?;

    progress.advance("Converting roughness to equirectangular...");
    let (eq_r, _, _) = cubemap_to_equirect(&all_roughness, full_res, 1);
    export_equirect_exr_gray(&eq_r, eq_w, eq_ht, &planet_dir.join("roughness.exr"))?;

    progress.advance("Converting AO to equirectangular...");
    let (eq_ao, _, _) = cubemap_to_equirect(&all_ao, full_res, 1);
    export_equirect_exr_gray(&eq_ao, eq_w, eq_ht, &planet_dir.join("ao.exr"))?;

    progress.advance("Converting water mask to equirectangular...");
    let (eq_o, _, _) = cubemap_to_equirect(&all_ocean, full_res, 1);
    export_equirect_exr_gray(&eq_o, eq_w, eq_ht, &planet_dir.join("water_mask.exr"))?;

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
    water_loss: f32,
    plate_params: PlateSimParams,
    healpix_params: HealpixTerrainParams,
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
            water_loss,
            &plate_params,
            &healpix_params,
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
    fn test_healpix_terrain_export() {
        use crate::healpix_terrain::HealpixTerrainParams;
        use crate::plate_sim::PlateSimParams;

        let sim = crate::plate_sim::simulate(&PlateSimParams {
            nside: 16,
            ..PlateSimParams::default()
        });
        let terrain = crate::healpix_terrain::generate(
            &sim,
            &HealpixTerrainParams::default(),
            64,
        );

        assert_eq!(terrain.faces.len(), 6);
        for (i, face) in terrain.faces.iter().enumerate() {
            assert_eq!(face.len(), 64 * 64, "face {i} wrong size");
            assert!(face.iter().all(|v| v.is_finite()), "face {i} has non-finite");
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

        let plate_params = PlateSimParams {
            nside: 16,
            seed: params.seed,
            num_plates: 14,
            ocean_fraction: derived.ocean_fraction,
            num_continents: 4,
            continent_size_variety: 0.35,
            tectonics_factor: derived.tectonics_factor,
        };

        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            0.0,  // water_loss
            &plate_params,
            &HealpixTerrainParams::default(),
            &tx,
            &cancel,
        );

        assert!(result.is_ok(), "Export failed: {:?}", result.err());

        let planet_dir = tmp_dir.join("test_planet");
        // Equirectangular EXR output files
        assert!(planet_dir.join("height.exr").exists());
        assert!(planet_dir.join("albedo.exr").exists());
        assert!(planet_dir.join("normal.exr").exists());
        assert!(planet_dir.join("roughness.exr").exists());
        assert!(planet_dir.join("water_mask.exr").exists());
        assert!(planet_dir.join("ao.exr").exists());

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

        let plate_params = PlateSimParams {
            nside: 16,
            seed: params.seed,
            ..PlateSimParams::default()
        };

        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            0.0,
            &plate_params,
            &HealpixTerrainParams::default(),
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

        let plate_params = PlateSimParams {
            nside: 256,
            seed: params.seed,
            ..PlateSimParams::default()
        };

        let start = Instant::now();
        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            0.0,
            &plate_params,
            &HealpixTerrainParams::default(),
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
