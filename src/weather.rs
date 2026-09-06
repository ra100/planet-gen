use crate::gpu::GpuContext;
use crate::terrain_compute::{DynamicsTextures, TectonicTerrain};
use bytemuck::{Pod, Zeroable};
use half::f16;
use std::sync::mpsc;
use wgpu::util::DeviceExt;

/// Preview weather stays independent of viewport and export resolution.
pub const DEFAULT_WEATHER_RESOLUTION: u32 = 384;
pub const WEATHER_DIAGNOSTIC_NO_SOURCE: u32 = 1;
/// Validation-only: continue transport without raining condensate out.
#[cfg(any(test, feature = "validation"))]
pub const WEATHER_DIAGNOSTIC_NO_SINK: u32 = 2;
/// Validation-only: continue transport without converting vapor into cloud phases.
#[cfg(any(test, feature = "validation"))]
const WEATHER_DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4;

/// Each `Rgba16Float` cubemap uses `6 * resolution^2 * 8` bytes.
/// Mass channels are low, deep, high, occupancy. Geometry channels are
/// base, low top, deep top, high top, all in kilometers.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Pod, Zeroable)]
pub struct WeatherSnapshot {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub storm_count: u32,
    pub coverage: f32,
    pub moisture: f32,
    pub surface_pressure_bar: f32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub storm_size: f32,
    pub radius_km: f32,
    pub rotation_rate_rad_s: f32,
    /// 0 = calm, 1 = physical baseline, 2 = strong transport.
    pub wind_scale: f32,
}

impl Default for WeatherSnapshot {
    fn default() -> Self {
        Self {
            face: 0,
            resolution: DEFAULT_WEATHER_RESOLUTION,
            seed: 0,
            storm_count: 0,
            coverage: 0.5,
            moisture: 0.5,
            surface_pressure_bar: 1.0,
            base_temp_c: 15.0,
            ocean_level: 0.0,
            axial_tilt_rad: 0.0,
            season: 0.5,
            storm_size: 1.0,
            radius_km: 6_371.0,
            rotation_rate_rad_s: 7.292_115e-5,
            wind_scale: 1.0,
        }
    }
}

const SPINUP_RESOLUTION: u32 = 128;
// Keep the 25,600 s spin-up horizon while giving transport more bounded steps.
const SPINUP_ITERATIONS: usize = 20;
const PHYSICAL_INTERVAL_SECONDS: f32 = 1280.0;
const ALL_OCEAN_SPINUP_ITERATIONS: usize = 16;
const ALL_OCEAN_PHYSICAL_INTERVAL_SECONDS: f32 = 1600.0;
const WEATHER_DIAGNOSTIC_ALL_OCEAN_COMPAT: u32 = 32;
const MAX_WIND_MPS: f32 = 50.0;
const MAX_SUBSTEP_TEXELS: f32 = 0.85;

#[derive(Clone, Copy)]
struct SpinupSchedule {
    iterations: usize,
    physical_interval_seconds: f32,
    diagnostic_flags: u32,
}

fn spinup_schedule(nearly_all_ocean: bool) -> SpinupSchedule {
    if nearly_all_ocean {
        SpinupSchedule {
            iterations: ALL_OCEAN_SPINUP_ITERATIONS,
            physical_interval_seconds: ALL_OCEAN_PHYSICAL_INTERVAL_SECONDS,
            diagnostic_flags: WEATHER_DIAGNOSTIC_ALL_OCEAN_COMPAT,
        }
    } else {
        SpinupSchedule {
            iterations: SPINUP_ITERATIONS,
            physical_interval_seconds: PHYSICAL_INTERVAL_SECONDS,
            diagnostic_flags: 0,
        }
    }
}

fn min_face_angle(resolution: u32) -> f32 {
    let step = 2.0 / (resolution - 1) as f32;
    let neighbor_length = (3.0 - 2.0 * step + step * step).sqrt();
    ((3.0 - step) / (3.0_f32.sqrt() * neighbor_length))
        .clamp(-1.0, 1.0)
        .acos()
}

#[cfg(any(test, feature = "validation"))]
fn outgoing_cfl(wind_scale: f32, resolution: u32, radius_km: f32, substeps: usize) -> f32 {
    outgoing_cfl_with_interval(
        wind_scale,
        resolution,
        radius_km,
        substeps,
        PHYSICAL_INTERVAL_SECONDS,
    )
}

fn outgoing_cfl_with_interval(
    wind_scale: f32,
    resolution: u32,
    radius_km: f32,
    substeps: usize,
    physical_interval_seconds: f32,
) -> f32 {
    // RV-003: cap raised to 4.0 in lockstep with the shader transport clamp
    // (weather_spinup.wgsl transport_substeps / ft_backtrace); bit-exact for
    // wind_scale <= 2.0, so no pins or gates are affected.
    2.0 * MAX_WIND_MPS * wind_scale.clamp(0.0, 4.0) * physical_interval_seconds
        / (radius_km.max(1.0) * 1000.0)
        / min_face_angle(resolution)
        / substeps as f32
}

#[cfg(any(test, feature = "validation"))]
fn wind_substeps(wind_scale: f32, resolution: u32, radius_km: f32) -> usize {
    wind_substeps_with_interval(wind_scale, resolution, radius_km, PHYSICAL_INTERVAL_SECONDS)
}

fn wind_substeps_with_interval(
    wind_scale: f32,
    resolution: u32,
    radius_km: f32,
    physical_interval_seconds: f32,
) -> usize {
    let outgoing = outgoing_cfl_with_interval(
        wind_scale,
        resolution,
        radius_km,
        1,
        physical_interval_seconds,
    );
    (outgoing / MAX_SUBSTEP_TEXELS).ceil().max(1.0) as usize
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SpinupParams {
    spin_resolution: u32,
    output_resolution: u32,
    seed: u32,
    storm_count: u32,
    coverage: f32,
    moisture: f32,
    surface_pressure_bar: f32,
    base_temp_c: f32,
    ocean_level: f32,
    axial_tilt_rad: f32,
    season: f32,
    storm_size: f32,
    radius_km: f32,
    rotation_rate_rad_s: f32,
    diagnostic_flags: u32,
    // Occupies the former private padding slot; WGSL has the same offset.
    wind_scale: f32,
}

struct SpinupTexture {
    _texture: wgpu::Texture,
    sampled: wgpu::TextureView,
    array: wgpu::TextureView,
    storage: wgpu::TextureView,
}

struct ProvenanceTexture {
    _texture: wgpu::Texture,
    array: wgpu::TextureView,
    storage: wgpu::TextureView,
}

struct ProvenancePipelines {
    init: wgpu::ComputePipeline,
    init_layout: wgpu::BindGroupLayout,
    transport: wgpu::ComputePipeline,
    transport_layout: wgpu::BindGroupLayout,
}

/// Readback quadruple from a provenance spin-up: tracer, final state, aux field, and tail.
type ProvenanceReadback = (
    Option<Vec<f32>>,
    Option<Vec<f32>>,
    Option<Vec<f32>>,
    Option<Vec<f32>>,
);

pub struct WeatherTextures {
    _mass_texture: wgpu::Texture,
    _geometry_texture: wgpu::Texture,
    mass_storage: wgpu::TextureView,
    geometry_storage: wgpu::TextureView,
    pub mass: wgpu::TextureView,
    pub geometry: wgpu::TextureView,
    pub resolution: u32,
}

/// Paired validation-only continuations from one immutable spin-up state.
///
/// This deliberately has no production call site: preview and export continue to use
/// [`WeatherFieldPipeline::generate`].
#[cfg(any(test, feature = "validation"))]
#[doc(hidden)]
pub struct ValidationContinuation {
    pub advected_mass: Vec<f32>,
    pub full_mass: Vec<f32>,
}

/// Validation-only internal spin-up reservoirs plus owned-water tracer, for
/// executable transfer-budget coverage. `state` is
/// (`q_bl`, `c_low`, `c_deep`, `c_high`); `aux` is
/// (`q_ft`, `P_bl`, `P_low`, `P_deep`); `provenance_tail` is
/// (`P_ft`, `P_high`, 0, 0).
///
/// This deliberately has no production call site: preview and export continue to use
/// [`WeatherFieldPipeline::generate`].
#[cfg(any(test, feature = "validation"))]
#[doc(hidden)]
pub struct TracerValidationTrace {
    /// Final spin-up condensate and boundary-layer reservoirs.
    pub state: Vec<f32>,
    /// Internal free-troposphere and source-ownership reservoirs.
    pub aux: Vec<f32>,
    /// Internal source-ownership tail reservoirs.
    pub provenance_tail: Vec<f32>,
    /// Owned-vapor tracer (R16), `None` when the adapter lacks R16 storage support.
    pub provenance: Option<Vec<f32>>,
    pub spin_resolution: u32,
}

fn read_weather_cube_texture(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
) -> Vec<f32> {
    let unpadded_bytes_per_row = resolution * 8;
    let bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weather texture readback"),
        size: (bytes_per_row * resolution * 6) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(resolution),
            },
        },
        wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 6,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("weather texture readback poll failed");
    rx.recv()
        .expect("weather texture readback callback dropped")
        .expect("weather texture readback failed");
    let mapped = slice.get_mapped_range();
    mapped
        .chunks_exact(bytes_per_row as usize)
        .flat_map(|row| row[..unpadded_bytes_per_row as usize].chunks_exact(2))
        .map(|bytes| f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32())
        .collect()
}

fn read_r16_weather_cube_texture(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
) -> Vec<f32> {
    let unpadded_bytes_per_row = resolution * 2;
    let bytes_per_row = unpadded_bytes_per_row.div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("weather provenance readback"),
        size: (bytes_per_row * resolution * 6) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(resolution),
            },
        },
        wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 6,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("weather provenance readback poll failed");
    rx.recv()
        .expect("weather provenance readback callback dropped")
        .expect("weather provenance readback failed");
    let mapped = slice.get_mapped_range();
    mapped
        .chunks_exact(bytes_per_row as usize)
        .flat_map(|row| row[..unpadded_bytes_per_row as usize].chunks_exact(2))
        .map(|bytes| f16::from_bits(u16::from_le_bytes([bytes[0], bytes[1]])).to_f32())
        .collect()
}

pub fn read_weather_cube_texture_for_sweep(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
) -> Vec<f32> {
    read_weather_cube_texture(gpu, texture, resolution)
}

impl WeatherTextures {
    pub fn read_mass(&self, gpu: &GpuContext) -> Vec<f32> {
        read_weather_cube_texture(gpu, &self._mass_texture, self.resolution)
    }

    pub fn read_geometry(&self, gpu: &GpuContext) -> Vec<f32> {
        read_weather_cube_texture(gpu, &self._geometry_texture, self.resolution)
    }

    pub fn overwrite_for_sweep(&self, gpu: &GpuContext, mass: &[f32], geometry: &[f32]) {
        let write = |texture: &wgpu::Texture, values: &[f32]| {
            let row_bytes = self.resolution as usize * 8;
            let padded_row_bytes = row_bytes.div_ceil(256) * 256;
            let mut bytes = vec![0; padded_row_bytes * self.resolution as usize * 6];
            for (row, values) in values
                .chunks_exact(self.resolution as usize * 4)
                .enumerate()
            {
                let target = &mut bytes[row * padded_row_bytes..][..row_bytes];
                for (value, target) in values.iter().zip(target.chunks_exact_mut(2)) {
                    target.copy_from_slice(&f16::from_f32(*value).to_bits().to_le_bytes());
                }
            }
            gpu.queue.write_texture(
                texture.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes as u32),
                    rows_per_image: Some(self.resolution),
                },
                wgpu::Extent3d {
                    width: self.resolution,
                    height: self.resolution,
                    depth_or_array_layers: 6,
                },
            );
        };
        let len = self.resolution as usize * self.resolution as usize * 6 * 4;
        assert_eq!(mass.len(), len);
        assert_eq!(geometry.len(), len);
        write(&self._mass_texture, mass);
        write(&self._geometry_texture, geometry);
    }
}

#[derive(Default)]
struct RevisionState {
    requested: u64,
    submitted: Option<(u64, WeatherSnapshot)>,
    ready: Option<u64>,
    pending: Option<WeatherSnapshot>,
}

impl RevisionState {
    fn request(&mut self, snapshot: WeatherSnapshot) {
        self.requested += 1;
        self.pending = Some(snapshot);
    }

    fn next_submission(&mut self) -> Option<(u64, WeatherSnapshot)> {
        if self.submitted.is_some() {
            return None;
        }
        let snapshot = self.pending.take()?;
        self.submitted = Some((self.requested, snapshot));
        Some((self.requested, snapshot))
    }

    fn complete(&mut self, revision: u64) -> Option<WeatherSnapshot> {
        let (submitted_revision, snapshot) = self.submitted?;
        if submitted_revision != revision {
            return None;
        }
        self.submitted = None;
        if self.ready.is_some_and(|ready| revision <= ready) {
            return None;
        }
        self.ready = Some(revision);
        Some(snapshot)
    }

    fn is_busy(&self) -> bool {
        self.submitted.is_some() || self.pending.is_some()
    }
}

/// Double-buffered weather generation with a latest-wins publication gate.
pub struct WeatherLifecycle {
    front: WeatherTextures,
    back: WeatherTextures,
    front_is_a: bool,
    front_snapshot: Option<WeatherSnapshot>,
    revisions: RevisionState,
    completed_tx: mpsc::Sender<u64>,
    completed_rx: mpsc::Receiver<u64>,
}

impl WeatherLifecycle {
    pub fn new(pipeline: &WeatherFieldPipeline, gpu: &GpuContext, resolution: u32) -> Self {
        let (completed_tx, completed_rx) = mpsc::channel();
        Self {
            front: pipeline.create_textures(gpu, resolution),
            back: pipeline.create_textures(gpu, resolution),
            front_is_a: true,
            front_snapshot: None,
            revisions: RevisionState::default(),
            completed_tx,
            completed_rx,
        }
    }

    pub fn request(&mut self, snapshot: WeatherSnapshot) {
        self.revisions.request(snapshot);
    }

    pub fn next_submission(&mut self) -> Option<(u64, WeatherSnapshot)> {
        self.revisions.next_submission()
    }

    pub fn back(&self) -> &WeatherTextures {
        if self.front_is_a {
            &self.back
        } else {
            &self.front
        }
    }

    pub fn front(&self) -> &WeatherTextures {
        if self.front_is_a {
            &self.front
        } else {
            &self.back
        }
    }

    pub fn front_snapshot(&self) -> Option<WeatherSnapshot> {
        self.front_snapshot
    }

    pub fn mark_submitted(&self, queue: &wgpu::Queue, revision: u64) {
        let completed_tx = self.completed_tx.clone();
        queue.on_submitted_work_done(move || {
            let _ = completed_tx.send(revision);
        });
    }

    /// Poll completion callbacks without waiting; every newer completion publishes atomically.
    pub fn poll(&mut self) -> bool {
        let mut published = false;
        while let Ok(revision) = self.completed_rx.try_recv() {
            if let Some(snapshot) = self.revisions.complete(revision) {
                self.front_is_a = !self.front_is_a;
                self.front_snapshot = Some(snapshot);
                published = true;
            }
        }
        published
    }

    pub fn is_busy(&self) -> bool {
        self.revisions.is_busy()
    }
}

pub struct WeatherFieldPipeline {
    pipeline: wgpu::ComputePipeline,
    diagnose_pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    spinup_init_pipeline: wgpu::ComputePipeline,
    spinup_init_layout: wgpu::BindGroupLayout,
    spinup_transport_pipeline: wgpu::ComputePipeline,
    spinup_transport_layout: wgpu::BindGroupLayout,
    spinup_finalize_pipeline: wgpu::ComputePipeline,
    spinup_finalize_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    spinup_iterations: usize,
    provenance: Option<ProvenancePipelines>,
}

impl WeatherFieldPipeline {
    pub fn new(gpu: &GpuContext) -> Result<Self, String> {
        Self::build(gpu)
    }

    fn build(gpu: &GpuContext) -> Result<Self, String> {
        let features = gpu.rgba16float_features;
        let required = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        if !features.allowed_usages.contains(required)
            || !features
                .flags
                .contains(wgpu::TextureFormatFeatureFlags::FILTERABLE)
        {
            return Err(format!(
                "GPU adapter '{}' does not support filterable Rgba16Float weather cubemaps",
                gpu.adapter_name()
            ));
        }
        let weather_shader = include_str!("shaders/weather_field.wgsl");
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("weather field shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}\n{}",
                        include_str!("shaders/cube_sphere.wgsl"),
                        include_str!("shaders/noise.wgsl"),
                        include_str!("shaders/climate.wgsl"),
                        weather_shader,
                    )
                    .into(),
                ),
            });
        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("weather field bgl"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 4,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 5,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 6,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::StorageTexture {
                                access: wgpu::StorageTextureAccess::WriteOnly,
                                format: wgpu::TextureFormat::Rgba16Float,
                                view_dimension: wgpu::TextureViewDimension::D2Array,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::Cube,
                                multisampled: false,
                            },
                            count: None,
                        },
                    ],
                });
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("weather field layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("weather field pipeline"),
                layout: Some(&layout),
                module: &shader,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let diagnose_pipeline =
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("weather final diagnosis pipeline"),
                    layout: Some(&layout),
                    module: &shader,
                    entry_point: Some("diagnose"),
                    compilation_options: Default::default(),
                    cache: None,
                });
        let spinup_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("weather spin-up shader"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}\n{}\n{}\n{}",
                        include_str!("shaders/cube_sphere.wgsl"),
                        include_str!("shaders/noise.wgsl"),
                        include_str!("shaders/climate.wgsl"),
                        include_str!("shaders/weather_spinup.wgsl"),
                    )
                    .into(),
                ),
            });
        let create_spinup_pipeline = |label, entry_point| {
            gpu.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(label),
                    layout: None,
                    module: &spinup_shader,
                    entry_point: Some(entry_point),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        let spinup_init_pipeline = create_spinup_pipeline("weather spin-up init", "init");
        let spinup_init_layout = spinup_init_pipeline.get_bind_group_layout(0);
        let spinup_transport_pipeline =
            create_spinup_pipeline("weather spin-up transport", "transport");
        let spinup_transport_layout = spinup_transport_pipeline.get_bind_group_layout(0);
        let spinup_finalize_pipeline =
            create_spinup_pipeline("weather spin-up finalize", "finalize");
        let spinup_finalize_layout = spinup_finalize_pipeline.get_bind_group_layout(0);
        let provenance_required =
            wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let provenance = (gpu
            .r16float_features
            .allowed_usages
            .contains(provenance_required)
            && gpu
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES))
        .then(|| {
            let init =
                create_spinup_pipeline("weather spin-up provenance init", "init_with_provenance");
            let transport = create_spinup_pipeline(
                "weather spin-up provenance transport",
                "transport_with_provenance",
            );
            ProvenancePipelines {
                init_layout: init.get_bind_group_layout(0),
                transport_layout: transport.get_bind_group_layout(0),
                init,
                transport,
            }
        });
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weather field sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            pipeline,
            diagnose_pipeline,
            bind_group_layout,
            spinup_init_pipeline,
            spinup_init_layout,
            spinup_transport_pipeline,
            spinup_transport_layout,
            spinup_finalize_pipeline,
            spinup_finalize_layout,
            sampler,
            spinup_iterations: SPINUP_ITERATIONS,
            provenance,
        })
    }

    pub fn create_textures(&self, gpu: &GpuContext, resolution: u32) -> WeatherTextures {
        let create_texture = |label| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: resolution,
                    height: resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let mass_texture = create_texture("weather mass cubemap");
        let geometry_texture = create_texture("weather geometry cubemap");
        let storage = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let sampled = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        WeatherTextures {
            mass_storage: storage(&mass_texture),
            geometry_storage: storage(&geometry_texture),
            mass: sampled(&mass_texture),
            geometry: sampled(&geometry_texture),
            _mass_texture: mass_texture,
            _geometry_texture: geometry_texture,
            resolution,
        }
    }

    pub fn generate(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
    ) {
        let _ = self.generate_with_provenance_mode(
            gpu, snapshot, terrain, dynamics, weather, 0, false, false,
        );
    }

    #[doc(hidden)]
    pub fn generate_with_diagnostic_flags(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
        diagnostic_flags: u32,
    ) {
        let _ = self.generate_with_provenance_mode(
            gpu,
            snapshot,
            terrain,
            dynamics,
            weather,
            diagnostic_flags,
            false,
            false,
        );
    }

    #[cfg(any(test, feature = "validation"))]
    #[doc(hidden)]
    pub fn generate_with_provenance_for_validation(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
    ) -> Option<Vec<f32>> {
        self.generate_with_provenance_mode(
            gpu, snapshot, terrain, dynamics, weather, 0, true, false,
        )
        .0
    }

    /// Validation-only: run the spin-up with the tracer enabled and read back both
    /// the final state and the tracer so callers can assert the ownership contract
    /// per cell (P bounds, P == T over ocean, proportional rainout, determinism).
    /// `provenance_enabled = false` forces the no-R16 fallback even on capable
    /// adapters, covering the branch incapable adapters always take.
    #[cfg(any(test, feature = "validation"))]
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn validation_tracer_trace_for_sweep(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
        diagnostic_flags: u32,
        provenance_enabled: bool,
    ) -> TracerValidationTrace {
        let spin_resolution = weather.resolution.clamp(2, SPINUP_RESOLUTION);
        let (provenance, state, aux, provenance_tail) = self.generate_with_provenance_mode(
            gpu,
            snapshot,
            terrain,
            dynamics,
            weather,
            diagnostic_flags,
            provenance_enabled,
            true,
        );
        TracerValidationTrace {
            state: state.expect("tracer trace requests the spin-up state"),
            aux: aux.expect("tracer trace requests the spin-up aux state"),
            provenance_tail: provenance_tail
                .expect("tracer trace requests the spin-up provenance tail"),
            provenance,
            spin_resolution,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn generate_with_provenance_mode(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        weather: &WeatherTextures,
        diagnostic_flags: u32,
        provenance_enabled: bool,
        readback_state: bool,
    ) -> ProvenanceReadback {
        assert_eq!(snapshot.resolution, weather.resolution);
        let pixels_per_face = (weather.resolution * weather.resolution) as usize;
        let mut heights = vec![0.0; pixels_per_face * 6];
        for (face_index, face) in terrain.faces.iter().enumerate() {
            let source_resolution = (face.len() as f32).sqrt() as usize;
            for y in 0..weather.resolution as usize {
                for x in 0..weather.resolution as usize {
                    heights[face_index * pixels_per_face + y * weather.resolution as usize + x] =
                        face[y * source_resolution / weather.resolution as usize
                            * source_resolution
                            + x * source_resolution / weather.resolution as usize];
                }
            }
        }
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather height"),
                contents: bytemuck::cast_slice(&heights),
                usage: wgpu::BufferUsages::STORAGE,
            });
        // `main` does not read the post-spin-up state, but the shared diagnosis layout
        // requires a non-overlapping cube binding for this pre-spin-up baseline pass.
        let diagnostic_placeholder = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("weather diagnosis placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let diagnostic_placeholder =
            diagnostic_placeholder.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            });
        let workgroups = weather.resolution.div_ceil(16);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        for face in 0..6 {
            let params = WeatherSnapshot { face, ..snapshot };
            let uniform = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("weather params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather field bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&weather.geometry_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&diagnostic_placeholder),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                pass.dispatch_workgroups(workgroups, workgroups, 1);
            }
        }

        let schedule = spinup_schedule(dynamics.is_nearly_all_ocean());
        let all_ocean_compat =
            schedule.diagnostic_flags != 0 && self.spinup_iterations == SPINUP_ITERATIONS;
        let spinup_iterations = if all_ocean_compat {
            schedule.iterations
        } else {
            self.spinup_iterations
        };
        let spin_resolution = weather.resolution.clamp(2, SPINUP_RESOLUTION);
        let spinup_params = SpinupParams {
            spin_resolution,
            output_resolution: weather.resolution,
            seed: snapshot.seed,
            storm_count: snapshot.storm_count,
            coverage: snapshot.coverage,
            moisture: snapshot.moisture,
            surface_pressure_bar: snapshot.surface_pressure_bar,
            base_temp_c: snapshot.base_temp_c,
            ocean_level: snapshot.ocean_level,
            axial_tilt_rad: snapshot.axial_tilt_rad,
            season: snapshot.season,
            storm_size: snapshot.storm_size,
            radius_km: snapshot.radius_km,
            rotation_rate_rad_s: snapshot.rotation_rate_rad_s,
            diagnostic_flags: diagnostic_flags
                | if all_ocean_compat {
                    schedule.diagnostic_flags
                } else {
                    0
                },
            wind_scale: snapshot.wind_scale,
        };
        let spinup_uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather spin-up params"),
                contents: bytemuck::bytes_of(&spinup_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let create_state = |label| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                // COPY_SRC: validation-only tracer readbacks (read_weather_cube_texture).
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            SpinupTexture {
                sampled: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                }),
                array: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };
        let state_a = create_state("weather spin-up state A");
        let state_b = create_state("weather spin-up state B");
        // FE-089: internal-only vertical/source bookkeeping. These textures
        // never enter WeatherTextures or the preview/export bind groups.
        let aux_a = create_state("weather spin-up aux A");
        let aux_b = create_state("weather spin-up aux B");
        let provenance_tail_a = create_state("weather spin-up provenance tail A");
        let provenance_tail_b = create_state("weather spin-up provenance tail B");
        let create_provenance = |label| {
            // Tracer VRAM policy: the A/B provenance cubemap pair is capped at
            // 384 KiB — exactly 2 x R16Float x SPINUP_RESOLUTION^2 x 6 faces x 2 B
            // today. Fail fast rather than silently doubling GPU memory if the
            // resolution cap or format ever moves.
            const PROVENANCE_PAIR_BUDGET_BYTES: usize = 384 * 1024;
            let pair_bytes = 2 * (spin_resolution * spin_resolution * 6 * 2) as usize;
            assert!(
                pair_bytes <= PROVENANCE_PAIR_BUDGET_BYTES,
                "provenance cubemap pair is {} KiB, over the {} KiB budget; \
                 raise PROVENANCE_PAIR_BUDGET_BYTES deliberately",
                pair_bytes / 1024,
                PROVENANCE_PAIR_BUDGET_BYTES / 1024,
            );
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            ProvenanceTexture {
                array: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };
        let provenance = if provenance_enabled {
            self.provenance.as_ref().map(|pipelines| {
                (
                    pipelines,
                    create_provenance("weather provenance A"),
                    create_provenance("weather provenance B"),
                )
            })
        } else {
            None
        };
        let init_bind_group = if let Some((pipelines, provenance_a, _)) = &provenance {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather spin-up provenance init bind group"),
                layout: &pipelines.init_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: spinup_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&state_a.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(&aux_a.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&provenance_tail_a.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 10,
                        resource: wgpu::BindingResource::TextureView(&provenance_a.storage),
                    },
                ],
            })
        } else {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather spin-up init bind group"),
                layout: &self.spinup_init_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: spinup_uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&state_a.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(&aux_a.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&provenance_tail_a.storage),
                    },
                ],
            })
        };
        let transport_bind_group =
            |label,
             source: &SpinupTexture,
             target: &SpinupTexture,
             aux_source: &SpinupTexture,
             aux_target: &SpinupTexture,
             tail_source: &SpinupTexture,
             tail_target: &SpinupTexture,
             provenance_source: Option<&ProvenanceTexture>,
             provenance_target: Option<&ProvenanceTexture>| {
                if let (Some((pipelines, _, _)), Some(provenance_source), Some(provenance_target)) =
                    (&provenance, provenance_source, provenance_target)
                {
                    return gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some(label),
                        layout: &pipelines.transport_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: spinup_uniform.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(
                                    &dynamics.wind_continentality,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::Sampler(&self.sampler),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: height.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(&source.array),
                            },
                            wgpu::BindGroupEntry {
                                binding: 7,
                                resource: wgpu::BindingResource::TextureView(&target.storage),
                            },
                            wgpu::BindGroupEntry {
                                binding: 9,
                                resource: wgpu::BindingResource::TextureView(
                                    &provenance_source.array,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 10,
                                resource: wgpu::BindingResource::TextureView(
                                    &provenance_target.storage,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 11,
                                resource: wgpu::BindingResource::TextureView(&aux_source.array),
                            },
                            wgpu::BindGroupEntry {
                                binding: 12,
                                resource: wgpu::BindingResource::TextureView(&aux_target.storage),
                            },
                            wgpu::BindGroupEntry {
                                binding: 13,
                                resource: wgpu::BindingResource::TextureView(&tail_source.array),
                            },
                            wgpu::BindGroupEntry {
                                binding: 14,
                                resource: wgpu::BindingResource::TextureView(&tail_target.storage),
                            },
                        ],
                    });
                }
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &self.spinup_transport_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: spinup_uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &dynamics.wind_continentality,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: height.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 6,
                            resource: wgpu::BindingResource::TextureView(&source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: wgpu::BindingResource::TextureView(&target.storage),
                        },
                        wgpu::BindGroupEntry {
                            binding: 11,
                            resource: wgpu::BindingResource::TextureView(&aux_source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 12,
                            resource: wgpu::BindingResource::TextureView(&aux_target.storage),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: wgpu::BindingResource::TextureView(&tail_source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 14,
                            resource: wgpu::BindingResource::TextureView(&tail_target.storage),
                        },
                    ],
                })
            };
        let a_to_b = transport_bind_group(
            "weather spin-up A to B",
            &state_a,
            &state_b,
            &aux_a,
            &aux_b,
            &provenance_tail_a,
            &provenance_tail_b,
            provenance.as_ref().map(|(_, a, _)| a),
            provenance.as_ref().map(|(_, _, b)| b),
        );
        let b_to_a = transport_bind_group(
            "weather spin-up B to A",
            &state_b,
            &state_a,
            &aux_b,
            &aux_a,
            &provenance_tail_b,
            &provenance_tail_a,
            provenance.as_ref().map(|(_, _, b)| b),
            provenance.as_ref().map(|(_, a, _)| a),
        );
        let transport_passes = spinup_iterations
            * wind_substeps_with_interval(
                snapshot.wind_scale,
                spin_resolution,
                snapshot.radius_km,
                if all_ocean_compat {
                    schedule.physical_interval_seconds
                } else {
                    PHYSICAL_INTERVAL_SECONDS
                },
            );
        let final_state = if transport_passes.is_multiple_of(2) {
            &state_a
        } else {
            &state_b
        };
        let final_provenance = provenance.as_ref().map(|(_, a, b)| {
            if transport_passes.is_multiple_of(2) {
                a
            } else {
                b
            }
        });
        let final_aux = if transport_passes.is_multiple_of(2) {
            &aux_a
        } else {
            &aux_b
        };
        let final_provenance_tail = if transport_passes.is_multiple_of(2) {
            &provenance_tail_a
        } else {
            &provenance_tail_b
        };
        let finalize_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up finalize bind group"),
            layout: &self.spinup_finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&final_state.array),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                },
            ],
        });
        let spin_workgroups = spin_resolution.div_ceil(8);
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(
                provenance
                    .as_ref()
                    .map_or(&self.spinup_init_pipeline, |(pipelines, _, _)| {
                        &pipelines.init
                    }),
            );
            pass.set_bind_group(0, &init_bind_group, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        for iteration in 0..transport_passes {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(
                provenance
                    .as_ref()
                    .map_or(&self.spinup_transport_pipeline, |(pipelines, _, _)| {
                        &pipelines.transport
                    }),
            );
            pass.set_bind_group(0, if iteration % 2 == 0 { &a_to_b } else { &b_to_a }, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_finalize_pipeline);
            pass.set_bind_group(0, &finalize_bind_group, &[]);
            pass.dispatch_workgroups(
                weather.resolution.div_ceil(8),
                weather.resolution.div_ceil(8),
                6,
            );
        }
        // The spin-up state is transient. Diagnose and pack both published cubemaps together.
        for face in 0..6 {
            // The face index only uses 3 bits; preserve the uniform ABI for diagnostics.
            let params = WeatherSnapshot { face, ..snapshot };
            let uniform = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("weather final diagnosis params"),
                    contents: bytemuck::bytes_of(&params),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("weather final diagnosis bind group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&weather.geometry_storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&final_state.sampled),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.diagnose_pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(workgroups, workgroups, 1);
        }
        gpu.queue.submit(Some(encoder.finish()));
        (
            final_provenance.map(|provenance| {
                read_r16_weather_cube_texture(gpu, &provenance._texture, spin_resolution)
            }),
            readback_state
                .then(|| read_weather_cube_texture(gpu, &final_state._texture, spin_resolution)),
            readback_state
                .then(|| read_weather_cube_texture(gpu, &final_aux._texture, spin_resolution)),
            readback_state.then(|| {
                read_weather_cube_texture(gpu, &final_provenance_tail._texture, spin_resolution)
            }),
        )
    }

    #[cfg(feature = "validation")]
    #[doc(hidden)]
    pub fn validation_paired_continuation_for_sweep(
        &self,
        gpu: &GpuContext,
        source_snapshot: WeatherSnapshot,
        source_terrain: &TectonicTerrain,
        source_dynamics: &DynamicsTextures,
        continuation_snapshot: WeatherSnapshot,
        continuation_terrain: &TectonicTerrain,
        continuation_dynamics: &DynamicsTextures,
    ) -> ValidationContinuation {
        self.paired_validation_continuation(
            gpu,
            source_snapshot,
            source_terrain,
            source_dynamics,
            continuation_snapshot,
            continuation_terrain,
            continuation_dynamics,
        )
    }

    /// Validation/test-only paired continuations from a single upstream front.
    #[cfg(any(test, feature = "validation"))]
    #[allow(clippy::too_many_arguments)]
    fn paired_validation_continuation(
        &self,
        gpu: &GpuContext,
        source_snapshot: WeatherSnapshot,
        source_terrain: &TectonicTerrain,
        source_dynamics: &DynamicsTextures,
        continuation_snapshot: WeatherSnapshot,
        continuation_terrain: &TectonicTerrain,
        continuation_dynamics: &DynamicsTextures,
    ) -> ValidationContinuation {
        let (source_state, _) = self.validation_spinup_state(
            gpu,
            source_snapshot,
            source_terrain,
            source_dynamics,
            None,
            0,
            self.spinup_iterations,
        );
        let advected_mass = self.validation_spinup_state(
            gpu,
            continuation_snapshot,
            continuation_terrain,
            continuation_dynamics,
            Some(&source_state),
            WEATHER_DIAGNOSTIC_NO_PHASE_CHANGE,
            1,
        );
        let full_mass = self.validation_spinup_state(
            gpu,
            continuation_snapshot,
            continuation_terrain,
            continuation_dynamics,
            Some(&source_state),
            0,
            1,
        );
        ValidationContinuation {
            advected_mass: advected_mass.1,
            full_mass: full_mass.1,
        }
    }

    #[cfg(any(test, feature = "validation"))]
    #[allow(clippy::too_many_arguments)]
    fn validation_spinup_state(
        &self,
        gpu: &GpuContext,
        snapshot: WeatherSnapshot,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        initial_state: Option<&[f32]>,
        diagnostic_flags: u32,
        iterations: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        assert_eq!(snapshot.resolution, terrain.resolution);
        let output = self.create_textures(gpu, snapshot.resolution);
        let spin_resolution = snapshot.resolution.clamp(2, SPINUP_RESOLUTION);
        let expected_state_values = spin_resolution as usize * spin_resolution as usize * 6 * 4;
        if let Some(state) = initial_state {
            assert_eq!(state.len(), expected_state_values);
        }
        let params = SpinupParams {
            spin_resolution,
            output_resolution: snapshot.resolution,
            seed: snapshot.seed,
            storm_count: snapshot.storm_count,
            coverage: snapshot.coverage,
            moisture: snapshot.moisture,
            surface_pressure_bar: snapshot.surface_pressure_bar,
            base_temp_c: snapshot.base_temp_c,
            ocean_level: snapshot.ocean_level,
            axial_tilt_rad: snapshot.axial_tilt_rad,
            season: snapshot.season,
            storm_size: snapshot.storm_size,
            radius_km: snapshot.radius_km,
            rotation_rate_rad_s: snapshot.rotation_rate_rad_s,
            diagnostic_flags,
            wind_scale: snapshot.wind_scale,
        };
        let uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("validation weather spin-up params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let heights: Vec<_> = terrain
            .faces
            .iter()
            .flat_map(|face| {
                (0..snapshot.resolution).flat_map(move |y| {
                    (0..snapshot.resolution).map(move |x| {
                        let source_resolution = (face.len() as f32).sqrt() as u32;
                        face[(y * source_resolution / snapshot.resolution * source_resolution
                            + x * source_resolution / snapshot.resolution)
                            as usize]
                    })
                })
            })
            .collect();
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("validation weather height"),
                contents: bytemuck::cast_slice(&heights),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let state = |label| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            SpinupTexture {
                sampled: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                }),
                array: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };
        let state_a = state("validation weather state A");
        let state_b = state("validation weather state B");
        let aux_a = state("validation weather aux A");
        let aux_b = state("validation weather aux B");
        let provenance_tail_a = state("validation weather provenance tail A");
        let provenance_tail_b = state("validation weather provenance tail B");
        if let Some(values) = initial_state {
            write_validation_state(gpu, &state_a._texture, spin_resolution, values);
        }
        let init = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("validation weather init"),
            layout: &self.spinup_init_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: height.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&state_a.storage),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(&aux_a.storage),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&provenance_tail_a.storage),
                },
            ],
        });
        let transport = |source: &SpinupTexture,
                         target: &SpinupTexture,
                         aux_source: &SpinupTexture,
                         aux_target: &SpinupTexture,
                         tail_source: &SpinupTexture,
                         tail_target: &SpinupTexture| {
            gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("validation weather transport"),
                layout: &self.spinup_transport_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: height.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 6,
                        resource: wgpu::BindingResource::TextureView(&source.array),
                    },
                    wgpu::BindGroupEntry {
                        binding: 7,
                        resource: wgpu::BindingResource::TextureView(&target.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 11,
                        resource: wgpu::BindingResource::TextureView(&aux_source.array),
                    },
                    wgpu::BindGroupEntry {
                        binding: 12,
                        resource: wgpu::BindingResource::TextureView(&aux_target.storage),
                    },
                    wgpu::BindGroupEntry {
                        binding: 13,
                        resource: wgpu::BindingResource::TextureView(&tail_source.array),
                    },
                    wgpu::BindGroupEntry {
                        binding: 14,
                        resource: wgpu::BindingResource::TextureView(&tail_target.storage),
                    },
                ],
            })
        };
        let a_to_b = transport(
            &state_a,
            &state_b,
            &aux_a,
            &aux_b,
            &provenance_tail_a,
            &provenance_tail_b,
        );
        let b_to_a = transport(
            &state_b,
            &state_a,
            &aux_b,
            &aux_a,
            &provenance_tail_b,
            &provenance_tail_a,
        );
        let passes =
            iterations * wind_substeps(snapshot.wind_scale, spin_resolution, snapshot.radius_km);
        let final_state = if passes.is_multiple_of(2) {
            &state_a
        } else {
            &state_b
        };
        let finalize = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("validation weather finalize"),
            layout: &self.spinup_finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&final_state.array),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&output.mass_storage),
                },
            ],
        });
        let workgroups = spin_resolution.div_ceil(8);
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if initial_state.is_none() {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_init_pipeline);
            pass.set_bind_group(0, &init, &[]);
            pass.dispatch_workgroups(workgroups, workgroups, 6);
        }
        for iteration in 0..passes {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_transport_pipeline);
            pass.set_bind_group(0, if iteration % 2 == 0 { &a_to_b } else { &b_to_a }, &[]);
            pass.dispatch_workgroups(workgroups, workgroups, 6);
        }
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.spinup_finalize_pipeline);
            pass.set_bind_group(0, &finalize, &[]);
            pass.dispatch_workgroups(
                snapshot.resolution.div_ceil(8),
                snapshot.resolution.div_ceil(8),
                6,
            );
        }
        gpu.queue.submit(Some(encoder.finish()));
        (
            read_weather_cube_texture(gpu, &final_state._texture, spin_resolution),
            output.read_mass(gpu),
        )
    }
}

#[cfg(any(test, feature = "validation"))]
fn write_validation_state(
    gpu: &GpuContext,
    texture: &wgpu::Texture,
    resolution: u32,
    values: &[f32],
) {
    let row_bytes = resolution as usize * 8;
    let padded_row_bytes = row_bytes.div_ceil(256) * 256;
    let mut bytes = vec![0; padded_row_bytes * resolution as usize * 6];
    for (row, values) in values.chunks_exact(resolution as usize * 4).enumerate() {
        for (value, target) in values
            .iter()
            .zip(bytes[row * padded_row_bytes..][..row_bytes].chunks_exact_mut(2))
        {
            target.copy_from_slice(&f16::from_f32(*value).to_bits().to_le_bytes());
        }
    }
    gpu.queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(padded_row_bytes as u32),
            rows_per_image: Some(resolution),
        },
        wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 6,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain_compute::WindFieldPipeline;

    const SPINUP_DIAGNOSTIC_NO_SOURCE: u32 = 1;
    const SPINUP_DIAGNOSTIC_NO_SINK: u32 = 2;
    const SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE: u32 = 4;
    const SPINUP_DIAGNOSTIC_NO_RELAXATION: u32 = 8;
    const SPINUP_DIAGNOSTIC_TRANSPORT_PRECLAMP: u32 = 16;

    fn terrain(resolution: u32) -> TectonicTerrain {
        terrain_from(resolution, |pos| {
            0.2 * (pos[0] * pos[2] + pos[1] * 0.3) - 0.05
        })
    }

    fn terrain_from(resolution: u32, height: impl Fn([f32; 3]) -> f32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|face| {
                (0..resolution * resolution)
                    .map(|index| {
                        crate::cube_sphere::cube_to_sphere(
                            face as u32,
                            (index % resolution) as f32 / (resolution - 1) as f32,
                            (index / resolution) as f32 / (resolution - 1) as f32,
                        )
                    })
                    .map(&height)
                    .collect()
            }),
            resolution,
        }
    }

    fn snapshot(resolution: u32) -> WeatherSnapshot {
        WeatherSnapshot {
            face: 0,
            resolution,
            seed: 42,
            storm_count: 2,
            coverage: 0.5,
            moisture: 1.0,
            surface_pressure_bar: 1.0,
            base_temp_c: 15.0,
            ocean_level: 0.0,
            axial_tilt_rad: 0.4,
            season: 0.5,
            storm_size: 1.0,
            radius_km: 6371.0,
            rotation_rate_rad_s: std::f32::consts::TAU / 86400.0,
            wind_scale: 1.0,
        }
    }

    fn condensate_centroid(state: &[f32], resolution: u32) -> Option<[f32; 3]> {
        let mut weighted = [0.0; 3];
        let mut total = 0.0;
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let index =
                        ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                    let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let mass =
                        (state[index + 1] + state[index + 2]) * (1.0 + u * u + v * v).powf(-1.5);
                    for axis in 0..3 {
                        weighted[axis] += pos[axis] * mass;
                    }
                    total += mass;
                }
            }
        }
        let length = weighted
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        (total > 0.0 && length > f32::EPSILON).then(|| weighted.map(|value| value / length))
    }

    fn condensate_core_fraction(state: &[f32], resolution: u32, radius_rad: f32) -> f32 {
        let Some(center) = condensate_centroid(state, resolution) else {
            return 0.0;
        };
        let mut core = 0.0;
        let mut total = 0.0;
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let index =
                        ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                    let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let mass =
                        (state[index + 1] + state[index + 2]) * (1.0 + u * u + v * v).powf(-1.5);
                    let in_core = center
                        .iter()
                        .zip(pos)
                        .map(|(left, right)| left * right)
                        .sum::<f32>()
                        .clamp(-1.0, 1.0)
                        .acos()
                        <= radius_rad;
                    if in_core {
                        core += mass;
                    }
                    total += mass;
                }
            }
        }
        core / total.max(f32::EPSILON)
    }

    #[test]
    fn condensate_core_fraction_rejects_uniform_diffusion() {
        let resolution = 16;
        let uniform = vec![0.0, 1.0, 0.0, 0.0]
            .into_iter()
            .cycle()
            .take(6 * resolution as usize * resolution as usize * 4)
            .collect::<Vec<_>>();

        assert!(condensate_core_fraction(&uniform, resolution, 0.25) < 0.05);
    }

    #[test]
    fn weather_snapshot_and_spinup_params_preserve_wgsl_layout() {
        use std::mem::{offset_of, size_of};

        assert_eq!(size_of::<WeatherSnapshot>(), 60);
        assert_eq!(offset_of!(WeatherSnapshot, wind_scale), 56);
        assert_eq!(size_of::<SpinupParams>(), 64);
        assert_eq!(offset_of!(SpinupParams, wind_scale), 60);
        let weather_wgsl = include_str!("shaders/weather_field.wgsl");
        let spinup_wgsl = include_str!("shaders/weather_spinup.wgsl");
        assert!(weather_wgsl.contains("wind_scale: f32,"));
        assert!(spinup_wgsl.contains("diagnostic_flags: u32,\n    wind_scale: f32,"));
    }

    #[test]
    fn spinup_keeps_the_bounded_25600_second_horizon() {
        assert_eq!(SPINUP_ITERATIONS, 20);
        assert_eq!(PHYSICAL_INTERVAL_SECONDS, 1280.0);
        assert_eq!(
            SPINUP_ITERATIONS as f32 * PHYSICAL_INTERVAL_SECONDS,
            25_600.0
        );
        let spinup_wgsl = include_str!("shaders/weather_spinup.wgsl");
        assert!(spinup_wgsl.contains(&format!(
            "const PHYSICAL_INTERVAL_SECONDS: f32 = {PHYSICAL_INTERVAL_SECONDS:.1};"
        )));
        assert!(spinup_wgsl.contains("0.88,") && spinup_wgsl.contains("1.12,"));
    }

    fn read_texture(gpu: &GpuContext, texture: &wgpu::Texture, resolution: u32) -> Vec<f32> {
        read_weather_cube_texture(gpu, texture, resolution)
    }

    fn write_texture_rgba16f(
        gpu: &GpuContext,
        texture: &wgpu::Texture,
        resolution: u32,
        value: [f32; 4],
    ) {
        write_texture_rgba16f_from(gpu, texture, resolution, |_| value);
    }

    fn write_texture_rgba16f_from(
        gpu: &GpuContext,
        texture: &wgpu::Texture,
        resolution: u32,
        value_at: impl Fn([f32; 3]) -> [f32; 4],
    ) {
        let unpadded_row_bytes = (resolution * 4 * 2) as usize;
        let row_bytes = unpadded_row_bytes.div_ceil(256) * 256;
        for face in 0..6 {
            let mut full_face = vec![0u8; row_bytes * resolution as usize];
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    let texel =
                        value_at(pos).map(|component| half::f16::from_f32(component).to_bits());
                    let offset = y as usize * row_bytes + x as usize * 8;
                    full_face[offset..offset + 8].copy_from_slice(bytemuck::bytes_of(&texel));
                }
            }
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: face,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &full_face,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_bytes as u32),
                    rows_per_image: Some(resolution),
                },
                wgpu::Extent3d {
                    width: resolution,
                    height: resolution,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    #[derive(Clone, Copy, Default)]
    struct SpinupTestConfig {
        iterations: usize,
        diagnostic_flags: u32,
        initial_state: Option<[f32; 4]>,
        initial_state_at: Option<fn([f32; 3]) -> [f32; 4]>,
    }

    struct SpinupPass {
        state: Vec<f32>,
        mass: Vec<f32>,
    }

    fn run_spinup_pass(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        snapshot: WeatherSnapshot,
        weather: &WeatherTextures,
        iterations: usize,
    ) -> SpinupPass {
        run_spinup_pass_with_config(
            gpu,
            pipeline,
            terrain,
            dynamics,
            snapshot,
            weather,
            SpinupTestConfig {
                iterations,
                ..Default::default()
            },
        )
    }

    fn run_spinup_pass_with_config(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        terrain: &TectonicTerrain,
        dynamics: &DynamicsTextures,
        snapshot: WeatherSnapshot,
        weather: &WeatherTextures,
        config: SpinupTestConfig,
    ) -> SpinupPass {
        assert_eq!(snapshot.resolution, weather.resolution);
        let spin_resolution = weather.resolution.clamp(2, SPINUP_RESOLUTION);
        let spinup_params = SpinupParams {
            spin_resolution,
            output_resolution: weather.resolution,
            seed: snapshot.seed,
            storm_count: snapshot.storm_count,
            coverage: snapshot.coverage,
            moisture: snapshot.moisture,
            surface_pressure_bar: snapshot.surface_pressure_bar,
            base_temp_c: snapshot.base_temp_c,
            ocean_level: snapshot.ocean_level,
            axial_tilt_rad: snapshot.axial_tilt_rad,
            season: snapshot.season,
            storm_size: snapshot.storm_size,
            radius_km: snapshot.radius_km,
            rotation_rate_rad_s: snapshot.rotation_rate_rad_s,
            diagnostic_flags: config.diagnostic_flags,
            wind_scale: snapshot.wind_scale,
        };
        let spinup_uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather spin-up params"),
                contents: bytemuck::bytes_of(&spinup_params),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let pixel_per_face = (terrain.resolution * terrain.resolution) as usize;
        let mut heights = vec![0.0; pixel_per_face * 6];
        for (face_index, face) in terrain.faces.iter().enumerate() {
            let source_resolution = (face.len() as f32).sqrt() as usize;
            for y in 0..snapshot.resolution as usize {
                for x in 0..snapshot.resolution as usize {
                    heights[face_index * pixel_per_face + y * snapshot.resolution as usize + x] =
                        face[y * source_resolution / snapshot.resolution as usize
                            * source_resolution
                            + x * source_resolution / snapshot.resolution as usize];
                }
            }
        }
        let height = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("weather height"),
                contents: bytemuck::cast_slice(&heights),
                usage: wgpu::BufferUsages::STORAGE,
            });

        let create_state = |label| {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: spin_resolution,
                    height: spin_resolution,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            SpinupTexture {
                sampled: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::Cube),
                    ..Default::default()
                }),
                array: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                storage: texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                }),
                _texture: texture,
            }
        };

        let state_a = create_state("weather spin-up state A");
        let state_b = create_state("weather spin-up state B");
        let aux_a = create_state("weather spin-up aux A");
        let aux_b = create_state("weather spin-up aux B");
        let provenance_tail_a = create_state("weather spin-up provenance tail A");
        let provenance_tail_b = create_state("weather spin-up provenance tail B");
        let init_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up init bind group"),
            layout: &pipeline.spinup_init_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dynamics.wind_continentality),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: height.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&state_a.storage),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(&aux_a.storage),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(&provenance_tail_a.storage),
                },
            ],
        });

        let spin_workgroups = spin_resolution.div_ceil(8);
        if let Some(initial_state) = config.initial_state {
            write_texture_rgba16f(gpu, &state_a._texture, spin_resolution, initial_state);
            let submission_index = gpu.queue.submit(std::iter::empty());
            let _ = gpu.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            });
        } else if let Some(initial_state_at) = config.initial_state_at {
            write_texture_rgba16f_from(gpu, &state_a._texture, spin_resolution, initial_state_at);
            let submission_index = gpu.queue.submit(std::iter::empty());
            let _ = gpu.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission_index),
                timeout: None,
            });
        }
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        if config.initial_state.is_none() && config.initial_state_at.is_none() {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_init_pipeline);
            pass.set_bind_group(0, &init_bind_group, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }
        let transport_bind_group =
            |label,
             source: &SpinupTexture,
             target: &SpinupTexture,
             aux_source: &SpinupTexture,
             aux_target: &SpinupTexture,
             tail_source: &SpinupTexture,
             tail_target: &SpinupTexture| {
                gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some(label),
                    layout: &pipeline.spinup_transport_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: spinup_uniform.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(
                                &dynamics.wind_continentality,
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&dynamics.pressure),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: height.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 6,
                            resource: wgpu::BindingResource::TextureView(&source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 7,
                            resource: wgpu::BindingResource::TextureView(&target.storage),
                        },
                        wgpu::BindGroupEntry {
                            binding: 11,
                            resource: wgpu::BindingResource::TextureView(&aux_source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 12,
                            resource: wgpu::BindingResource::TextureView(&aux_target.storage),
                        },
                        wgpu::BindGroupEntry {
                            binding: 13,
                            resource: wgpu::BindingResource::TextureView(&tail_source.array),
                        },
                        wgpu::BindGroupEntry {
                            binding: 14,
                            resource: wgpu::BindingResource::TextureView(&tail_target.storage),
                        },
                    ],
                })
            };
        let a_to_b = transport_bind_group(
            "weather spin-up A to B",
            &state_a,
            &state_b,
            &aux_a,
            &aux_b,
            &provenance_tail_a,
            &provenance_tail_b,
        );
        let b_to_a = transport_bind_group(
            "weather spin-up B to A",
            &state_b,
            &state_a,
            &aux_b,
            &aux_a,
            &provenance_tail_b,
            &provenance_tail_a,
        );
        let physical_interval_seconds =
            if config.diagnostic_flags & WEATHER_DIAGNOSTIC_ALL_OCEAN_COMPAT != 0 {
                ALL_OCEAN_PHYSICAL_INTERVAL_SECONDS
            } else {
                PHYSICAL_INTERVAL_SECONDS
            };
        let transport_passes = config.iterations
            * wind_substeps_with_interval(
                snapshot.wind_scale,
                spin_resolution,
                snapshot.radius_km,
                physical_interval_seconds,
            );
        for iteration in 0..transport_passes {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_transport_pipeline);
            pass.set_bind_group(0, if iteration % 2 == 0 { &a_to_b } else { &b_to_a }, &[]);
            pass.dispatch_workgroups(spin_workgroups, spin_workgroups, 6);
        }

        let final_state = if transport_passes.is_multiple_of(2) {
            &state_a
        } else {
            &state_b
        };
        let finalize_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather spin-up debug finalize bind group"),
            layout: &pipeline.spinup_finalize_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: spinup_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&final_state.array),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::TextureView(&weather.mass_storage),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline.spinup_finalize_pipeline);
            pass.set_bind_group(0, &finalize_bind_group, &[]);
            pass.dispatch_workgroups(
                weather.resolution.div_ceil(8),
                weather.resolution.div_ceil(8),
                6,
            );
        }
        gpu.queue.submit(Some(encoder.finish()));
        SpinupPass {
            state: read_weather_cube_texture(gpu, &final_state._texture, spin_resolution),
            mass: read_weather_cube_texture(gpu, &weather._mass_texture, weather.resolution),
        }
    }

    /// FNV-1a over the exact f32 bit patterns of a mass readback. Pins are
    /// strict bit-equality: any GPU output change flips the hash.
    fn mass_fingerprint(values: &[f32]) -> u64 {
        values
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, value| {
                (hash ^ u64::from(value.to_bits())).wrapping_mul(0x100_0000_01b3)
            })
    }

    fn generate_weather(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        dynamics: &DynamicsTextures,
        terrain: &TectonicTerrain,
        snapshot: WeatherSnapshot,
    ) -> WeatherTextures {
        let weather = pipeline.create_textures(gpu, snapshot.resolution);
        pipeline.generate(gpu, snapshot, terrain, dynamics, &weather);
        weather
    }

    fn generate_weather_with_provenance(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        dynamics: &DynamicsTextures,
        terrain: &TectonicTerrain,
        snapshot: WeatherSnapshot,
    ) -> (WeatherTextures, Option<Vec<f32>>) {
        let weather = pipeline.create_textures(gpu, snapshot.resolution);
        let provenance = pipeline
            .generate_with_provenance_for_validation(gpu, snapshot, terrain, dynamics, &weather);
        (weather, provenance)
    }

    fn provenance_total(values: &[f32], resolution: u32) -> f32 {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let pixel = index % (resolution * resolution) as usize;
                let x = pixel % resolution as usize;
                let y = pixel / resolution as usize;
                let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                value * (1.0 + u * u + v * v).powf(-1.5)
            })
            .sum()
    }

    #[test]
    fn provenance_mode_is_capability_gated_and_pins_both_formation_paths() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.2, 0.0, -0.1, 0.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let legacy = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(resolution));
        let (off, provenance) = generate_weather_with_provenance(
            &gpu,
            &pipeline,
            &dynamics,
            &terrain,
            snapshot(resolution),
        );
        // Realism climate migration: pins below use the shared global-reference
        // temperature and fixed 5 km/height-unit lapse rate.
        // FE-081 de-classification couples provenance into mass (warm_marine_lift
        // and inland_provenance read provenance_share), so capture-on and
        // capture-off are intentionally different fields now. Each path is pinned
        // bit-exactly instead of compared to each other; baseline moved
        // 3011d61→FE-081→DS-046 (eddy diffusion, mesoscale steering, forcing
        // exponent 0.85). Pins regenerated from a clean deterministic build
        // (cargo test --lib run twice, identical hashes). RV-002: re-baselined —
        // stale against the uncommitted FE-084/FE-086 tree before this work
        // (proven pre-existing by revert experiment); values verified across
        // repeated deterministic release builds. Cloud-formation correction:
        // remove post-transport noise erosion and use resolved marine lifting;
        // the new model intentionally changes both published-field pins.
        assert_eq!(
            mass_fingerprint(&legacy.read_mass(&gpu)),
            3_391_817_111_555_384_101
        );
        assert_eq!(
            mass_fingerprint(&off.read_mass(&gpu)),
            15_530_981_776_531_301_157
        );
        assert_eq!(provenance.is_some(), pipeline.provenance.is_some());
    }

    #[test]
    fn provenance_ocean_land_and_mixed_sources_obey_bounds_and_transport() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let run = |terrain: TectonicTerrain| {
            let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
                (
                    [
                        pos[2] * 0.8,
                        0.0,
                        -pos[0] * 0.8,
                        if pos[2] > 0.0 { 0.0 } else { 1.0 },
                    ],
                    1013.0,
                )
            });
            generate_weather_with_provenance(
                &gpu,
                &pipeline,
                &dynamics,
                &terrain,
                WeatherSnapshot {
                    coverage: 1.0,
                    base_temp_c: 30.0,
                    ..snapshot(resolution)
                },
            )
        };
        let ocean = run(terrain_from(resolution, |_| -0.1));
        let land = run(terrain_from(resolution, |_| 0.1));
        let mixed = run(terrain_from(resolution, |pos| {
            if pos[2] > 0.0 { -0.1 } else { 0.1 }
        }));
        let Some(ocean_p) = ocean.1 else {
            return;
        };
        let Some(land_p) = land.1 else {
            return;
        };
        let Some(mixed_p) = mixed.1 else {
            return;
        };
        let ocean_mass = ocean.0.read_mass(&gpu);
        let land_mass = land.0.read_mass(&gpu);
        let mixed_mass = mixed.0.read_mass(&gpu);
        assert!(provenance_total(&ocean_p, resolution) > 0.0);
        // FE-084: land evapotranspiration mints at LAND_ET_PROVENANCE_SHARE
        // (×0.3), so an all-land world carries a small nonzero provenance mass
        // instead of none — and it stays below open-ocean marine minting, so
        // land-origin and marine-origin plumes stay distinguishable.
        let land_provenance = provenance_total(&land_p, resolution);
        assert!(land_provenance > 0.0, "land ET minted no provenance");
        assert!(
            land_provenance < provenance_total(&ocean_p, resolution),
            "all-land provenance not bounded under marine minting: {land_provenance}"
        );
        assert!(
            mixed_p
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
        );
        assert!(
            mixed_p
                .iter()
                .zip(mixed_mass.chunks_exact(4))
                .any(|(p, mass)| { *p > 0.001 && mass.iter().sum::<f32>() > 0.001 })
        );
        assert!(land_mass.iter().all(|value| value.is_finite()));
        assert!(ocean_mass.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn provenance_shader_keeps_phase_and_rainout_conservative_without_new_passes() {
        let shader = include_str!("shaders/weather_spinup.wgsl");
        assert!(shader.contains("fn provenance_transport"));
        assert!(
            shader.contains("let p_after_source = transported + provenance_source_evaporation;")
        );
        assert!(
            shader.contains(
                "let p_after_rainout = p_after_source * (1.0 - provenance_rainout_scale);"
            )
        );
        assert!(shader.contains("bounded_provenance"));
        assert!(shader.contains("fn transport_with_provenance"));
        assert!(!shader.contains("provenance_finalize"));
    }

    #[test]
    fn all_ocean_schedule_and_resolved_formation_match_the_pinned_fixture() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.25, 0.0, -0.15, 0.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let selected = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(resolution));
        let schedule = spinup_schedule(dynamics.is_nearly_all_ocean());
        // Realism climate migration: global-reference temperature replaces the
        // old equatorial baseline. The new pin is verified on repeat runs.
        // FE-081 de-classification removed marine_climate conversion inflation;
        // baseline moved 3011d61→FE-081→DS-046 (eddy diffusion, mesoscale
        // steering, forcing exponent 0.85). Pin regenerated from a clean
        // deterministic build (cargo test --lib run twice, identical hash).
        // RV-002: re-baselined — the pin was stale against the uncommitted
        // FE-084/FE-086 tree before this work (failure proven pre-existing by
        // a revert experiment); new value verified identical across repeated
        // deterministic release builds. Cloud-formation correction removes
        // diagnosis noise masks and changes marine condensation support, but
        // preserves the compatibility schedule (not the old cloud shapes).
        let fingerprint = mass_fingerprint(&selected.read_mass(&gpu));

        assert_eq!(schedule.iterations, 16);
        assert_eq!(schedule.physical_interval_seconds, 1600.0);
        assert_eq!(fingerprint, 17_670_523_320_019_362_597);
    }

    #[test]
    fn land_dynamics_retains_the_20_by_1280_spinup_schedule() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_test_textures(&gpu, 16, |_| ([0.0, 0.0, 0.0, 0.65], 1013.0));
        let schedule = spinup_schedule(dynamics.is_nearly_all_ocean());

        assert_eq!(schedule.iterations, 20);
        assert_eq!(schedule.physical_interval_seconds, 1280.0);
        assert_eq!(schedule.diagnostic_flags, 0);
    }

    #[test]
    fn production_dynamics_selects_spinup_from_terrain_and_ocean_level() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let ocean = terrain_from(16, |_| -0.1);
        let land = terrain_from(16, |position| if position[0] > 0.0 { 0.1 } else { -0.1 });

        let all_ocean = wind.create_textures(&gpu, 16, &ocean, 0.0);
        let land_bearing = wind.create_textures(&gpu, 16, &land, 0.0);

        assert!(all_ocean.is_nearly_all_ocean());
        assert_eq!(
            spinup_schedule(all_ocean.is_nearly_all_ocean()).iterations,
            16
        );
        assert!(!land_bearing.is_nearly_all_ocean());
        assert_eq!(
            spinup_schedule(land_bearing.is_nearly_all_ocean()).iterations,
            20
        );
    }

    fn channel_sum_where(
        values: &[f32],
        resolution: u32,
        channel: usize,
        include: impl Fn([f32; 3]) -> bool,
    ) -> f32 {
        let mut sum = 0.0;
        for face in 0..6 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    if include(pos) {
                        let pixel =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        sum += values[pixel + channel];
                    }
                }
            }
        }
        sum
    }

    #[test]
    fn weather_is_deterministic_finite_and_channel_ordered() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16, &terrain, 0.0);
        wind.generate_gpu(
            &gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0, 1.0,
        );
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let first = generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16));
        let second = pipeline.create_textures(&gpu, 16);
        pipeline.generate_with_diagnostic_flags(
            &gpu,
            snapshot(16),
            &terrain,
            &dynamics,
            &second,
            0,
        );
        let a = read_texture(&gpu, &first._mass_texture, 16);
        let b = read_texture(&gpu, &second._mass_texture, 16);
        let geometry_a = read_texture(&gpu, &first._geometry_texture, 16);
        let geometry_b = read_texture(&gpu, &second._geometry_texture, 16);
        assert_eq!(a, b);
        assert_eq!(geometry_a, geometry_b);
        assert!(a.iter().all(|value| value.is_finite()));
        assert!(geometry_a.iter().all(|value| value.is_finite()));
        assert!(
            a.chunks_exact(4)
                .all(|pixel| pixel.iter().all(|value| (0.0..=1.0).contains(value)))
        );
        assert!(a.chunks_exact(4).all(|pixel| {
            pixel[3] + 0.002 >= pixel[0]
                && pixel[3] + 0.002 >= pixel[1]
                && pixel[3] + 0.002 >= pixel[2]
        }));
        assert!(geometry_a.chunks_exact(4).all(|pixel| {
            pixel[0] >= 0.0
                && pixel[0] <= pixel[1]
                && pixel[1] <= pixel[2]
                && pixel[2] <= pixel[3]
                && pixel[3] <= 20.0
        }));
        let occupied = a.chunks_exact(4).filter(|pixel| pixel[3] > 0.05).count();
        let occupied_fraction = occupied as f32 / (16 * 16 * 6) as f32;
        assert!(
            (0.05..=0.65).contains(&occupied_fraction),
            "half coverage should leave coherent clear sky, occupied={occupied_fraction:.2}"
        );
        let mut overcast = snapshot(16);
        overcast.coverage = 1.0;
        let overcast = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, overcast)._mass_texture,
            16,
        );
        let pixel =
            |face: usize, x: usize, y: usize| &overcast[((face * 16 + y) * 16 + x) * 4..][..4];
        let mut populated_seams = 0;
        for corners in [
            [(0, 0, 0), (2, 15, 15), (4, 15, 0)],
            [(0, 15, 0), (2, 15, 0), (5, 0, 0)],
            [(0, 0, 15), (3, 15, 0), (4, 15, 15)],
            [(0, 15, 15), (3, 15, 15), (5, 0, 15)],
            [(1, 15, 0), (2, 0, 15), (4, 0, 0)],
            [(1, 0, 0), (2, 0, 0), (5, 15, 0)],
            [(1, 15, 15), (3, 0, 0), (4, 0, 15)],
            [(1, 0, 15), (3, 0, 15), (5, 15, 15)],
        ] {
            let reference = pixel(corners[0].0, corners[0].1, corners[0].2);
            populated_seams += usize::from(reference[3] > 0.01);
            for &(face, x, y) in &corners[1..] {
                assert!(
                    reference
                        .iter()
                        .zip(pixel(face, x, y))
                        .all(|(a, b)| (a - b).abs() <= 0.01),
                    "weather corner {corners:?} is discontinuous"
                );
            }
        }
        assert!(
            populated_seams > 0,
            "seam checks must include populated weather"
        );

        let mut clear = snapshot(16);
        clear.moisture = 0.0;
        let clear = generate_weather(&gpu, &pipeline, &dynamics, &terrain, clear);
        assert!(
            read_texture(&gpu, &clear._mass_texture, 16)
                .chunks_exact(4)
                .all(|pixel| pixel == [0.0; 4])
        );

        let mut clear = snapshot(16);
        clear.coverage = 0.0;
        let clear = generate_weather(&gpu, &pipeline, &dynamics, &terrain, clear);
        assert!(
            read_texture(&gpu, &clear._mass_texture, 16)
                .chunks_exact(4)
                .all(|pixel| pixel == [0.0; 4])
        );
    }

    #[test]
    fn weather_seed_temperature_and_season_change_the_field() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16, &terrain, 0.0);
        wind.generate_gpu(
            &gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0, 1.0,
        );
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let baseline = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(16))._mass_texture,
            16,
        );
        let mut seeded = snapshot(16);
        seeded.seed = 43;
        let seeded = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, seeded)._mass_texture,
            16,
        );
        assert_ne!(baseline, seeded);
        let mut warm = snapshot(16);
        warm.base_temp_c = 35.0;
        let warm = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, warm)._mass_texture,
            16,
        );
        assert_ne!(baseline, warm);

        let mut winter = snapshot(16);
        winter.season = 0.0;
        let winter = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, winter)._mass_texture,
            16,
        );
        let mut summer = snapshot(16);
        summer.season = 1.0;
        let summer = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, summer)._mass_texture,
            16,
        );
        assert_ne!(winter, summer);
    }

    #[test]
    fn deterministic_seed_suite_preserves_weather_fields() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, resolution, &terrain, 0.0);
        wind.generate_gpu(
            &gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0, 1.0,
        );
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");

        for seed in [7, 19, 37, 73, 101, 211, 509, 997] {
            let params = WeatherSnapshot {
                seed,
                ..snapshot(resolution)
            };
            let first =
                generate_weather(&gpu, &pipeline, &dynamics, &terrain, params).read_mass(&gpu);
            let second =
                generate_weather(&gpu, &pipeline, &dynamics, &terrain, params).read_mass(&gpu);
            assert_eq!(first, second, "seed={seed}");
            assert!(first.iter().all(|value| value.is_finite()), "seed={seed}");
        }
    }

    #[test]
    fn coverage_is_continuous_from_zero() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(16);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_textures(&gpu, 16, &terrain, 0.0);
        wind.generate_gpu(
            &gpu, &terrain, &dynamics, 42, 0.0, 0.4, 0.5, 1.0, 15.0, 1.0, 1.0,
        );
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let mut totals = Vec::new();
        // FE-081 de-classification (DS-045c): f16::EPSILON sat inside the
        // round-to-zero lottery of the Rgba16Float mass storage — the per-texel
        // result sometimes quantized back to 0.0, failing strict monotonicity at
        // the first nonzero step only. Start the chain at 2^-6 instead — note
        // 16× f16::MIN_POSITIVE equals EPSILON itself (2^-10), which stays in
        // the lottery — so every device rounds it to strictly positive mass and
        // the zero-exact and strict-increase checks keep their intent.
        let first_nonzero_coverage = 2.0_f32.powi(-6);
        for coverage in [0.0, first_nonzero_coverage, 0.25, 0.5, 0.75, 1.0] {
            let mut params = snapshot(16);
            params.coverage = coverage;
            let values = read_texture(
                &gpu,
                &generate_weather(&gpu, &pipeline, &dynamics, &terrain, params)._mass_texture,
                16,
            );
            totals.push(values.chunks_exact(4).map(|pixel| pixel[3]).sum::<f32>());
        }
        assert_eq!(totals[0], 0.0);
        assert!(
            totals.windows(2).all(|pair| pair[1] > pair[0]),
            "{totals:?}"
        );
    }

    #[test]
    fn coverage_expands_causal_support() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| pos[2].max(0.0) * 0.45 - 0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let target = [1.0, 0.0, 0.0];
            let projection = pos[0];
            (
                [
                    (target[0] - pos[0] * projection) * 0.8,
                    -pos[1] * projection * 0.8,
                    -pos[2] * projection * 0.8,
                    (pos[2] + 1.0) * 0.5,
                ],
                1013.0,
            )
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let field = |coverage| {
            read_texture(
                &gpu,
                &generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    &terrain,
                    WeatherSnapshot {
                        coverage,
                        storm_count: 0,
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let metrics = |values: &[f32]| {
            let occupied: Vec<_> = values
                .chunks_exact(4)
                .map(|pixel| pixel[3] >= 0.01)
                .collect();
            let area =
                occupied.iter().filter(|&&value| value).count() as f32 / occupied.len() as f32;
            let clear = 1.0 - area;
            (area, clear, occupied)
        };
        let fields = [0.25, 0.5, 0.75].map(field);
        let support = fields.each_ref().map(|values| metrics(values));
        let summary: [(f32, f32); 3] =
            std::array::from_fn(|index| (support[index].0, support[index].1));
        eprintln!("coverage area/clear-gap={summary:?}");

        for index in 0..2 {
            let retained = support[index]
                .2
                .iter()
                .zip(&support[index + 1].2)
                .filter(|(before, after)| **before && **after)
                .count() as f32
                / (support[index].0 * support[index].2.len() as f32).max(1.0);
            assert!(
                support[index + 1].0 - support[index].0 >= 0.08,
                "support={summary:?}"
            );
            assert!(
                support[index].1 - support[index + 1].1 >= 0.08,
                "support={summary:?}"
            );
            assert!(retained >= 0.90, "support={summary:?}");
        }
    }

    #[test]
    fn marine_forcing_drives_cool_decks_warm_trades_and_continuous_coverage() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let ocean_terrain = terrain_from(resolution, |_| -0.1);
        let inland_terrain = terrain_from(resolution, |_| 0.1);
        let coverage_terrain = terrain_from(resolution, |pos| pos[2].max(0.0) * 0.45 - 0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate =
            |terrain: &TectonicTerrain, continentality: f32, base_temp_c: f32, coverage: f32| {
                let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |_| {
                    ([0.0, 0.0, 0.0, continentality], 1025.0)
                });
                let weather = generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    terrain,
                    WeatherSnapshot {
                        storm_count: 0,
                        base_temp_c,
                        coverage,
                        ..snapshot(resolution)
                    },
                );
                (weather.read_mass(&gpu), weather.read_geometry(&gpu))
            };
        let mean = |values: &[f32], channel: usize| {
            values
                .chunks_exact(4)
                .map(|pixel| pixel[channel])
                .sum::<f32>()
                / (values.len() / 4) as f32
        };

        let (cool_ocean, cool_geometry) = generate(&ocean_terrain, 0.0, 5.0, 0.75);
        let (cool_inland, _) = generate(&inland_terrain, 1.0, 5.0, 0.75);
        let cool_low = mean(&cool_ocean, 0);
        let inland_low = mean(&cool_inland, 0);
        let cool_deep = mean(&cool_ocean, 1);
        let deck_thickness = cool_geometry
            .chunks_exact(4)
            .map(|geometry| geometry[1] - geometry[0])
            .sum::<f32>()
            / (cool_geometry.len() / 4) as f32;
        assert!(
            cool_low >= inland_low * 1.5,
            "ocean={cool_low} inland={inland_low}"
        );
        assert!(
            cool_low >= cool_deep * 4.0,
            "low={cool_low} deep={cool_deep}"
        );
        assert!(
            (0.3..=1.2).contains(&deck_thickness),
            "deck thickness={deck_thickness}km"
        );

        let (warm_ocean, warm_geometry) = generate(&ocean_terrain, 0.0, 28.0, 0.75);
        let warm_top = mean(&warm_geometry, 1);
        let clear_gaps = warm_ocean
            .chunks_exact(4)
            .filter(|mass| mass[0] <= 0.05)
            .count() as f32
            / (warm_ocean.len() / 4) as f32;
        assert!(
            (1.0..=3.0).contains(&warm_top),
            "warm marine top={warm_top}km"
        );
        assert!(clear_gaps >= 0.15, "warm clear gaps={clear_gaps}");

        let coverage_field = |coverage| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                let projection = pos[0];
                (
                    [
                        (1.0 - pos[0] * projection) * 0.8,
                        -pos[1] * projection * 0.8,
                        -pos[2] * projection * 0.8,
                        (pos[2] + 1.0) * 0.5,
                    ],
                    1025.0,
                )
            });
            generate_weather(
                &gpu,
                &pipeline,
                &dynamics,
                &coverage_terrain,
                WeatherSnapshot {
                    storm_count: 0,
                    base_temp_c: 15.0,
                    coverage,
                    ..snapshot(resolution)
                },
            )
            .read_mass(&gpu)
        };
        let fields = [0.0, 0.25, 0.5, 0.75].map(coverage_field);
        let occupied = |values: &[f32]| {
            values
                .chunks_exact(4)
                .map(|pixel| pixel[3] >= 0.01)
                .collect::<Vec<_>>()
        };
        let supports = fields.each_ref().map(|field| occupied(field));
        assert!(fields[0].iter().all(|value| *value == 0.0));
        for index in 1..3 {
            let before = &supports[index];
            let after = &supports[index + 1];
            let before_area =
                before.iter().filter(|&&value| value).count() as f32 / before.len() as f32;
            let after_area =
                after.iter().filter(|&&value| value).count() as f32 / after.len() as f32;
            let retained = before
                .iter()
                .zip(after)
                .filter(|(before, after)| **before && **after)
                .count() as f32
                / (before_area * before.len() as f32).max(1.0);
            assert!(
                after_area - before_area >= 0.08,
                "areas={before_area:?}/{after_area:?}"
            );
            assert!(retained >= 0.90, "retained={retained:?}");
        }
    }

    #[test]
    fn orography_follows_wind_and_stops_in_calm_air() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| {
            let ridge = (-((pos[2] / 0.14).powi(2))).exp() * pos[0].max(0.0).powi(8);
            -0.15 + ridge * 0.45
        });
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |speed: f32| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                let tangent = [pos[2], 0.0, -pos[0]];
                let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                    .sqrt()
                    .max(0.0001);
                (
                    [
                        tangent[0] / length * speed,
                        0.0,
                        tangent[2] / length * speed,
                        // Isolate terrain forcing here; marine/inland sourcing
                        // is covered by its dedicated transport fixture.
                        0.0,
                    ],
                    1013.0,
                )
            });
            read_texture(
                &gpu,
                &generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    &terrain,
                    WeatherSnapshot {
                        storm_count: 0,
                        coverage: 1.0,
                        base_temp_c: 5.0,
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let side = |values: &[f32], positive: bool| {
            let count = (0..6)
                .flat_map(|face| {
                    (0..resolution).flat_map(move |y| {
                        (0..resolution).map(move |x| {
                            let pos = crate::cube_sphere::cube_to_sphere(
                                face,
                                x as f32 / (resolution - 1) as f32,
                                y as f32 / (resolution - 1) as f32,
                            );
                            pos[0] > 0.8
                                && if positive {
                                    pos[2] > 0.04 && pos[2] < 0.12
                                } else {
                                    pos[2] < -0.04 && pos[2] > -0.12
                                }
                        })
                    })
                })
                .filter(|selected| *selected)
                .count();
            channel_sum_where(values, resolution, 0, |pos| {
                pos[0] > 0.8
                    && if positive {
                        pos[2] > 0.04 && pos[2] < 0.12
                    } else {
                        pos[2] < -0.04 && pos[2] > -0.12
                    }
            }) / count as f32
        };
        let eastward = generate(1.0);
        let westward = generate(-1.0);
        let calm = generate(0.0);
        let whisper = generate(0.01);
        let east_asymmetry = side(&eastward, true) - side(&eastward, false);
        let west_asymmetry = side(&westward, true) - side(&westward, false);
        let calm_asymmetry = side(&calm, true) - side(&calm, false);
        let whisper_delta = (calm_asymmetry - (side(&whisper, true) - side(&whisper, false))).abs();
        let forward_delta = east_asymmetry - calm_asymmetry;
        let reverse_delta = west_asymmetry - calm_asymmetry;
        assert!(whisper_delta <= 0.01, "whisper delta={whisper_delta}");
        assert!(forward_delta >= 0.003, "forward delta={forward_delta}");
        assert!(reverse_delta <= -0.003, "reverse delta={reverse_delta}");
        assert!(
            east_asymmetry - west_asymmetry >= 0.006,
            "east={east_asymmetry}, west={west_asymmetry}"
        );
    }

    #[test]
    fn fronts_use_signed_gradient_alignment() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let generate = |pressure_sign: f32, convergent: bool| {
            let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
                let speed = if convergent { 0.8 } else { 0.0 };
                let projection = pos[1];
                (
                    [
                        -pos[0] * projection * speed,
                        (1.0 - pos[1] * projection) * speed,
                        -pos[2] * projection * speed,
                        0.0,
                    ],
                    1013.0 + pressure_sign * pos[1] * 40.0,
                )
            });
            read_texture(
                &gpu,
                &generate_weather(
                    &gpu,
                    &pipeline,
                    &dynamics,
                    &terrain,
                    WeatherSnapshot {
                        coverage: 1.0,
                        base_temp_c: -10.0,
                        ..snapshot(resolution)
                    },
                )
                ._mass_texture,
                resolution,
            )
        };
        let aligned = generate(-1.0, true);
        let opposed = generate(1.0, true);
        let no_convergence = generate(-1.0, false);
        let northern_high = |values: &[f32]| {
            channel_sum_where(values, resolution, 2, |pos| pos[1] > 0.25 && pos[1] < 0.85)
        };
        let high = [
            northern_high(&aligned),
            northern_high(&opposed),
            northern_high(&no_convergence),
        ];
        assert!(high[0] > high[1] * 1.02, "high={high:?}");
        assert!(high[0] > high[2] * 1.25, "high={high:?}");
    }

    #[test]
    fn storm_controls_do_not_inject_deep_clouds_without_source_eligibility() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| 0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([-pos[2] * 0.8, 0.0, pos[0] * 0.8, 1.0], 1000.0)
        });
        let generate = |storm_count, storm_size, moisture, base_temp_c, surface_pressure_bar| {
            let weather = generate_weather(
                &gpu,
                &pipeline,
                &dynamics,
                &terrain,
                WeatherSnapshot {
                    storm_count,
                    storm_size,
                    coverage: 0.5,
                    moisture,
                    base_temp_c,
                    surface_pressure_bar,
                    ..snapshot(resolution)
                },
            );
            read_texture(&gpu, &weather._mass_texture, resolution)
        };

        let warm_storm0 = generate(0, 0.3, 1.0, 35.0, 1.03);
        let warm_storm8 = generate(8, 3.0, 1.0, 35.0, 1.03);
        // FE-084 doctrine change: warm transpiring land now HAS source
        // eligibility (bounded evapotranspiration), so storms may organize
        // locally-sourced vapor into deep cloud — but only within the local
        // budget scale. The hard no-source guarantees live in the controls
        // below (moisture=0 and frozen ground are exact zeros).
        assert!(
            warm_storm8
                .chunks_exact(4)
                .zip(warm_storm0.chunks_exact(4))
                .all(|(storm8, storm0)| storm8[1] - storm0[1] <= 0.05),
            "storm controls added unbounded deep mass: beyond local ET budget"
        );
        for (count, size, moisture, temperature, pressure) in
            [(8, 3.0, 0.0, 35.0, 1.03), (8, 3.0, 1.0, -35.0, 1.03)]
        {
            let values = generate(count, size, moisture, temperature, pressure);
            assert!(
                values.chunks_exact(4).all(|pixel| pixel[1] <= 0.001),
                "storm controls created deep mass without a source-eligible updraft"
            );
        }
    }

    #[test]
    fn storm_controls_amplify_warm_humid_convergent_sources() {
        let resolution = 128;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let hash = |mut value: u32| {
            value = value.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
            value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277_803_737);
            (value >> 22) ^ value
        };
        let mixed = 201_u32.wrapping_mul(2_654_435_769);
        let jitter = [
            (hash(mixed) & 0xffff) as f32 / 655.35 * 2.0 - 1.0,
            (hash(mixed ^ 0x68bc_21eb) & 0xffff) as f32 / 655.35 * 2.0 - 1.0,
        ];
        let target = [
            -0.875 * jitter[1] * 0.12 + 0.484_122_93,
            0.484_122_93 * jitter[1] * 0.12 + 0.875,
            -jitter[0] * 0.12,
        ];
        let target_length = target.iter().map(|value| value * value).sum::<f32>().sqrt();
        let target = target.map(|value| value / target_length);
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let projection = target[0] * pos[0] + target[1] * pos[1] + target[2] * pos[2];
            (
                [
                    (target[0] - pos[0] * projection) * 0.8,
                    (target[1] - pos[1] * projection) * 0.8,
                    (target[2] - pos[2] * projection) * 0.8,
                    0.0,
                ],
                1000.0,
            )
        });
        let generate = |storm_count| {
            let weather = generate_weather(
                &gpu,
                &pipeline,
                &dynamics,
                &terrain,
                WeatherSnapshot {
                    seed: 0,
                    storm_count,
                    storm_size: 3.0,
                    coverage: 1.0,
                    moisture: 1.0,
                    base_temp_c: 35.0,
                    surface_pressure_bar: 1.0,
                    ..snapshot(resolution)
                },
            );
            read_texture(&gpu, &weather._mass_texture, resolution)
        };
        let baseline = generate(0);
        let storms = generate(8);
        let deep_response: f32 = storms
            .chunks_exact(4)
            .zip(baseline.chunks_exact(4))
            .map(|(storm, calm)| (storm[1] - calm[1]).max(0.0))
            .sum();
        assert!(
            deep_response > 0.002,
            "warm humid convergent storm response={deep_response}"
        );
    }

    #[test]
    fn inland_clouds_require_upstream_marine_transport() {
        let resolution = 64;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| if pos[2] > 0.0 { -0.15 } else { 0.12 });
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let run = |wind_scale, diagnostic_flags| {
            let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
                let tangent = [pos[2], 0.0, -pos[0]];
                let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                    .sqrt()
                    .max(0.0001);
                (
                    [
                        tangent[0] / length * wind_scale,
                        0.0,
                        tangent[2] / length * wind_scale,
                        if pos[2] > 0.0 { 0.0 } else { 1.0 },
                    ],
                    1025.0,
                )
            });
            let weather = pipeline.create_textures(&gpu, resolution);
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                WeatherSnapshot {
                    coverage: 1.0,
                    storm_count: 0,
                    storm_size: 3.0,
                    ..snapshot(resolution)
                },
                &weather,
                SpinupTestConfig {
                    iterations: 16,
                    diagnostic_flags,
                    initial_state: Some([0.0; 4]),
                    ..Default::default()
                },
            )
        };

        let downwind_land = |spinup: &SpinupPass| {
            channel_sum_where(&spinup.mass, resolution, 0, |pos| {
                pos[0] > 0.8 && pos[2] < -0.04 && pos[2] > -0.12
            })
        };
        let forward = run(0.8, 0);
        let reverse = run(-0.8, 0);
        let source_disabled = run(0.8, SPINUP_DIAGNOSTIC_NO_SOURCE);
        let forward_land = downwind_land(&forward);
        let reverse_land = downwind_land(&reverse);
        let disabled_land = downwind_land(&source_disabled);

        assert!(
            forward_land > 0.0,
            "forward downwind land mass={forward_land}"
        );
        assert!(
            forward_land > reverse_land * 2.0,
            "forward/reverse downwind land mass={forward_land}/{reverse_land}"
        );
        assert_eq!(
            disabled_land, 0.0,
            "disabled source land mass={disabled_land}"
        );
        assert!(
            source_disabled.state.iter().all(|value| *value == 0.0)
                && source_disabled.mass.iter().all(|value| *value == 0.0),
            "dry no-source fixture generated state or condensate"
        );
    }

    #[test]
    fn divergence_suppresses_occupancy() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            let projection = pos[0];
            (
                [
                    (1.0 - pos[0] * projection) * 0.8,
                    -pos[1] * projection * 0.8,
                    -pos[2] * projection * 0.8,
                    0.0,
                ],
                1013.0,
            )
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let mut params = snapshot(resolution);
        params.coverage = 0.75;
        let mass = read_texture(
            &gpu,
            &generate_weather(&gpu, &pipeline, &dynamics, &terrain, params)._mass_texture,
            resolution,
        );
        let convergent = channel_sum_where(&mass, resolution, 3, |pos| pos[0] > 0.55);
        let divergent = channel_sum_where(&mass, resolution, 3, |pos| pos[0] < -0.55);
        assert!(convergent > divergent * 1.1, "{convergent} <= {divergent}");
    }

    #[test]
    fn physical_weather_mass_has_no_seeded_detail_path() {
        let finalize = include_str!("shaders/weather_spinup.wgsl")
            .split("fn finalize")
            .nth(1)
            .expect("weather finalize entry point");
        assert!(!finalize.contains("snoise"));
        assert!(!finalize.contains("noise_seed_offset"));
        assert!(finalize.contains("state.y * 1.25"));
        assert!(finalize.contains("state.z * 1.5"));
        // Production publishes a second diagnosis after this finalize pass.
        // Checking only finalize missed three large noise masks in that pass.
        let diagnosis = include_str!("shaders/weather_field.wgsl")
            .split("fn diagnose")
            .nth(1)
            .expect("published weather diagnosis entry point");
        assert!(!diagnosis.contains("snoise"));
        assert!(!diagnosis.contains("noise_seed_offset"));
        assert!(diagnosis.contains("soft_bound(transported_low, 1.0)"));
    }

    #[test]
    fn published_low_clouds_follow_transported_mass_without_noise_cutouts() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.25, 0.0, -0.15, 0.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        for seed in [42, 137] {
            let mut params = snapshot(resolution);
            params.seed = seed;
            let weather = pipeline.create_textures(&gpu, resolution);
            let trace = pipeline.validation_tracer_trace_for_sweep(
                &gpu, params, &terrain, &dynamics, &weather, 0, false,
            );
            let mass = weather.read_mass(&gpu);
            let n = resolution as usize;
            assert_eq!(trace.spin_resolution, resolution);
            let mut checked_clouds = 0;
            // Interior samples avoid cross-face filtering. Match the actual
            // cubemap sampler's half-texel offset, not a texel-load oracle.
            for face in 0..6 {
                for y in 1..n - 1 {
                    for x in 1..n - 1 {
                        let sx = x as f32 / (n - 1) as f32 * n as f32 - 0.5;
                        let sy = y as f32 / (n - 1) as f32 * n as f32 - 0.5;
                        let ix = sx.floor() as usize;
                        let iy = sy.floor() as usize;
                        let fx = sx - ix as f32;
                        let fy = sy - iy as f32;
                        let at = |x, y| trace.state[(face * n * n + y * n + x) * 4 + 1];
                        let top = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
                        let bottom = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
                        let transported = (top * (1.0 - fy) + bottom * fy) * 1.25;
                        let expected = transported / (1.0 + transported);
                        let actual = mass[(face * n * n + y * n + x) * 4];
                        assert!(
                            (actual - expected).abs() < 0.001,
                            "seed {seed}, face {face}, ({x},{y}): {actual} != {expected}"
                        );
                        checked_clouds += usize::from(expected > 0.02);
                    }
                }
            }
            assert!(checked_clouds > 100, "oracle must exercise occupied clouds");
        }
    }

    #[test]
    fn submerged_relief_cannot_lift_air_or_cast_rain_shadows() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.25, 0.0, -0.15, 0.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let flat = terrain_from(resolution, |_| -0.1);
        let seabed_ridges = terrain_from(resolution, |p| {
            -0.15 + 0.10 * (p[0] * 19.0 + p[2] * 11.0).sin()
        });
        let params = snapshot(resolution);
        let reference = generate_weather(&gpu, &pipeline, &dynamics, &flat, params);
        let candidate = generate_weather(&gpu, &pipeline, &dynamics, &seabed_ridges, params);
        assert_eq!(
            reference.read_mass(&gpu),
            candidate.read_mass(&gpu),
            "ocean water, not the seabed, is the atmospheric lower boundary"
        );
        assert_eq!(
            read_texture(&gpu, &reference._geometry_texture, resolution),
            read_texture(&gpu, &candidate._geometry_texture, resolution),
            "submerged relief must not change cloud-layer heights"
        );
    }

    #[test]
    fn revisions_publish_monotonically_while_coalescing_pending_requests() {
        let mut revisions = RevisionState::default();
        let mut first_snapshot = snapshot(16);
        first_snapshot.seed = 1;
        first_snapshot.radius_km = 5000.0;
        revisions.request(first_snapshot);
        let (first, _) = revisions.next_submission().unwrap();
        let mut latest_snapshot = snapshot(16);
        latest_snapshot.seed = 3;
        latest_snapshot.radius_km = 7000.0;
        revisions.request(snapshot(16));
        revisions.request(latest_snapshot);
        let published_first = revisions.complete(first).unwrap();
        assert_eq!(published_first.seed, 1);
        assert_eq!(published_first.radius_km, 5000.0);
        assert_eq!(revisions.ready, Some(1));
        let (latest, submitted_snapshot) = revisions.next_submission().unwrap();
        assert_eq!(latest, 3);
        assert_eq!(submitted_snapshot.seed, 3);
        let published_snapshot = revisions.complete(latest).unwrap();
        assert_eq!(published_snapshot.seed, 3);
        assert_eq!(published_snapshot.radius_km, 7000.0);
        assert_eq!(revisions.ready, Some(3));
    }

    #[test]
    fn spinup_zero_iterations_keeps_initialized_vapor_out_of_final_mass() {
        let resolution = 8;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline
            .create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 0.0], 1013.0));
        let mut pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        pipeline.spinup_iterations = 0;
        let weather = pipeline.create_textures(&gpu, resolution);

        // Half-float quantization is the storage path, so pick values with stable rounding.
        let baseline = [0.4_f32, 0.20_f32, 0.0_f32, 0.0_f32];
        write_texture_rgba16f(&gpu, &weather._mass_texture, resolution, baseline);

        let pre = read_texture(&gpu, &weather._mass_texture, resolution);
        println!("pre pixel 0 {:?}", pre[0..4].to_vec());
        assert!(
            pre.chunks_exact(4)
                .all(|pixel| (0..4)
                    .all(|channel| (pixel[channel] - baseline[channel]).abs() <= 0.0001)),
            "baseline write failed: first pixel={:?}",
            pre.chunks_exact(4).next().unwrap()
        );

        let mut params = snapshot(resolution);
        params.coverage = 1.0;
        params.moisture = 1.0;

        let output =
            run_spinup_pass(&gpu, &pipeline, &terrain, &dynamics, params, &weather, 0).mass;
        println!("output pixel 0 {:?}", output[0..4].to_vec());
        assert!(
            output
                .chunks_exact(4)
                .all(|pixel| pixel.iter().all(|channel| *channel == 0.0)),
            "vapor-only initialization escaped U12 phase change: max={}",
            output.iter().copied().fold(0.0, f32::max)
        );
    }

    #[test]
    fn finite_volume_transport_is_nonnegative_and_has_bounded_solid_angle_drift() {
        const TRANSPORT_STEPS: usize = 16;
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([pos[2] * 0.4, 0.0, -pos[0] * 0.4, 0.0], 1013.0)
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let flags = SPINUP_DIAGNOSTIC_NO_SOURCE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        let run = |iterations, diagnostic_flags| {
            let weather = pipeline.create_textures(&gpu, resolution);
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot(resolution),
                &weather,
                SpinupTestConfig {
                    iterations,
                    diagnostic_flags,
                    initial_state_at: Some(|pos| {
                        // An asymmetric, bounded moisture pulse near +Z travels toward +X.
                        let center = [-0.30, 0.15, 0.94];
                        let alignment =
                            pos[0] * center[0] + pos[1] * center[1] + pos[2] * center[2];
                        let pulse = (-18.0 * (1.0 - alignment)).exp();
                        let gradient = (0.08 + 0.04 * pos[1] + 0.03 * pos[0]).max(0.01);
                        [
                            gradient + pulse * 0.24,
                            gradient * 0.45 + pulse * 0.15,
                            gradient * 0.20 + pulse * 0.08,
                            gradient * 0.70 + pulse * 0.42,
                        ]
                    }),
                    ..Default::default()
                },
            )
            .state
        };
        let initial = run(0, flags);
        let final_state = run(TRANSPORT_STEPS, flags);
        let preclamp_state = run(
            TRANSPORT_STEPS,
            flags | SPINUP_DIAGNOSTIC_TRANSPORT_PRECLAMP,
        );
        let total = |state: &[f32], channel: usize| {
            let mut total = 0.0;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        let weight = (1.0 + u * u + v * v).powf(-1.5);
                        let index =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        total += state[index + channel] * weight;
                    }
                }
            }
            total
        };
        for channel in 0..4 {
            let initial_total = total(&initial, channel);
            let final_total = total(&final_state, channel);
            let drift = (final_total - initial_total).abs() / initial_total;
            assert!(
                drift <= 0.02,
                "channel={channel}, drift={drift:.4}, initial={initial_total:.4}, final={final_total:.4}"
            );
        }

        let centroid_x = |state: &[f32]| {
            let mut weighted_x = 0.0;
            let mut total_mass = 0.0;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let pos = crate::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        let index =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        let mass =
                            state[index + 1] + state[index + 2] * (1.0 + u * u + v * v).powf(-1.5);
                        weighted_x += pos[0] * mass;
                        total_mass += mass;
                    }
                }
            }
            weighted_x / total_mass
        };
        let initial_x = centroid_x(&initial);
        let final_x = centroid_x(&final_state);
        println!(
            "transport-only redistribution: centroid_x={initial_x:.4} -> {final_x:.4}, delta={:.4}",
            final_x - initial_x
        );

        assert!(
            initial
                .iter()
                .chain(&final_state)
                .all(|value| value.is_finite() && *value >= 0.0),
            "transport-only state contains non-finite or negative moisture"
        );
        assert!(
            preclamp_state
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0),
            "pre-clamp transport candidate contains non-finite or negative moisture"
        );
        assert_eq!(
            preclamp_state, final_state,
            "transport clamp changed a valid candidate"
        );
    }

    #[test]
    fn finite_volume_transport_keeps_vapor_and_deep_channels_exactly_isolated() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([-pos[2] * 0.8, 0.0, pos[0] * 0.8, 1.0], 1013.0)
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let flags = SPINUP_DIAGNOSTIC_NO_SOURCE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        fn vapor_pulse(pos: [f32; 3]) -> [f32; 4] {
            [(-32.0 * (1.0 - pos[2])).exp() * 0.4, 0.0, 0.0, 0.0]
        }
        fn deep_pulse(pos: [f32; 3]) -> [f32; 4] {
            [0.0, 0.0, (-32.0 * (1.0 - pos[2])).exp() * 0.4, 0.0]
        }
        let run = |state_at, preclamp| {
            let weather = pipeline.create_textures(&gpu, resolution);
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot(resolution),
                &weather,
                SpinupTestConfig {
                    iterations: 8,
                    diagnostic_flags: flags
                        | if preclamp {
                            SPINUP_DIAGNOSTIC_TRANSPORT_PRECLAMP
                        } else {
                            0
                        },
                    initial_state_at: Some(state_at),
                    ..Default::default()
                },
            )
            .state
        };
        let solid_angle_total = |state: &[f32], channel| {
            state
                .chunks_exact(4)
                .enumerate()
                .map(|(index, pixel)| {
                    let x =
                        (index % resolution as usize) as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let y = ((index / resolution as usize) % resolution as usize) as f32
                        / (resolution - 1) as f32
                        * 2.0
                        - 1.0;
                    pixel[channel] * (1.0 + x * x + y * y).powf(-1.5)
                })
                .sum::<f32>()
        };
        for (channel, state_at) in [
            (0, vapor_pulse as fn([f32; 3]) -> [f32; 4]),
            (2, deep_pulse),
        ] {
            let initial = {
                let mut state = vec![0.0; (resolution * resolution * 6 * 4) as usize];
                for (index, pixel) in state.chunks_exact_mut(4).enumerate() {
                    let face = index / (resolution * resolution) as usize;
                    let x = (index % resolution as usize) as u32;
                    let y = ((index / resolution as usize) % resolution as usize) as u32;
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        x as f32 / (resolution - 1) as f32,
                        y as f32 / (resolution - 1) as f32,
                    );
                    pixel[channel] = (-32.0 * (1.0 - pos[2])).exp() * 0.4;
                }
                state
            };
            let normal = run(state_at, false);
            let preclamp = run(state_at, true);
            assert_eq!(
                normal,
                run(state_at, false),
                "channel={channel} is non-deterministic"
            );
            assert_eq!(
                normal, preclamp,
                "transport clamp changed channel {channel}"
            );
            assert!(normal.chunks_exact(4).all(|pixel| {
                pixel
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
                    && pixel
                        .iter()
                        .enumerate()
                        .all(|(index, value)| index == channel || *value == 0.0)
            }));
            let drift =
                (solid_angle_total(&normal, channel) - solid_angle_total(&initial, channel)).abs()
                    / solid_angle_total(&initial, channel).max(f32::EPSILON);
            assert!(drift <= 0.02, "channel={channel}, drift={drift}");
            let initial_peak = initial
                .chunks_exact(4)
                .map(|pixel| pixel[channel])
                .fold(0.0_f32, f32::max);
            let final_peak = normal
                .chunks_exact(4)
                .map(|pixel| pixel[channel])
                .fold(0.0_f32, f32::max);
            assert!(
                final_peak <= initial_peak + 0.001,
                "channel={channel}, overshot initial peak {initial_peak} -> {final_peak}"
            );
            let last = resolution as usize - 1;
            let pixel = |face: usize, x: usize, y: usize| {
                &normal[(face * resolution as usize * resolution as usize
                    + y * resolution as usize
                    + x)
                    * 4..][..4]
            };
            for corners in [
                [(0, 0, 0), (2, last, last), (4, last, 0)],
                [(0, last, 0), (2, last, 0), (5, 0, 0)],
                [(0, 0, last), (3, last, 0), (4, last, last)],
                [(0, last, last), (3, last, last), (5, 0, last)],
                [(1, last, 0), (2, 0, last), (4, 0, 0)],
                [(1, 0, 0), (2, 0, 0), (5, last, 0)],
                [(1, last, last), (3, 0, 0), (4, 0, last)],
                [(1, 0, last), (3, 0, last), (5, last, last)],
            ] {
                let reference = pixel(corners[0].0, corners[0].1, corners[0].2);
                for &(face, x, y) in &corners[1..] {
                    assert!(
                        reference
                            .iter()
                            .zip(pixel(face, x, y))
                            .all(|(a, b)| { (a - b).abs() <= 2.0 / resolution as f32 }),
                        "channel={channel}, seam {corners:?} is discontinuous"
                    );
                }
            }
        }
    }

    #[test]
    fn u15_strong_wind_preserves_transport_edges_and_mass() {
        let resolution = 128;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([pos[2] * 0.8, 0.0, -pos[0] * 0.8, 0.0], 1013.0)
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let flags = SPINUP_DIAGNOSTIC_NO_SOURCE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        let run = |wind_scale: f32, iterations: usize| {
            let weather = pipeline.create_textures(&gpu, resolution);
            let mut params = snapshot(resolution);
            params.wind_scale = wind_scale;
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                params,
                &weather,
                SpinupTestConfig {
                    iterations,
                    diagnostic_flags: flags,
                    initial_state_at: Some(|pos| {
                        let center = [-0.32, 0.14, 0.94];
                        let alignment =
                            pos[0] * center[0] + pos[1] * center[1] + pos[2] * center[2];
                        let pulse = (-35.0 * (1.0 - alignment)).exp();
                        [pulse, pulse * 0.5, pulse * 0.25, pulse]
                    }),
                    ..Default::default()
                },
            )
            .state
        };
        let centroid = |state: &[f32]| {
            condensate_centroid(state, resolution).expect("condensate centroid is defined")
        };
        let initial = centroid(&run(0.0, SPINUP_ITERATIONS));
        let distance_texels = |state: &[f32]| {
            let end = centroid(state);
            let dot = initial
                .iter()
                .zip(end)
                .map(|(start, finish)| start * finish)
                .sum::<f32>()
                .clamp(-1.0, 1.0);
            dot.acos() / (std::f32::consts::FRAC_PI_2 / resolution as f32)
        };
        let states = [0.0, 0.5, 1.0, 2.0].map(|scale| run(scale, SPINUP_ITERATIONS));
        let distances = states.each_ref().map(|state| distance_texels(state));
        let substeps = [0.0, 0.5, 1.0, 2.0].map(|scale| wind_substeps(scale, resolution, 6371.0));
        assert_eq!(substeps, [1, 2, 4, 7]);
        let total_mass = |state: &[f32]| {
            let mut total = 0.0;
            for face in 0..6 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let index =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        let u = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        let v = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                        total += (state[index + 1] + state[index + 2])
                            * (1.0 + u * u + v * v).powf(-1.5);
                    }
                }
            }
            total
        };
        let edge_energy = |state: &[f32]| {
            let last = resolution as usize - 1;
            let edge_point = |(face, edge): (usize, usize), index| match edge {
                0 => (face, 0, index),
                1 => (face, last, index),
                2 => (face, index, 0),
                3 => (face, index, last),
                _ => unreachable!(),
            };
            let edge_interior = |edge: (usize, usize), index| {
                let (face, x, y) = edge_point(edge, index);
                match edge.1 {
                    0 => (face, x + 1, y),
                    1 => (face, x - 1, y),
                    2 => (face, x, y + 1),
                    3 => (face, x, y - 1),
                    _ => unreachable!(),
                }
            };
            let pixel = |face: usize, x: usize, y: usize| {
                let index = (face * resolution as usize * resolution as usize
                    + y * resolution as usize
                    + x)
                    * 4;
                let u = x as f32 / last as f32 * 2.0 - 1.0;
                let v = y as f32 / last as f32 * 2.0 - 1.0;
                (
                    state[index + 1] + state[index + 2],
                    crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        x as f32 / last as f32,
                        y as f32 / last as f32,
                    ),
                    (1.0 + u * u + v * v).powf(-1.5),
                )
            };
            let energy_between = |left: (usize, usize, usize), right: (usize, usize, usize)| {
                let (left_value, left_pos, left_weight) = pixel(left.0, left.1, left.2);
                let (right_value, right_pos, right_weight) = pixel(right.0, right.1, right.2);
                let angle = left_pos
                    .iter()
                    .zip(right_pos)
                    .map(|(left, right)| left * right)
                    .sum::<f32>()
                    .clamp(-1.0, 1.0)
                    .acos();
                (left_value - right_value).abs() * (left_weight + right_weight) * 0.5 / angle
            };
            let mut energy = 0.0;
            for face in 0..6 {
                for y in 0..resolution as usize {
                    for x in 0..resolution as usize {
                        if x + 1 < resolution as usize {
                            energy += energy_between((face, x, y), (face, x + 1, y));
                        }
                        if y + 1 < resolution as usize {
                            energy += energy_between((face, x, y), (face, x, y + 1));
                        }
                    }
                }
            }
            let edges: Vec<_> = (0..6)
                .flat_map(|face| (0..4).map(move |edge| (face, edge)))
                .collect();
            for (index, left) in edges.iter().enumerate() {
                for right in &edges[index + 1..] {
                    let same = |a: [f32; 3], b: [f32; 3]| {
                        a.iter()
                            .zip(b)
                            .all(|(left, right)| (left - right).abs() < 1e-5)
                    };
                    let forward = same(
                        crate::cube_sphere::cube_to_sphere(
                            left.0 as u32,
                            edge_point(*left, 0).1 as f32 / last as f32,
                            edge_point(*left, 0).2 as f32 / last as f32,
                        ),
                        crate::cube_sphere::cube_to_sphere(
                            right.0 as u32,
                            edge_point(*right, 0).1 as f32 / last as f32,
                            edge_point(*right, 0).2 as f32 / last as f32,
                        ),
                    ) && same(
                        crate::cube_sphere::cube_to_sphere(
                            left.0 as u32,
                            edge_point(*left, last).1 as f32 / last as f32,
                            edge_point(*left, last).2 as f32 / last as f32,
                        ),
                        crate::cube_sphere::cube_to_sphere(
                            right.0 as u32,
                            edge_point(*right, last).1 as f32 / last as f32,
                            edge_point(*right, last).2 as f32 / last as f32,
                        ),
                    );
                    let reversed = same(
                        crate::cube_sphere::cube_to_sphere(
                            left.0 as u32,
                            edge_point(*left, 0).1 as f32 / last as f32,
                            edge_point(*left, 0).2 as f32 / last as f32,
                        ),
                        crate::cube_sphere::cube_to_sphere(
                            right.0 as u32,
                            edge_point(*right, last).1 as f32 / last as f32,
                            edge_point(*right, last).2 as f32 / last as f32,
                        ),
                    ) && same(
                        crate::cube_sphere::cube_to_sphere(
                            left.0 as u32,
                            edge_point(*left, last).1 as f32 / last as f32,
                            edge_point(*left, last).2 as f32 / last as f32,
                        ),
                        crate::cube_sphere::cube_to_sphere(
                            right.0 as u32,
                            edge_point(*right, 0).1 as f32 / last as f32,
                            edge_point(*right, 0).2 as f32 / last as f32,
                        ),
                    );
                    if forward || reversed {
                        for index in 0..=last {
                            let right_index = if reversed { last - index } else { index };
                            energy += energy_between(
                                edge_point(*left, index),
                                edge_interior(*right, right_index),
                            );
                            energy += energy_between(
                                edge_point(*right, right_index),
                                edge_interior(*left, index),
                            );
                        }
                    }
                }
            }
            energy
        };
        let mass_delta = (total_mass(&states[3]) - total_mass(&states[2])).abs()
            / total_mass(&states[2]).max(f32::EPSILON);
        let edge_ratio = edge_energy(&states[3]) / edge_energy(&states[2]).max(f32::EPSILON);
        let core_fraction = condensate_core_fraction(&states[3], resolution, 0.25);
        println!(
            "U15 20x1280s transport texels={distances:?}, substeps={substeps:?}, mass_delta={mass_delta:.5}, core_fraction={core_fraction:.5}, edge_gradient_ratio={edge_ratio:.5}"
        );
        assert!(
            distances[0] <= 1.0 / resolution as f32,
            "calm moved {} texels",
            distances[0]
        );
        assert!(
            distances[1] >= 0.5 && distances[2] >= 1.0 && distances[3] >= 2.0,
            "{distances:?}"
        );
        assert!(
            distances[1..].windows(2).all(|pair| pair[1] > pair[0]),
            "{distances:?}"
        );
        assert!(mass_delta <= 0.20, "mass_delta={mass_delta:.5}");
        assert!(
            core_fraction >= 0.10,
            "core_fraction={core_fraction:.5}; uniform diffusion must fail this concentration gate"
        );
        assert!(edge_ratio >= 0.70, "edge_gradient_ratio={edge_ratio:.5}");
        for scale in [0.5, 1.0, 2.0] {
            let per_substep = outgoing_cfl(
                scale,
                resolution,
                6371.0,
                wind_substeps(scale, resolution, 6371.0),
            );
            assert!(
                per_substep <= MAX_SUBSTEP_TEXELS,
                "scale={scale}, outgoing CFL={per_substep}"
            );
        }
    }

    #[test]
    fn outgoing_cfl_substeps_match_pinned_cubemap_counts() {
        for (resolution, expected) in [
            (32, [1, 1, 2]),
            (64, [1, 2, 4]),
            (128, [2, 4, 7]),
            (256, [4, 7, 13]),
            (384, [5, 10, 20]),
        ] {
            assert_eq!(
                [0.5, 1.0, 2.0].map(|scale| wind_substeps(scale, resolution, 6371.0)),
                expected,
                "resolution={resolution}"
            );
        }
    }

    #[test]
    fn spinup_pingpong_selects_the_last_nonuniform_transport_state() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |pos| pos[0] * 0.15 - 0.1);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline.create_test_textures(&gpu, resolution, |pos| {
            ([pos[2] * 0.8, 0.0, -pos[0] * 0.8, 1.0], 1013.0)
        });

        let diff_count = |a: &[f32], b: &[f32]| {
            a.chunks_exact(4)
                .zip(b.chunks_exact(4))
                .filter(|(lhs, rhs)| {
                    (0..4).any(|channel| (lhs[channel] - rhs[channel]).abs() > 0.001)
                })
                .count()
        };

        let flags = SPINUP_DIAGNOSTIC_NO_SOURCE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        let mut snapshots = Vec::new();
        for spinup_iterations in 0..4 {
            let mut pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
            pipeline.spinup_iterations = spinup_iterations;
            let weather = pipeline.create_textures(&gpu, resolution);
            let pixels = run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot(resolution),
                &weather,
                SpinupTestConfig {
                    iterations: spinup_iterations,
                    diagnostic_flags: flags,
                    initial_state_at: Some(|pos| {
                        let pulse = (-24.0 * (1.0 - pos[2])).exp();
                        [0.05 + pulse * 0.35, 0.02 + pos[0].max(0.0) * 0.08, 0.0, 0.0]
                    }),
                    ..Default::default()
                },
            );
            snapshots.push(pixels.state);
        }

        assert!(
            diff_count(&snapshots[0], &snapshots[1]) > 0,
            "one transport pass did not publish state B: changed_0_1={}",
            diff_count(&snapshots[0], &snapshots[1])
        );
        assert!(
            diff_count(&snapshots[2], &snapshots[3]) > 0,
            "three transport passes did not publish state B: changed_2_3={}",
            diff_count(&snapshots[2], &snapshots[3])
        );
    }

    #[test]
    fn source_ownership_keeps_vapor_out_of_diagnosis_until_u12_phase_change() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 0.0], 1013.0));
        let vapor_only = |terrain: TectonicTerrain| {
            let mut pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
            pipeline.spinup_iterations = 0;
            let weather =
                generate_weather(&gpu, &pipeline, &dynamics, &terrain, snapshot(resolution));
            weather.read_mass(&gpu)
        };

        for terrain in [
            terrain_from(resolution, |_| -0.1),
            terrain_from(resolution, |_| 0.2),
            terrain_from(resolution, |pos| {
                -0.15 + (-(pos[2] / 0.14).powi(2)).exp() * 0.45
            }),
        ] {
            let mass = vapor_only(terrain);
            assert!(
                mass.iter().all(|value| *value == 0.0),
                "vapor-only diagnosis emitted cloud mass: max={}",
                mass.iter().copied().fold(0.0, f32::max)
            );
        }

        let terrain = terrain_from(resolution, |_| -0.1);
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let zero_baseline = pipeline.create_textures(&gpu, resolution);
        write_texture_rgba16f(&gpu, &zero_baseline._mass_texture, resolution, [0.0; 4]);
        let no_source = run_spinup_pass_with_config(
            &gpu,
            &pipeline,
            &terrain,
            &dynamics,
            snapshot(resolution),
            &zero_baseline,
            SpinupTestConfig {
                iterations: 4,
                diagnostic_flags: SPINUP_DIAGNOSTIC_NO_SOURCE,
                ..Default::default()
            },
        );
        assert!(no_source.state.iter().all(|value| *value == 0.0));
        assert!(no_source.mass.iter().all(|value| *value == 0.0));

        let no_phase = run_spinup_pass_with_config(
            &gpu,
            &pipeline,
            &terrain,
            &dynamics,
            snapshot(resolution),
            &pipeline.create_textures(&gpu, resolution),
            SpinupTestConfig {
                iterations: 4,
                diagnostic_flags: SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE,
                ..Default::default()
            },
        );
        assert!(no_phase.state.chunks_exact(4).any(|state| state[0] > 0.0));
        assert!(
            no_phase
                .state
                .chunks_exact(4)
                .all(|state| state[1] == 0.0 && state[2] == 0.0 && state[3] == 0.0)
        );
        assert!(no_phase.mass.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn validation_paired_continuations_share_one_front_and_preserve_total_condensate() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let terrain = terrain_from(resolution, |_| -0.1);
        let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(0.0001);
            ([tangent[0] / length, 0.0, tangent[2] / length, 0.0], 1013.0)
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let first = pipeline.paired_validation_continuation(
            &gpu,
            snapshot(resolution),
            &terrain,
            &dynamics,
            snapshot(resolution),
            &terrain,
            &dynamics,
        );
        let second = pipeline.paired_validation_continuation(
            &gpu,
            snapshot(resolution),
            &terrain,
            &dynamics,
            snapshot(resolution),
            &terrain,
            &dynamics,
        );
        assert_eq!(first.advected_mass, second.advected_mass);
        assert_eq!(first.full_mass, second.full_mass);
        let total = |mass: &[f32]| {
            mass.chunks_exact(4)
                .map(|pixel| pixel[0] + pixel[1] + pixel[2])
                .sum::<f32>()
        };
        assert!(total(&first.full_mass) >= total(&first.advected_mass));
    }

    #[test]
    fn land_only_zero_front_cannot_form_condensate() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let terrain = terrain_from(resolution, |_| -0.1);
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 1.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let state = vec![0.0; resolution as usize * resolution as usize * 6 * 4];
        let (_, mass) = pipeline.validation_spinup_state(
            &gpu,
            snapshot(resolution),
            &terrain,
            &dynamics,
            Some(&state),
            0,
            1,
        );
        assert!(mass.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn land_humidity_seed_is_vapor_only_finite_and_respects_zero_controls() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| 0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 0.65], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let run = |snapshot, flags, iterations| {
            let weather = pipeline.create_textures(&gpu, resolution);
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot,
                &weather,
                SpinupTestConfig {
                    iterations,
                    diagnostic_flags: flags,
                    ..Default::default()
                },
            )
        };

        let seeded = run(snapshot(resolution), 0, 0);
        assert!(seeded.state.chunks_exact(4).all(|state| {
            state[0].is_finite()
                && state[0] > 0.0
                && state[0] < 0.68
                && state[1] == 0.0
                && state[2] == 0.0
                && state[3] == 0.0
        }));
        assert!(seeded.mass.iter().all(|value| *value == 0.0));

        let mut dry = snapshot(resolution);
        dry.moisture = 0.0;
        assert!(run(dry, 0, 0).state.iter().all(|value| *value == 0.0));
        assert!(
            run(snapshot(resolution), SPINUP_DIAGNOSTIC_NO_SOURCE, 0)
                .state
                .iter()
                .all(|value| *value == 0.0)
        );

        let mut cold = snapshot(resolution);
        // Keep even the +15 C equator below the frozen source cutoff.
        cold.base_temp_c = -50.0;
        let inland_dynamics =
            wind.create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 1.0], 1013.0));
        let weather = pipeline.create_textures(&gpu, resolution);
        let cold_inland = run_spinup_pass_with_config(
            &gpu,
            &pipeline,
            &terrain,
            &inland_dynamics,
            cold,
            &weather,
            SpinupTestConfig::default(),
        );
        assert!(cold_inland.state.iter().all(|value| *value == 0.0));
    }

    #[test]
    fn finite_land_vapor_depletes_without_a_marine_source() {
        let resolution = 16;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
            let projection = pos[0];
            (
                [
                    (1.0 - pos[0] * projection) * 0.8,
                    -pos[1] * projection * 0.8,
                    -pos[2] * projection * 0.8,
                    1.0,
                ],
                1013.0,
            )
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let weather = pipeline.create_textures(&gpu, resolution);
        let pass = run_spinup_pass_with_config(
            &gpu,
            &pipeline,
            &terrain,
            &dynamics,
            snapshot(resolution),
            &weather,
            SpinupTestConfig {
                iterations: SPINUP_ITERATIONS,
                diagnostic_flags: SPINUP_DIAGNOSTIC_NO_SOURCE,
                initial_state: Some([0.50, 0.0, 0.0, 0.0]),
                ..Default::default()
            },
        );
        assert!(pass.state.iter().all(|value| value.is_finite()));
        assert!(
            pass.state
                .chunks_exact(4)
                .map(|state| state[0])
                .sum::<f32>()
                < 0.50 * (resolution * resolution * 6) as f32,
            "land vapor did not deplete without marine supply"
        );
    }

    #[test]
    fn open_ocean_recharges_across_wind_strengths_while_coastal_land_stays_bounded() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let ocean = terrain_from(resolution, |_| -0.1);
        let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(0.0001);
            (
                [tangent[0] / length, 0.0, tangent[2] / length, 0.65],
                1013.0,
            )
        });
        let flags = SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        let run = |terrain: &TectonicTerrain, wind_scale: f32| {
            let weather = pipeline.create_textures(&gpu, resolution);
            let mut params = snapshot(resolution);
            params.seed = 31;
            params.wind_scale = wind_scale;
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                terrain,
                &dynamics,
                params,
                &weather,
                SpinupTestConfig {
                    iterations: SPINUP_ITERATIONS,
                    diagnostic_flags: flags,
                    initial_state: Some([0.01, 0.0, 0.0, 0.0]),
                    ..Default::default()
                },
            )
            .state
        };
        let mean_vapor = |values: &[f32], include: fn([f32; 3]) -> bool| {
            let (sum, count) = values
                .chunks_exact(4)
                .enumerate()
                .filter_map(|(index, state)| {
                    let face = index / (resolution * resolution) as usize;
                    let pixel = index % (resolution * resolution) as usize;
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        (pixel % resolution as usize) as f32 / (resolution - 1) as f32,
                        (pixel / resolution as usize) as f32 / (resolution - 1) as f32,
                    );
                    include(pos).then_some(state[0])
                })
                .fold((0.0, 0usize), |(sum, count), vapor| {
                    (sum + vapor, count + 1)
                });
            sum / count.max(1) as f32
        };
        let fields = [0.0, 1.0, 2.0].map(|scale| run(&ocean, scale));
        let ocean_mean = fields.each_ref().map(|field| mean_vapor(field, |_| true));
        assert!(
            ocean_mean.iter().all(|mean| *mean > 0.03),
            "open-ocean vapor did not replenish: {ocean_mean:?}"
        );
        assert!(
            ocean_mean
                .iter()
                .all(|mean| (mean - ocean_mean[0]).abs() <= 0.02),
            "wind left an ocean-wide source gap: {ocean_mean:?}"
        );
        let coast = terrain_from(resolution, |pos| if pos[2] < 0.0 { -0.1 } else { 0.1 });
        let coastal = run(&coast, 0.0);
        let ocean_vapor = mean_vapor(&coastal, |pos| pos[2] < -0.1);
        let land_vapor = mean_vapor(&coastal, |pos| pos[2] > 0.1);
        assert!(ocean_vapor > 0.03, "transitional ocean vapor={ocean_vapor}");
        // FE-084: coastal land vapor stays bounded well under the adjacent
        // ocean source. No floor above the init seed: this fixture's cool land
        // (~12 °C) sits low in the 6–22 °C ET window, so its climate ceiling
        // can sit below the bootstrap seed — real evapotranspiration self-
        // regulates to zero net flux there. Active minting is pinned by the
        // provenance tests (warm land) instead.
        assert!(
            land_vapor <= 0.5 * ocean_vapor,
            "land recharge unbounded relative to ocean: {land_vapor} vs {ocean_vapor}"
        );

        // Central angular spread is invariant under a rigid translation/rotation
        // of the whole vapor field; wind must change its organization instead.
        let organization = |values: &[f32]| {
            let (center, total) = values.chunks_exact(4).enumerate().fold(
                ([0.0; 3], 0.0),
                |(mut center, total), (index, state)| {
                    let face = index / (resolution * resolution) as usize;
                    let pixel = index % (resolution * resolution) as usize;
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        (pixel % resolution as usize) as f32 / (resolution - 1) as f32,
                        (pixel / resolution as usize) as f32 / (resolution - 1) as f32,
                    );
                    for axis in 0..3 {
                        center[axis] += pos[axis] * state[0];
                    }
                    (center, total + state[0])
                },
            );
            let length = center.iter().map(|value| value * value).sum::<f32>().sqrt();
            let direction = center.map(|value| value / length.max(0.0001));
            values
                .chunks_exact(4)
                .enumerate()
                .map(|(index, state)| {
                    let face = index / (resolution * resolution) as usize;
                    let pixel = index % (resolution * resolution) as usize;
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        (pixel % resolution as usize) as f32 / (resolution - 1) as f32,
                        (pixel / resolution as usize) as f32 / (resolution - 1) as f32,
                    );
                    let alignment = pos
                        .iter()
                        .zip(direction)
                        .map(|(position, direction)| position * direction)
                        .sum::<f32>();
                    state[0] * (1.0 - alignment)
                })
                .sum::<f32>()
                / total.max(0.0001)
        };
        let coastal_wind = [1.0, 2.0].map(|scale| run(&coast, scale));
        let organization_delta =
            (organization(&coastal_wind[0]) - organization(&coastal_wind[1])).abs();
        assert!(
            organization_delta > 0.0005,
            "wind only translated vapor instead of changing coastal organization: {organization_delta}"
        );
    }

    #[test]
    fn coastal_ocean_source_is_local_and_land_vapor_is_transport_traced() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let coast = terrain_from(resolution, |pos| if pos[2] < 0.0 { -0.1 } else { 0.1 });
        let land = terrain_from(resolution, |_| 0.1);
        let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
            let target = [0.0, 0.0, 1.0];
            let projection = pos[2];
            (
                [
                    (target[0] - pos[0] * projection) * 0.8,
                    (target[1] - pos[1] * projection) * 0.8,
                    (target[2] - pos[2] * projection) * 0.8,
                    0.65,
                ],
                1013.0,
            )
        });
        let flags = SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE
            | SPINUP_DIAGNOSTIC_NO_SINK
            | SPINUP_DIAGNOSTIC_NO_RELAXATION;
        fn zero(_: [f32; 3]) -> [f32; 4] {
            [0.0; 4]
        }
        fn ocean_tracer(pos: [f32; 3]) -> [f32; 4] {
            if pos[2] < -0.15 {
                [0.25, 0.0, 0.0, 0.0]
            } else {
                [0.0; 4]
            }
        }
        let run = |terrain: &TectonicTerrain, diagnostic_flags, initial_state_at| {
            let weather = pipeline.create_textures(&gpu, resolution);
            let mut params = snapshot(resolution);
            params.seed = 31;
            params.wind_scale = 1.0;
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                terrain,
                &dynamics,
                params,
                &weather,
                SpinupTestConfig {
                    iterations: SPINUP_ITERATIONS,
                    diagnostic_flags,
                    initial_state_at: Some(initial_state_at),
                    ..Default::default()
                },
            )
            .state
        };
        let mean_vapor = |values: &[f32], include: fn([f32; 3]) -> bool| {
            let (sum, count) = values
                .chunks_exact(4)
                .enumerate()
                .filter_map(|(index, state)| {
                    let face = index / (resolution * resolution) as usize;
                    let pixel = index % (resolution * resolution) as usize;
                    let pos = crate::cube_sphere::cube_to_sphere(
                        face as u32,
                        (pixel % resolution as usize) as f32 / (resolution - 1) as f32,
                        (pixel / resolution as usize) as f32 / (resolution - 1) as f32,
                    );
                    include(pos).then_some(state[0])
                })
                .fold((0.0, 0usize), |(sum, count), vapor| {
                    (sum + vapor, count + 1)
                });
            sum / count.max(1) as f32
        };
        let sourced = run(&coast, flags, zero);
        let no_source = run(&coast, flags | SPINUP_DIAGNOSTIC_NO_SOURCE, zero);
        // This control carries pre-existing ocean vapor only: any land vapor is
        // transport, never a local source.
        let traced = run(&coast, flags | SPINUP_DIAGNOSTIC_NO_SOURCE, ocean_tracer);
        let land_only = run(&land, flags, zero);
        let ocean_source = mean_vapor(&sourced, |pos| pos[2] < -0.15);
        let downwind_land = mean_vapor(&sourced, |pos| pos[2] > 0.15);
        let traced_land = mean_vapor(&traced, |pos| pos[2] > 0.15);

        assert!(ocean_source > 0.03, "coastal ocean source={ocean_source}");
        assert!(downwind_land > 0.0001, "source did not reach downwind land");
        assert!(
            traced_land > 0.0001,
            "ocean tracer did not cross onto downwind land: {traced_land}"
        );
        assert!(no_source.iter().all(|value| *value == 0.0));
        // FE-084 doctrine change: land self-recharges through BOUNDED
        // evapotranspiration now, so an all-land world gains a small local
        // source — far below open-ocean strength — while the NO_SOURCE
        // diagnostic still zeroes everything exactly.
        let land_no_source = run(&land, flags | SPINUP_DIAGNOSTIC_NO_SOURCE, zero);
        let land_recharge = mean_vapor(&land_only, |_| true);
        assert!(
            land_recharge > 0.0 && land_recharge <= 0.02,
            "land evapotranspiration dead or unbounded: {land_recharge}"
        );
        assert!(
            land_no_source.iter().all(|value| *value == 0.0),
            "NO_SOURCE did not kill land evapotranspiration"
        );
    }

    #[test]
    fn phase_partition_turbulence_is_deterministic_and_area_mass_neutral() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain_from(resolution, |_| -0.1);
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind.create_test_textures(&gpu, resolution, |pos| {
            let projection = pos[0];
            (
                [
                    (1.0 - pos[0] * projection) * 0.8,
                    -pos[1] * projection * 0.8,
                    -pos[2] * projection * 0.8,
                    1.0,
                ],
                1013.0,
            )
        });
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let run = || {
            let weather = pipeline.create_textures(&gpu, resolution);
            run_spinup_pass_with_config(
                &gpu,
                &pipeline,
                &terrain,
                &dynamics,
                snapshot(resolution),
                &weather,
                SpinupTestConfig {
                    iterations: SPINUP_ITERATIONS,
                    diagnostic_flags: SPINUP_DIAGNOSTIC_NO_SOURCE
                        | SPINUP_DIAGNOSTIC_NO_SINK
                        | SPINUP_DIAGNOSTIC_NO_RELAXATION,
                    initial_state: Some([0.50, 0.0, 0.0, 0.0]),
                    ..Default::default()
                },
            )
            .state
        };
        let first = run();
        let second = run();
        assert_eq!(first, second);
        assert!(
            first
                .chunks_exact(4)
                .any(|state| state[1] > 0.0 || state[2] > 0.0),
            "fixture did not exercise phase partitioning"
        );
        let weighted_mass = |state: &[f32]| {
            state
                .chunks_exact(4)
                .enumerate()
                .map(|(index, pixel)| {
                    let x =
                        (index % resolution as usize) as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let y = ((index / resolution as usize) % resolution as usize) as f32
                        / (resolution - 1) as f32
                        * 2.0
                        - 1.0;
                    pixel.iter().sum::<f32>() * (1.0 + x * x + y * y).powf(-1.5)
                })
                .sum::<f32>()
        };
        let initial = 0.50
            * (0..6)
                .flat_map(|_| {
                    (0..resolution).flat_map(move |y| (0..resolution).map(move |x| (x, y)))
                })
                .map(|(x, y)| {
                    let x = x as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    let y = y as f32 / (resolution - 1) as f32 * 2.0 - 1.0;
                    (1.0 + x * x + y * y).powf(-1.5)
                })
                .sum::<f32>();
        let drift = (weighted_mass(&first) - initial).abs() / initial;
        assert!(drift <= 0.02, "area-weighted mass drift={drift}");
    }

    #[derive(Clone, Copy)]
    struct PhaseBudgetFixture {
        q_sat: f32,
        q_target: f32,
        vapor: f32,
        low: f32,
        deep: f32,
        high: f32,
        lift: f32,
        thermal: f32,
        convergence: f32,
        frontal_lift: f32,
        terrain_wind_support: f32,
        marine: f32,
        catalyst: f32,
        source_enabled: bool,
        source_envelope: f32,
        moisture: f32,
        phase_budget: f32,
        step_fraction: f32,
    }

    #[derive(Clone, Copy)]
    struct PhaseBudgetTransfer {
        recharge: f32,
        storm_recharge: f32,
        condensed: f32,
        orographic_condensation: f32,
        deep_partition: f32,
        catalyst_transfer: f32,
        physical_eligibility: f32,
        organizing_eligibility: f32,
        target_deep_fraction: f32,
        detrainment: f32,
        rainout: f32,
        sublimation: f32,
        before: f32,
        after_rainout: f32,
        deep_response: f32,
    }

    fn smooth_step(edge0: f32, edge1: f32, value: f32) -> f32 {
        let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }

    fn inland_phase_provenance(vapor: f32, q_sat: f32, marine: f32, rain_shadow: f32) -> f32 {
        let transported = smooth_step(0.30, 0.70, vapor / q_sat.max(0.0001));
        (transported * (1.0 - marine) + marine)
            * (1.0 - (1.0 - marine) * smooth_step(0.0001, 0.01, rain_shadow))
    }

    #[test]
    fn lee_gate_preserves_transport_supported_windward_land_and_suppresses_lee_phase_creation() {
        let windward = inland_phase_provenance(0.60, 0.68, 0.0, 0.0);
        let lee = inland_phase_provenance(0.60, 0.68, 0.0, 0.20);
        let marine_lee = inland_phase_provenance(0.60, 0.68, 1.0, 0.20);

        assert!(windward > 0.99, "windward={windward}");
        assert_eq!(lee, 0.0, "lee={lee}");
        assert_eq!(marine_lee, 1.0, "marine_lee={marine_lee}");
    }

    fn phase_budget_fixture(input: PhaseBudgetFixture) -> PhaseBudgetTransfer {
        let before = input.vapor + input.low + input.deep + input.high;
        let recharge = if input.source_enabled {
            (input.q_target - input.vapor).max(0.0)
                * (0.006 + (0.030 - 0.006) * input.marine)
                * input.step_fraction
        } else {
            0.0
        };
        let mut vapor = input.vapor + recharge;
        let mut low = input.low;
        let mut deep = input.deep;
        let mut high = input.high;
        let cold = 1.0 - input.thermal;
        let lcl_lift = (input.convergence * 0.70
            + input.lift * 1.75
            + input.frontal_lift * 0.35
            + input.marine * (0.04 + cold * 0.16))
            .clamp(0.0, 1.0);
        let pre_storm_humidity = smooth_step(0.45, 0.95, vapor / input.q_sat.max(0.0001));
        let pre_storm_warm_gate = input.thermal * smooth_step(0.10, 0.20, lcl_lift);
        let pre_storm_physical_eligibility =
            pre_storm_warm_gate * smooth_step(0.12, 0.75, lcl_lift) * pre_storm_humidity;
        let storm_recharge = if input.source_enabled
            && input.catalyst > 0.0
            && input.source_envelope > 0.0
            && input.moisture > 0.0
            && pre_storm_physical_eligibility > 0.0
        {
            let catalyst_activation = input.catalyst * input.source_envelope;
            let organizing = smooth_step(0.0, 0.025, pre_storm_physical_eligibility);
            let storm_gate = catalyst_activation * organizing * input.phase_budget;
            let fraction =
                ((0.06 + (0.18 - 0.06) * input.marine) * storm_gate * input.step_fraction)
                    .clamp(0.0, 0.24);
            (input.q_target - vapor).max(0.0) * fraction
        } else {
            0.0
        };
        vapor += storm_recharge;
        let humidity = smooth_step(0.45, 0.95, vapor / input.q_sat.max(0.0001));
        let warm_gate = input.thermal * smooth_step(0.10, 0.20, lcl_lift);
        let convection = (lcl_lift + input.catalyst * humidity * warm_gate * 0.40).clamp(0.0, 1.0);
        let q_lcl = (input.q_sat * (1.0 - (0.08 + (0.42 - 0.08) * convection)))
            .min(input.q_target * 0.70)
            * (1.0 - input.lift * 0.65);
        let inland_provenance = (smooth_step(0.30, 0.70, vapor / input.q_sat.max(0.0001))
            * (1.0 - input.marine)
            + input.marine)
            .clamp(0.0, 1.0);
        let condensed = ((vapor - q_lcl).max(0.0)
            * (0.16 + (0.56 - 0.16) * convection)
            * input.phase_budget
            * inland_provenance)
            .min(vapor);
        let physical_eligibility = warm_gate * smooth_step(0.12, 0.75, lcl_lift) * humidity;
        let final_eligibility = input.source_envelope * physical_eligibility;
        let deep_fraction =
            (physical_eligibility * (0.30 + input.catalyst * 0.45)).clamp(0.0, 0.75);
        let deep_partition = condensed * deep_fraction;
        vapor -= condensed;
        low += condensed - deep_partition;
        deep += deep_partition;
        let orographic_condensation =
            (vapor * input.lift * input.terrain_wind_support * input.phase_budget * 0.28)
                .min(vapor);
        let orographic_deep_fraction = if input.catalyst > 0.0 {
            deep_fraction
        } else {
            0.0
        };
        vapor -= orographic_condensation;
        low += orographic_condensation * (1.0 - orographic_deep_fraction);
        deep += orographic_condensation * orographic_deep_fraction;
        let evaporation = low.min((input.q_target - vapor).max(0.0) * 0.012);
        vapor += evaporation;
        low -= evaporation;
        let catalyst_activation = input.catalyst * input.source_envelope;
        let q_up = input.q_sat * (1.0 - 0.79 * input.catalyst);
        let vapor_excess = (vapor - q_up).max(0.0);
        let total_condensate = vapor_excess + low + deep;
        let organizing_lift = lcl_lift.max(input.catalyst * 0.20);
        let organizing_eligibility = input.thermal
            * smooth_step(0.10, 0.20, organizing_lift)
            * smooth_step(0.12, 0.75, organizing_lift)
            * humidity;
        let organizing = smooth_step(0.0, 0.025, organizing_eligibility);
        let target_deep_fraction = (0.30 * physical_eligibility
            + 0.70 * catalyst_activation * organizing)
            .clamp(0.0, 0.92);
        let deep_demand = (target_deep_fraction * total_condensate - deep)
            .max(0.0)
            .min(vapor_excess + low);
        let catalyst_transfer =
            (1.0 - (-2.0 * catalyst_activation * input.step_fraction).exp()) * deep_demand;
        let vapor_transfer = vapor_excess.min(catalyst_transfer);
        let low_transfer = catalyst_transfer - vapor_transfer;
        vapor -= vapor_transfer;
        low -= low_transfer;
        deep += catalyst_transfer;
        let detrainment = deep.min(deep * final_eligibility * (0.18 + input.frontal_lift * 0.12));
        deep -= detrainment;
        high += detrainment;
        let condensate = low + deep;
        let rainout = ((condensate - input.q_target).max(0.0) * 0.22
            + deep * (0.01 + 0.08 * input.thermal)
            + low * input.marine * cold * 0.055)
            .min(condensate);
        let rainout_scale = rainout / condensate.max(0.0001);
        low *= 1.0 - rainout_scale;
        deep *= 1.0 - rainout_scale;
        let sublimation = (high * 0.005 * input.step_fraction).min(1.0 - vapor);
        high -= sublimation;
        vapor += sublimation;
        PhaseBudgetTransfer {
            recharge,
            storm_recharge,
            condensed,
            orographic_condensation,
            deep_partition,
            catalyst_transfer,
            physical_eligibility,
            organizing_eligibility,
            target_deep_fraction,
            detrainment,
            rainout,
            sublimation,
            before,
            after_rainout: vapor + low + deep + high,
            deep_response: deep,
        }
    }

    #[test]
    fn phase_budget_fixture_transfers_every_reservoir_and_conserves_the_column() {
        let result = phase_budget_fixture(PhaseBudgetFixture {
            q_sat: 0.68,
            q_target: 0.60,
            vapor: 0.599,
            low: 0.80,
            deep: 0.10,
            high: 0.05,
            lift: 0.80,
            thermal: 0.80,
            convergence: 0.80,
            frontal_lift: 0.0,
            terrain_wind_support: 1.0,
            marine: 1.0,
            catalyst: 1.0,
            source_enabled: true,
            source_envelope: 1.0,
            moisture: 1.0,
            phase_budget: 1.0,
            step_fraction: 1.0,
        });
        println!(
            "phase fixture recharge={:.6} condense={:.6} catalyst_transfer={:.6} terrain={:.6} partition={:.6} detrain={:.6} rain={:.6} deep={:.6}",
            result.recharge,
            result.condensed,
            result.catalyst_transfer,
            result.orographic_condensation,
            result.deep_partition,
            result.detrainment,
            result.rainout,
            result.deep_response,
        );
        assert!(result.recharge > 0.0);
        assert!(result.storm_recharge > 0.0);
        assert!(result.condensed > 0.0);
        assert!(result.catalyst_transfer > 0.0);
        assert!(result.orographic_condensation > 0.0);
        assert!(result.detrainment > 0.0 && result.rainout > 0.0);
        assert!(result.deep_response >= 0.02);
        assert!(result.after_rainout >= 0.0);
        assert!(
            ((result.after_rainout + result.rainout)
                - (result.before + result.recharge + result.storm_recharge))
                .abs()
                <= 0.000_001,
            "phase budget did not conserve its column: before={:.6}, after={:.6}, rain={:.6}",
            result.before,
            result.after_rainout,
            result.rainout,
        );

        let dry = phase_budget_fixture(PhaseBudgetFixture {
            q_sat: 0.0,
            q_target: 0.0,
            vapor: 0.0,
            low: 0.0,
            deep: 0.0,
            high: 0.0,
            lift: 0.0,
            thermal: 0.0,
            convergence: 0.0,
            frontal_lift: 0.0,
            terrain_wind_support: 0.0,
            marine: 0.0,
            catalyst: 0.0,
            source_enabled: false,
            source_envelope: 0.0,
            moisture: 0.0,
            phase_budget: 0.0,
            step_fraction: 1.0,
        });
        assert_eq!(dry.recharge, 0.0);
        assert_eq!(dry.storm_recharge, 0.0);
        assert_eq!(dry.condensed, 0.0);
        assert_eq!(dry.orographic_condensation, 0.0);
        assert_eq!(dry.deep_partition, 0.0);
        assert_eq!(dry.catalyst_transfer, 0.0);
        assert_eq!(dry.target_deep_fraction, 0.0);
        assert_eq!(dry.detrainment, 0.0);
        assert_eq!(dry.rainout, 0.0);
        assert_eq!(dry.sublimation, 0.0);

        let calm = phase_budget_fixture(PhaseBudgetFixture {
            terrain_wind_support: 0.0,
            ..PhaseBudgetFixture {
                q_sat: 0.68,
                q_target: 0.60,
                vapor: 0.599,
                low: 0.80,
                deep: 0.0,
                high: 0.0,
                lift: 0.80,
                thermal: 0.80,
                convergence: 0.80,
                frontal_lift: 0.0,
                terrain_wind_support: 1.0,
                marine: 0.0,
                catalyst: 0.0,
                source_enabled: true,
                source_envelope: 1.0,
                moisture: 1.0,
                phase_budget: 1.0,
                step_fraction: 1.0,
            }
        });
        assert_eq!(calm.orographic_condensation, 0.0);
        assert_eq!(calm.catalyst_transfer, 0.0);
        assert_eq!(calm.storm_recharge, 0.0);

        let source_ineligible = phase_budget_fixture(PhaseBudgetFixture {
            source_envelope: 0.0,
            catalyst: 1.0,
            ..PhaseBudgetFixture {
                q_sat: 0.68,
                q_target: 0.60,
                vapor: 0.599,
                low: 0.80,
                deep: 0.0,
                high: 0.0,
                lift: 0.80,
                thermal: 0.80,
                convergence: 0.80,
                frontal_lift: 0.0,
                terrain_wind_support: 1.0,
                marine: 1.0,
                catalyst: 1.0,
                source_enabled: true,
                source_envelope: 1.0,
                moisture: 1.0,
                phase_budget: 1.0,
                step_fraction: 1.0,
            }
        });
        assert_eq!(source_ineligible.catalyst_transfer, 0.0);
        assert_eq!(source_ineligible.storm_recharge, 0.0);

        let sources_disabled = phase_budget_fixture(PhaseBudgetFixture {
            source_enabled: false,
            q_sat: 0.68,
            q_target: 0.60,
            vapor: 0.599,
            low: 0.80,
            deep: 0.0,
            high: 0.0,
            lift: 0.80,
            thermal: 0.80,
            convergence: 0.80,
            frontal_lift: 0.0,
            terrain_wind_support: 1.0,
            marine: 1.0,
            catalyst: 1.0,
            source_envelope: 1.0,
            moisture: 1.0,
            phase_budget: 1.0,
            step_fraction: 1.0,
        });
        assert_eq!(sources_disabled.recharge, 0.0);
        assert_eq!(sources_disabled.storm_recharge, 0.0);
        assert!(
            ((sources_disabled.after_rainout + sources_disabled.rainout) - sources_disabled.before)
                .abs()
                <= 0.000_001,
            "catalyst changed no-source column mass"
        );

        let physically_ineligible = phase_budget_fixture(PhaseBudgetFixture {
            q_sat: 0.68,
            q_target: 0.60,
            vapor: 0.60,
            low: 0.80,
            deep: 0.0,
            high: 0.0,
            lift: 0.80,
            thermal: 0.0,
            convergence: 0.80,
            frontal_lift: 0.0,
            terrain_wind_support: 1.0,
            marine: 1.0,
            catalyst: 1.0,
            source_enabled: true,
            source_envelope: 1.0,
            moisture: 1.0,
            phase_budget: 1.0,
            step_fraction: 1.0,
        });
        assert_eq!(physically_ineligible.catalyst_transfer, 0.0);
        assert_eq!(physically_ineligible.storm_recharge, 0.0);
        assert_eq!(physically_ineligible.organizing_eligibility, 0.0);
        assert_eq!(physically_ineligible.target_deep_fraction, 0.0);
        assert!(
            ((physically_ineligible.after_rainout + physically_ineligible.rainout)
                - (physically_ineligible.before
                    + physically_ineligible.recharge
                    + physically_ineligible.storm_recharge))
                .abs()
                <= 0.000_001
        );

        let low_positive = phase_budget_fixture(PhaseBudgetFixture {
            q_sat: 0.68,
            q_target: 0.60,
            vapor: 0.68,
            low: 0.80,
            deep: 0.0,
            high: 0.0,
            lift: 0.80,
            thermal: 0.01,
            convergence: 0.80,
            frontal_lift: 0.0,
            terrain_wind_support: 1.0,
            marine: 1.0,
            catalyst: 1.0,
            source_enabled: true,
            source_envelope: 1.0,
            moisture: 1.0,
            phase_budget: 1.0,
            step_fraction: 1.0,
        });
        assert!(low_positive.physical_eligibility > 0.0);
        assert!(low_positive.physical_eligibility < 0.025);
        assert!(low_positive.target_deep_fraction > 0.30 * low_positive.physical_eligibility);
    }

    #[test]
    fn phase_budget_relaxation_sublimates_high_mass_to_vapor_conservatively() {
        let result = phase_budget_fixture(PhaseBudgetFixture {
            q_sat: 1.0,
            q_target: 0.3,
            vapor: 0.3,
            low: 0.0,
            deep: 0.0,
            high: 0.4,
            lift: 0.0,
            thermal: 0.0,
            convergence: 0.0,
            frontal_lift: 0.0,
            terrain_wind_support: 0.0,
            marine: 0.0,
            catalyst: 0.0,
            source_enabled: false,
            source_envelope: 0.0,
            moisture: 0.0,
            phase_budget: 0.0,
            step_fraction: 1.0,
        });

        assert!((result.sublimation - 0.002).abs() <= 1e-6);
        assert!(((result.after_rainout + result.rainout) - result.before).abs() <= 1e-6);
    }

    #[test]
    fn spinup_relaxation_returns_high_mass_to_vapor_conservatively() {
        let resolution = 8;
        let gpu = GpuContext::new().expect("GPU init failed");
        let terrain = terrain(resolution);
        let wind_pipeline = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let dynamics = wind_pipeline
            .create_test_textures(&gpu, resolution, |_| ([0.0, 0.0, 0.0, 0.0], 1013.0));
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let weather = pipeline.create_textures(&gpu, resolution);
        let state = run_spinup_pass_with_config(
            &gpu,
            &pipeline,
            &terrain,
            &dynamics,
            snapshot(resolution),
            &weather,
            SpinupTestConfig {
                iterations: 1,
                diagnostic_flags: SPINUP_DIAGNOSTIC_NO_SOURCE
                    | SPINUP_DIAGNOSTIC_NO_SINK
                    | SPINUP_DIAGNOSTIC_NO_PHASE_CHANGE,
                initial_state: Some([0.3, 0.0, 0.0, 0.4]),
                ..Default::default()
            },
        )
        .state;
        let initial = 0.7;
        for pixel in state.chunks_exact(4) {
            assert!(((pixel[0] + pixel[3]) - initial).abs() <= 0.001);
            assert!(pixel[0] > 0.3 && pixel[3] < 0.4, "state={pixel:?}");
        }
    }

    // === DS-045 A1 metrics: cloud boundaries must emerge from flow/history,
    // not static per-cell land classification. ===

    fn sphere_to_face_uv_test(dir: [f32; 3]) -> (u32, f32, f32) {
        let a = [dir[0].abs(), dir[1].abs(), dir[2].abs()];
        if a[0] >= a[1] && a[0] >= a[2] {
            return if dir[0] > 0.0 {
                (0, -dir[2] / a[0] * 0.5 + 0.5, -dir[1] / a[0] * 0.5 + 0.5)
            } else {
                (1, dir[2] / a[0] * 0.5 + 0.5, -dir[1] / a[0] * 0.5 + 0.5)
            };
        }
        if a[1] >= a[0] && a[1] >= a[2] {
            return if dir[1] > 0.0 {
                (2, dir[0] / a[1] * 0.5 + 0.5, dir[2] / a[1] * 0.5 + 0.5)
            } else {
                (3, dir[0] / a[1] * 0.5 + 0.5, -dir[2] / a[1] * 0.5 + 0.5)
            };
        }
        if dir[2] > 0.0 {
            (4, dir[0] / a[2] * 0.5 + 0.5, -dir[1] / a[2] * 0.5 + 0.5)
        } else {
            (5, -dir[0] / a[2] * 0.5 + 0.5, -dir[1] / a[2] * 0.5 + 0.5)
        }
    }

    fn state_at(state: &[f32], res: u32, dir: [f32; 3]) -> [f32; 4] {
        let (face, u, v) = sphere_to_face_uv_test(dir);
        let x = ((u * (res - 1) as f32).round() as u32).min(res - 1);
        let y = ((v * (res - 1) as f32).round() as u32).min(res - 1);
        let index = ((face * res * res + y * res + x) * 4) as usize;
        [
            state[index],
            state[index + 1],
            state[index + 2],
            state[index + 3],
        ]
    }

    fn condensate_at(state: &[f32], res: u32, dir: [f32; 3]) -> f32 {
        let s = state_at(state, res, dir);
        s[1] + s[2] + s[3]
    }

    fn angle_between(a: [f32; 3], b: [f32; 3]) -> f32 {
        a.iter()
            .zip(b)
            .map(|(x, y)| x * y)
            .sum::<f32>()
            .clamp(-1.0, 1.0)
            .acos()
    }

    fn normalize_dir(dir: [f32; 3]) -> [f32; 3] {
        let length = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        [dir[0] / length, dir[1] / length, dir[2] / length]
    }

    /// Zonal dynamics shared by the DS-045 fixtures (mirrors the coastal gates).
    fn ds045_dynamics(
        wind: &WindFieldPipeline,
        gpu: &GpuContext,
        resolution: u32,
    ) -> DynamicsTextures {
        wind.create_test_textures(gpu, resolution, |pos| {
            let tangent = [pos[2], 0.0, -pos[0]];
            let length = (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                .sqrt()
                .max(0.0001);
            (
                [tangent[0] / length, 0.0, tangent[2] / length, 0.65],
                1013.0,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn ds045_run(
        gpu: &GpuContext,
        pipeline: &WeatherFieldPipeline,
        dynamics: &DynamicsTextures,
        resolution: u32,
        seed: u32,
        wind_scale: f32,
        storm_count: u32,
        height: impl Fn([f32; 3]) -> f32,
    ) -> Vec<f32> {
        let terrain = terrain_from(resolution, height);
        let weather = pipeline.create_textures(gpu, resolution);
        let mut params = snapshot(resolution);
        params.seed = seed;
        params.wind_scale = wind_scale;
        params.storm_count = storm_count;
        run_spinup_pass_with_config(
            gpu,
            pipeline,
            &terrain,
            dynamics,
            params,
            &weather,
            SpinupTestConfig {
                iterations: SPINUP_ITERATIONS,
                ..Default::default()
            },
        )
        .state
    }

    /// Near-flat coast shared by DS-045 fixtures: any large height structure
    /// would add its own orographic edge and pollute the boundary metrics.
    fn ds045_shelf_coast(pos: [f32; 3]) -> f32 {
        if pos[2] < 0.0 { -0.05 } else { 0.01 }
    }

    #[test]
    fn ds045_coast_crossing_boundary_gradient_spans_many_texels() {
        let resolution = 64;
        let texel_angle = (std::f32::consts::FRAC_PI_2) / resolution as f32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = ds045_dynamics(&wind, &gpu, resolution);
        let state = ds045_run(
            &gpu,
            &pipeline,
            &dynamics,
            resolution,
            42,
            2.0,
            2,
            ds045_shelf_coast,
        );
        assert!(state.iter().all(|value| value.is_finite()));
        // Transect across the ONSHORE coast at ~15°N: zonal flow carries marine
        // air onto land at phi=pi, so any coastline-hugging artifact would live
        // on this crossing.
        let latitude = 15.0_f32.to_radians();
        let span = 40.0 * texel_angle;
        let samples = 161;
        let values: Vec<f32> = (0..samples)
            .map(|index| {
                let phi =
                    std::f32::consts::PI + span - 2.0 * span * index as f32 / (samples - 1) as f32;
                let dir = normalize_dir([
                    phi.cos() * latitude.cos(),
                    latitude.sin(),
                    phi.sin() * latitude.cos(),
                ]);
                condensate_at(&state, resolution, dir)
            })
            .collect();
        let vmax = values.iter().cloned().fold(f32::MIN, f32::max);
        let vmin = values.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            vmax - vmin > 0.002,
            "transect shows no cloud contrast across the coast: [{vmin}, {vmax}]"
        );
        // Effective gradient width of the dominant cross-coast transition
        // (marine deck falling to its termination): span between the 90% and
        // 10% crossings of the observed range. A coastline-hugging edge would
        // collapse this toward a couple of texels; flow-boundaries stay broad.
        let hi = vmin + 0.9 * (vmax - vmin);
        let lo = vmin + 0.1 * (vmax - vmin);
        let mut last_hi: Option<usize> = None;
        let mut termination: Option<usize> = None;
        for (index, value) in values.iter().enumerate() {
            if *value < lo {
                if last_hi.is_some() {
                    termination = Some(index);
                    break;
                }
            } else if *value >= hi {
                last_hi = Some(index);
            }
        }
        let arc_per_sample_texels = 2.0 * span / (samples - 1) as f32 / texel_angle;
        let width_texels = match (last_hi, termination) {
            (Some(a), Some(b)) => (b - a) as f32 * arc_per_sample_texels,
            _ => panic!("transect lacks a deck-to-clear transition to measure"),
        };
        assert!(
            width_texels >= 16.0,
            "coast-crossing gradient width {width_texels:.1} texels < 16 at max wind"
        );
    }

    #[test]
    fn ds045_lake_has_no_static_outline_beyond_wind_scaled_plume() {
        let resolution = 64;
        let texel_angle = (std::f32::consts::FRAC_PI_2) / resolution as f32;
        let lake_center = [0.0, 0.0, 1.0];
        let lake_radius = 10.0 * texel_angle;
        let height = |pos: [f32; 3]| {
            if angle_between(pos, lake_center) < lake_radius {
                -0.08
            } else {
                0.08
            }
        };
        let upwind = [-1.0_f32, 0.0, 0.0];
        let plume_extent = |values: &[f32]| -> f32 {
            let background: f32 = {
                let mut sum = 0.0;
                let mut count = 0;
                for face in 0..6u32 {
                    for y in 0..resolution {
                        for x in 0..resolution {
                            let pos = crate::cube_sphere::cube_to_sphere(
                                face,
                                x as f32 / (resolution - 1) as f32,
                                y as f32 / (resolution - 1) as f32,
                            );
                            if angle_between(pos, lake_center) > lake_radius + 30.0 * texel_angle {
                                let index = ((face * resolution * resolution + y * resolution + x)
                                    * 4) as usize;
                                sum += values[index + 1] + values[index + 2] + values[index + 3];
                                count += 1;
                            }
                        }
                    }
                }
                sum / count.max(1) as f32
            };
            let mut extent = 0.0_f32;
            let ring_count = 40;
            for ring in 1..=ring_count {
                let distance = lake_radius + ring as f32 * texel_angle;
                let mut sector_sum = 0.0;
                let mut sector_count = 0u32;
                for spoke in 0..36u32 {
                    let angle = spoke as f32 * std::f32::consts::TAU / 36.0;
                    let lateral = normalize_dir([angle.cos(), angle.sin(), 0.0]);
                    let dir = normalize_dir([
                        lake_center[0] * distance.cos() + lateral[0] * distance.sin(),
                        lake_center[1] * distance.cos() + lateral[1] * distance.sin(),
                        lake_center[2] * distance.cos() + lateral[2] * distance.sin(),
                    ]);
                    let alignment = dir.iter().zip(upwind).map(|(a, b)| a * b).sum::<f32>();
                    if alignment > 0.35 {
                        continue; // measure only the downwind/crosswind sectors
                    }
                    sector_sum += condensate_at(values, resolution, dir);
                    sector_count += 1;
                }
                if sector_count > 0
                    && sector_sum / sector_count as f32 > background + 0.35 * background.max(0.002)
                {
                    extent = ring as f32;
                }
            }
            extent
        };
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = ds045_dynamics(&wind, &gpu, resolution);
        let runs = [1.0_f32, 2.0]
            .map(|scale| ds045_run(&gpu, &pipeline, &dynamics, resolution, 42, scale, 2, height));
        // No static outline: the annulus immediately upwind of the rim must sit
        // at background level for both wind strengths.
        for (scale, state) in [1.0_f32, 2.0].iter().zip(&runs) {
            let mut halo = 0.0;
            let mut count = 0u32;
            for spoke in 0..24u32 {
                let angle = spoke as f32 * std::f32::consts::TAU / 24.0;
                let offset = normalize_dir([angle.cos(), angle.sin(), 0.0]);
                let dir = normalize_dir([
                    lake_center[0] * (lake_radius + 3.0 * texel_angle).cos()
                        + offset[0] * (lake_radius + 3.0 * texel_angle).sin(),
                    lake_center[1] * (lake_radius + 3.0 * texel_angle).cos()
                        + offset[1] * (lake_radius + 3.0 * texel_angle).sin(),
                    lake_center[2] * (lake_radius + 3.0 * texel_angle).cos(),
                ]);
                if dir.iter().zip(upwind).map(|(a, b)| a * b).sum::<f32>() < 0.5 {
                    continue;
                }
                halo += condensate_at(state, resolution, dir);
                count += 1;
            }
            let background = 0.004; // conservative floor; outline means far above this
            let mean_halo = halo / count.max(1) as f32;
            assert!(
                mean_halo <= 4.0 * background,
                "wind {scale}: upwind lake-halo outline {mean_halo:.5} exceeds background bound"
            );
        }
        let extent_low = plume_extent(&runs[0]);
        let extent_high = plume_extent(&runs[1]);
        assert!(
            extent_high >= extent_low + 2.0,
            "downwind plume does not scale with wind: {extent_low:.0} vs {extent_high:.0} rings"
        );
    }

    fn ds045_boundary_set(state: &[f32], resolution: u32, channel: usize) -> Vec<bool> {
        // Boundary strength = face-space |grad| of the selected state channel,
        // top decile. Deep convection (channel 2) carries the storm-seed
        // signature; total condensate carries coastline artifacts.
        let mut strength = vec![0.0_f32; state.len() / 4];
        for face in 0..6u32 {
            for y in 0..resolution {
                for x in 0..resolution {
                    let index =
                        ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                    let c = |dx: i32, dy: i32| {
                        let nx = (x as i32 + dx).clamp(0, resolution as i32 - 1) as u32;
                        let ny = (y as i32 + dy).clamp(0, resolution as i32 - 1) as u32;
                        let j =
                            ((face * resolution * resolution + ny * resolution + nx) * 4) as usize;
                        if channel == 4 {
                            state[j + 1] + state[j + 2] + state[j + 3]
                        } else {
                            state[j + channel]
                        }
                    };
                    strength[index / 4] = (c(1, 0) - c(-1, 0)).abs() + (c(0, 1) - c(0, -1)).abs();
                }
            }
        }
        let mut sorted = strength.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let threshold = sorted[sorted.len() / 10];
        strength
            .iter()
            .map(|s| *s >= threshold && *s > 0.0)
            .collect()
    }

    #[test]
    fn ds045_storm_seed_permutation_changes_boundary_geometry() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = ds045_dynamics(&wind, &gpu, resolution);
        let a = ds045_boundary_set(
            &ds045_run(
                &gpu,
                &pipeline,
                &dynamics,
                resolution,
                42,
                1.0,
                8,
                ds045_shelf_coast,
            ),
            resolution,
            2,
        );
        let b = ds045_boundary_set(
            &ds045_run(
                &gpu,
                &pipeline,
                &dynamics,
                resolution,
                997,
                1.0,
                8,
                ds045_shelf_coast,
            ),
            resolution,
            2,
        );
        let intersection = a.iter().zip(&b).filter(|(x, y)| **x && **y).count();
        let union = a.iter().zip(&b).filter(|(x, y)| **x || **y).count();
        let jaccard = intersection as f32 / union.max(1) as f32;
        assert!(
            a.iter().filter(|x| **x).count() > 16 && b.iter().filter(|x| **x).count() > 16,
            "boundary sets degenerate"
        );
        assert!(
            jaccard < 0.85,
            "storm-seed permutation left boundary geometry geography-locked: jaccard={jaccard:.3}"
        );
    }

    #[test]
    fn ds045_no_coastline_correlated_boundary_across_wind_sweep() {
        let resolution = 32;
        let gpu = GpuContext::new().expect("GPU init failed");
        let wind = WindFieldPipeline::new(&gpu).expect("dynamics unavailable");
        let pipeline = WeatherFieldPipeline::new(&gpu).expect("weather unavailable");
        let dynamics = ds045_dynamics(&wind, &gpu, resolution);
        for scale in [0.5_f32, 1.0, 2.0] {
            let state = ds045_run(
                &gpu,
                &pipeline,
                &dynamics,
                resolution,
                42,
                scale,
                2,
                ds045_shelf_coast,
            );
            let boundary = ds045_boundary_set(&state, resolution, 4);
            let mut xs = Vec::new();
            let mut ys = Vec::new();
            for face in 0..6u32 {
                for y in 0..resolution {
                    for x in 0..resolution {
                        let pos = crate::cube_sphere::cube_to_sphere(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        let index =
                            ((face * resolution * resolution + y * resolution + x) * 4) as usize;
                        xs.push(if boundary[index / 4] { 1.0 } else { 0.0 });
                        ys.push(1.0 - (pos[2].abs() / 0.05).min(1.0));
                    }
                }
            }
            let mean_x = xs.iter().sum::<f32>() / xs.len() as f32;
            let mean_y = ys.iter().sum::<f32>() / ys.len() as f32;
            let covariance: f32 = xs
                .iter()
                .zip(&ys)
                .map(|(x, y)| (x - mean_x) * (y - mean_y))
                .sum();
            let var_x: f32 = xs.iter().map(|x| (x - mean_x) * (x - mean_x)).sum();
            let var_y: f32 = ys.iter().map(|y| (y - mean_y) * (y - mean_y)).sum();
            let correlation = covariance / (var_x * var_y).sqrt().max(0.000001);
            assert!(
                correlation.abs() < 0.35,
                "wind {scale}: coastline-correlated boundary r={correlation:.3}"
            );
        }
    }

    #[test]
    fn fe089_vertical_transfer_is_proportional_capped_and_never_mints_water() {
        let transfer = |source: f32, rate: f32| source.min((source * rate).min(0.12));
        let owned_transfer = |source: f32, owned: f32, amount: f32| {
            if source > 0.0 && amount > 0.0 {
                amount * owned / source
            } else {
                0.0
            }
        };

        for (source, owned, rate) in [(0.0, 0.0, 0.08), (0.04, 0.01, 0.03), (8.0, 2.0, 0.035)] {
            let amount = transfer(source, rate);
            let owned_amount = owned_transfer(source, owned, amount);
            assert!(amount <= source && amount <= 0.12);
            assert!(owned_amount <= owned + f32::EPSILON);
            if source == 0.0 {
                assert_eq!(owned_amount, 0.0, "zero supply must stay exactly zero");
            }
            if source > 0.0 {
                assert!((owned_amount / amount - owned / source).abs() < 1e-6);
            }
        }

        let shader = include_str!("shaders/weather_spinup.wgsl");
        for required in [
            "const K_FT_TO_BL: f32 = 0.08;",
            "const K_BL_TO_FT: f32 = 0.03;",
            "const K_DEEP_TO_FT: f32 = 0.035;",
            "const TRANSFER_CAP_PER_SUBSTEP: f32 = 0.12;",
            "state=(q_bl,c_low,c_deep,c_high)",
            "aux=(q_ft,P_bl,P_low,P_deep)",
            "tail=(P_ft,P_high,0,0)",
        ] {
            assert!(
                shader.contains(required),
                "missing ARCH-090 baseline: {required}"
            );
        }
    }
}
