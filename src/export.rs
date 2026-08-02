use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use wgpu::util::DeviceExt;

use crate::export_staging::{
    EquirectStage, ExportStage, StageMetadata, StagedCubemapSampler, StagedEquirectRowGenerator,
};
use crate::gpu::GpuContext;
use crate::openexr_writer::AtomicScanlineExrWriter;
use crate::planet::{DerivedProperties, PlanetParams};
use crate::plates::{generate_plates, PlateGenParams};
use crate::png_writer::{AtomicScanlinePngWriter, PngRowFormat};
use crate::terrain_compute::{
    ErosionPipeline, TectonicTerrain, TerrainComputePipeline, TerrainGenParams,
    TerrainGenerationParams,
};

// ============ Constants ============

pub const DEFAULT_EXPORT_RESOLUTION: u32 = 4096;
pub const TILE_SIZE: u32 = 512;
pub const MAX_AO_STENCIL_RADIUS: u32 = 6;
pub const MIN_EMISSION_STENCIL_RADIUS: u32 = 3;
pub const MAX_8K_OWNED_LIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_OWNED_LIVE_BYTES: u64 = MAX_8K_OWNED_LIVE_BYTES;
pub const MESO_EROSION_RESOLUTION: u32 = 2048;

pub const fn erosion_resolution_for_export(face_resolution: u32) -> u32 {
    if face_resolution == 8192 {
        MESO_EROSION_RESOLUTION
    } else {
        face_resolution
    }
}
const STAGED_SAMPLE_REGION_SIDE: u32 = 512;
const STAGED_SAMPLE_CACHE_REGIONS: usize = 8;
const MATERIALIZATION_CHECKPOINT_ROW_INTERVAL: u32 = 1024;
const MAX_EQUIRECT_ROW_WORKERS: usize = 8;
const ROW_QUEUE_PER_WORKER: usize = 2;
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuantizedLayer {
    Albedo,
    Roughness,
    AmbientOcclusion,
    CloudDensity,
    WaterMask,
}

impl QuantizedLayer {
    fn png_format(self) -> PngRowFormat {
        match self {
            Self::Albedo => PngRowFormat::Rgba8,
            Self::Roughness | Self::AmbientOcclusion | Self::CloudDensity => PngRowFormat::Gray16,
            Self::WaterMask => PngRowFormat::Gray8,
        }
    }

    fn channels(self) -> u32 {
        match self {
            Self::Albedo => 4,
            Self::Roughness | Self::AmbientOcclusion | Self::CloudDensity | Self::WaterMask => 1,
        }
    }
}

pub const fn emission_stencil_radius(face_resolution: u32) -> u32 {
    let radius = face_resolution / 100;
    if radius > MIN_EMISSION_STENCIL_RADIUS {
        radius
    } else {
        MIN_EMISSION_STENCIL_RADIUS
    }
}

pub const fn max_map_stencil_radius(face_resolution: u32) -> u32 {
    let emission = emission_stencil_radius(face_resolution);
    if MAX_AO_STENCIL_RADIUS > emission {
        MAX_AO_STENCIL_RADIUS
    } else {
        emission
    }
}

// ============ Config ============

pub struct ExportLayers {
    pub height: bool,
    pub albedo: bool,
    pub normals: bool,
    pub roughness: bool,
    pub water_mask: bool,
    pub clouds: bool,
    pub emission: bool,
}

impl Default for ExportLayers {
    fn default() -> Self {
        Self {
            height: true,
            albedo: true,
            normals: true,
            roughness: true,
            water_mask: true,
            clouds: true,
            emission: true,
        }
    }
}

pub struct ExportConfig {
    pub face_resolution: u32,
    pub tile_size: u32,
    pub output_dir: PathBuf,
    pub planet_name: String,
    pub erosion_iterations: u32,
    pub season: f32,
    pub layers: ExportLayers,
    pub cloud_coverage: f32,
    pub cloud_type: f32,
    pub cloud_seed: u32,
    pub night_lights: f32,
}

// ============ Progress ============

#[derive(Clone, Debug)]
pub enum ExportProgress {
    Progress { message: String, fraction: f32 },
    Complete,
    Error(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExportTimings {
    pub generation_inclusive_ms: f64,
    pub erosion_inclusive_ms: f64,
    pub export_inclusive_ms: f64,
    pub generation_completed: bool,
    pub erosion_completed: bool,
    pub export_completed: bool,
    pub map_batch_metrics: MapBatchMetrics,
    pub meso_erosion_resolution: Option<u32>,
    pub meso_erosion_ms: f64,
    pub meso_erosion_completed: bool,
    pub delta_reconstruction_ms: f64,
    pub delta_reconstruction_completed: bool,
    pub staged_io_metrics: StagedIoMetrics,
    pub staged_io_completed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StagedIoMetrics {
    pub cache_hits: u64,
    pub region_reads: u64,
    pub rows_generated: u64,
    pub row_values: u64,
    pub peak_cached_values: usize,
    pub cache_capacity_values: usize,
    /// Wall-clock time for row materialization and intermediate staging.
    pub row_generation_ms: f64,
    /// Summed worker time; diagnostic only and never used as a wall-clock stage duration.
    pub worker_row_generation_ms: f64,
    pub output_write_ms: f64,
    pub output_finish_ms: f64,
    pub output_write_bytes: u64,
    pub published_file_bytes: u64,
    pub intermediate_write_ms: f64,
    pub intermediate_write_bytes: u64,
}

impl StagedIoMetrics {
    pub fn peak_cached_bytes(&self) -> u64 {
        u64::try_from(self.peak_cached_values)
            .unwrap_or(u64::MAX)
            .saturating_mul(std::mem::size_of::<f32>() as u64)
    }

    pub fn cache_capacity_bytes(&self) -> u64 {
        u64::try_from(self.cache_capacity_values)
            .unwrap_or(u64::MAX)
            .saturating_mul(std::mem::size_of::<f32>() as u64)
    }

    pub fn stage_wall_elapsed_ms(&self) -> f64 {
        self.row_generation_ms + self.output_write_ms + self.output_finish_ms
    }

    fn record(&mut self, metrics: Self) {
        self.cache_hits += metrics.cache_hits;
        self.region_reads += metrics.region_reads;
        self.rows_generated += metrics.rows_generated;
        self.row_values += metrics.row_values;
        self.peak_cached_values = self.peak_cached_values.max(metrics.peak_cached_values);
        self.cache_capacity_values = self
            .cache_capacity_values
            .max(metrics.cache_capacity_values);
        self.row_generation_ms += metrics.row_generation_ms;
        self.worker_row_generation_ms += metrics.worker_row_generation_ms;
        self.output_write_ms += metrics.output_write_ms;
        self.output_finish_ms += metrics.output_finish_ms;
        self.output_write_bytes += metrics.output_write_bytes;
        self.published_file_bytes += metrics.published_file_bytes;
        self.intermediate_write_ms += metrics.intermediate_write_ms;
        self.intermediate_write_bytes += metrics.intermediate_write_bytes;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerMaterializationCheckpoint {
    pub layer: &'static str,
    pub rows_completed: u32,
    pub rows_total: u32,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub region_reads: u64,
    pub peak_cached_bytes: u64,
    pub cache_capacity_bytes: u64,
    pub intermediate_write_ms: f64,
    pub intermediate_write_bytes: u64,
    pub completed: bool,
    pub reason: Option<String>,
}

pub type LayerMaterializationCheckpointSink =
    dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>;

fn discard_layer_materialization_checkpoint(
    _: &LayerMaterializationCheckpoint,
) -> Result<(), String> {
    Ok(())
}

fn record_sampler_diagnostics(
    metrics: &mut StagedIoMetrics,
    diagnostics: crate::export_staging::StagedSamplerDiagnostics,
) {
    metrics.cache_hits += diagnostics.cache_hits;
    metrics.region_reads += diagnostics.region_reads;
    metrics.rows_generated += diagnostics.rows_generated;
    metrics.row_values += diagnostics.row_values;
}

fn sampler_diagnostics_delta(
    before: crate::export_staging::StagedSamplerDiagnostics,
    after: crate::export_staging::StagedSamplerDiagnostics,
) -> crate::export_staging::StagedSamplerDiagnostics {
    crate::export_staging::StagedSamplerDiagnostics {
        cache_hits: after.cache_hits - before.cache_hits,
        region_reads: after.region_reads - before.region_reads,
        rows_generated: after.rows_generated - before.rows_generated,
        row_values: after.row_values - before.row_values,
        peak_cached_values: after.peak_cached_values,
        ..Default::default()
    }
}

fn merge_worker_cache_peaks(metrics: &mut StagedIoMetrics, worker_peaks: &Mutex<Vec<usize>>) {
    if let Ok(worker_peaks) = worker_peaks.lock() {
        metrics.peak_cached_values = worker_peaks.iter().copied().sum();
    }
}

struct ParallelRow {
    y: Option<u32>,
    result: Result<Vec<f32>, String>,
    diagnostics: crate::export_staging::StagedSamplerDiagnostics,
    elapsed: Duration,
}

impl ParallelRow {
    fn failure(message: String) -> Self {
        Self {
            y: None,
            result: Err(message),
            diagnostics: Default::default(),
            elapsed: Duration::ZERO,
        }
    }
}

#[derive(Debug)]
struct StagedExportFailure {
    message: String,
    metrics: StagedIoMetrics,
}

fn merge_staged_export(
    timings: &mut ExportTimings,
    result: Result<StagedIoMetrics, StagedExportFailure>,
) -> Result<(), String> {
    match result {
        Ok(metrics) => {
            timings.staged_io_metrics.record(metrics);
            Ok(())
        }
        Err(failure) => {
            timings.staged_io_metrics.record(failure.metrics);
            Err(failure.message)
        }
    }
}

// ============ Tile Coordinator ============

pub struct TileCoordinator {
    pub face_resolution: u32,
    pub tile_size: u32,
    pub tiles_per_axis: u32,
    pub stencil_radius: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRegion {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
}

impl TileCoordinator {
    pub fn new(face_resolution: u32, tile_size: u32) -> Self {
        Self::try_new(face_resolution, tile_size).expect("invalid tile coordinator")
    }

    pub fn try_new(face_resolution: u32, tile_size: u32) -> Result<Self, String> {
        if tile_size == 0 {
            return Err("tile size must be greater than zero".into());
        }
        if face_resolution % tile_size != 0 {
            return Err("face resolution must be a multiple of tile size".into());
        }
        Ok(Self {
            face_resolution,
            tile_size,
            tiles_per_axis: face_resolution / tile_size,
            stencil_radius: max_map_stencil_radius(face_resolution),
        })
    }

    pub fn total_tiles(&self) -> u32 {
        6 * self.tiles_per_axis * self.tiles_per_axis
    }

    pub fn tiles_per_face(&self) -> u32 {
        self.tiles_per_axis * self.tiles_per_axis
    }

    pub fn region(&self, tile_x: u32, tile_y: u32) -> TileRegion {
        let core_x = tile_x * self.tile_size;
        let core_y = tile_y * self.tile_size;
        let origin_x = core_x.saturating_sub(self.stencil_radius);
        let origin_y = core_y.saturating_sub(self.stencil_radius);
        let end_x = (core_x + self.tile_size + self.stencil_radius).min(self.face_resolution);
        let end_y = (core_y + self.tile_size + self.stencil_radius).min(self.face_resolution);
        TileRegion {
            origin_x,
            origin_y,
            width: end_x - origin_x,
            height: end_y - origin_y,
            crop_x: core_x - origin_x,
            crop_y: core_y - origin_y,
            crop_width: self.tile_size,
            crop_height: self.tile_size,
        }
    }

    fn tile_fits_device_limits(&self, limits: &wgpu::Limits, output_element_bytes: u32) -> bool {
        let side = self.tile_size + 2 * self.stencil_radius;
        let pixels = u64::from(side) * u64::from(side);
        let tile_bytes = pixels * (4 + 2 * u64::from(output_element_bytes));
        let binding_limit = u64::from(limits.max_storage_buffer_binding_size);
        tile_bytes <= limits.max_buffer_size && tile_bytes <= binding_limit
    }
}

pub fn select_tile_size(
    face_resolution: u32,
    requested_tile_size: u32,
    limits: &wgpu::Limits,
    output_element_bytes: u32,
) -> Result<u32, String> {
    if requested_tile_size == 0 || face_resolution % requested_tile_size != 0 {
        return Err("requested tile size must divide the face resolution".into());
    }
    let mut tile_size = requested_tile_size;
    loop {
        let coordinator = TileCoordinator::try_new(face_resolution, tile_size)?;
        if coordinator.tile_fits_device_limits(limits, output_element_bytes) {
            return Ok(tile_size);
        }
        if tile_size == 1 {
            return Err("correctness halo exceeds device storage limits".into());
        }
        tile_size /= 2;
        while face_resolution % tile_size != 0 {
            tile_size -= 1;
        }
    }
}

pub fn estimated_peak_streaming_bytes(face_resolution: u32, output_element_bytes: u32) -> u64 {
    let pixels = u64::from(face_resolution) * u64::from(face_resolution);
    let terrain_faces = 6 * pixels * 4;
    let face_map = pixels * u64::from(output_element_bytes);
    let equirect_layer = 2 * pixels * u64::from(output_element_bytes);
    terrain_faces + face_map + equirect_layer
}

pub fn estimated_staged_equirect_bytes(face_resolution: u32, channels: u32) -> Result<u64, String> {
    if face_resolution <= 2 || channels == 0 {
        return Err(
            "staged equirect ledger requires positive face channels and resolution above 2".into(),
        );
    }
    let (region_side, cache_regions) = staged_sampler_config(face_resolution, channels)?;
    let row_values = u64::from(face_resolution)
        .checked_mul(2)
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .ok_or("staged equirect row size overflows u64")?;
    let cache_values = u64::try_from(cache_regions)
        .map_err(|_| "staged equirect cache size exceeds u64")?
        .checked_mul(u64::from(region_side))
        .and_then(|value| value.checked_mul(u64::from(region_side)))
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .ok_or("staged equirect cache size overflows u64")?;
    let rgba_row_values = u64::from(face_resolution)
        .checked_mul(2)
        .and_then(|value| value.checked_mul(4))
        .ok_or("staged equirect RGBA row size overflows u64")?;
    row_values
        .checked_add(cache_values)
        .and_then(|value| value.checked_add(rgba_row_values))
        .and_then(|value| value.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or_else(|| "staged equirect live-byte ledger overflows u64".into())
}

fn staged_sampler_config(face_resolution: u32, channels: u32) -> Result<(u32, usize), String> {
    let region_side = STAGED_SAMPLE_REGION_SIDE.min(face_resolution - 1).max(2);
    let face_values = u64::from(face_resolution)
        .checked_mul(u64::from(face_resolution))
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .ok_or("staged sampler face size overflows u64")?;
    let region_values = u64::from(region_side)
        .checked_mul(u64::from(region_side))
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .ok_or("staged sampler region size overflows u64")?;
    let cache_regions = ((face_values - 1) / region_values)
        .min(u64::try_from(STAGED_SAMPLE_CACHE_REGIONS).unwrap_or(u64::MAX));
    let cache_regions =
        usize::try_from(cache_regions).map_err(|_| "staged sampler cache size exceeds usize")?;
    if cache_regions == 0 {
        return Err("staged sampler cache cannot remain smaller than one face".into());
    }
    Ok((region_side, cache_regions))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EquirectRowMaterializationConfig {
    workers: usize,
    queue_bound: usize,
    aggregate_cache_values: usize,
    resource_bytes: u64,
}

fn equirect_row_materialization_config(
    width: u32,
    height: u32,
    channels: u32,
    region_side: u32,
    cache_regions: usize,
    worker_cap: usize,
) -> Result<EquirectRowMaterializationConfig, String> {
    if worker_cap == 0 {
        return Err("parallel equirect worker count must be nonzero".into());
    }
    let intermediate_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .and_then(|value| value.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or("parallel equirect intermediate size overflows u64")?;
    let cache_values_per_worker = u64::try_from(cache_regions)
        .map_err(|_| "parallel equirect cache count exceeds u64")?
        .checked_mul(u64::from(region_side))
        .and_then(|value| value.checked_mul(u64::from(region_side)))
        .and_then(|value| value.checked_mul(u64::from(channels)))
        .ok_or("parallel equirect cache size overflows u64")?;
    let cache_bytes_per_worker = cache_values_per_worker
        .checked_mul(std::mem::size_of::<f32>() as u64)
        .ok_or("parallel equirect cache size overflows u64")?;
    let row_bytes = u64::from(width)
        .checked_mul(u64::from(channels))
        .and_then(|value| value.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or("parallel equirect row size overflows u64")?;
    let preferred_workers = worker_cap.min(MAX_EQUIRECT_ROW_WORKERS);
    for workers in (1..=preferred_workers).rev() {
        let queue_bound = workers
            .checked_mul(ROW_QUEUE_PER_WORKER)
            .ok_or("parallel equirect queue bound overflows usize")?;
        let worker_bytes = u64::try_from(workers)
            .map_err(|_| "parallel equirect worker count exceeds u64")?
            .checked_mul(cache_bytes_per_worker)
            .and_then(|value| value.checked_add(row_bytes))
            .and_then(|value| {
                value.checked_add(u64::try_from(queue_bound).ok()?.checked_mul(row_bytes)?)
            })
            .ok_or("parallel equirect worker resources overflow u64")?;
        let resource_bytes = intermediate_bytes
            .checked_add(worker_bytes)
            .ok_or("parallel equirect resource ledger overflows u64")?;
        if resource_bytes <= MAX_OWNED_LIVE_BYTES {
            let aggregate_cache_values = u64::try_from(workers)
                .map_err(|_| "parallel equirect worker count exceeds u64")?
                .checked_mul(cache_values_per_worker)
                .ok_or("parallel equirect aggregate cache size overflows u64")?;
            return Ok(EquirectRowMaterializationConfig {
                workers,
                queue_bound,
                aggregate_cache_values: usize::try_from(aggregate_cache_values)
                    .map_err(|_| "parallel equirect aggregate cache size exceeds usize")?,
                resource_bytes,
            });
        }
    }
    Err("parallel equirect rows exceed the 4 GiB resource budget".into())
}

pub fn estimated_staged_export_peak_bytes(
    face_resolution: u32,
    layers: &ExportLayers,
) -> Result<u64, String> {
    let terrain_faces = u64::from(face_resolution)
        .checked_mul(u64::from(face_resolution))
        .and_then(|value| value.checked_mul(6))
        .and_then(|value| value.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or("staged authoritative terrain ledger overflows u64")?;
    let height = if layers.height {
        estimated_staged_equirect_bytes(face_resolution, 1)?
    } else {
        0
    };
    let normal = if layers.normals {
        estimated_staged_equirect_bytes(face_resolution, 4)?
    } else {
        0
    };
    let quantized = if layers.albedo {
        estimated_staged_equirect_bytes(face_resolution, 4)?
    } else if layers.roughness || layers.water_mask || layers.clouds {
        estimated_staged_equirect_bytes(face_resolution, 1)?
    } else {
        0
    };
    terrain_faces
        .checked_add(height.max(normal).max(quantized))
        .ok_or_else(|| "staged authoritative live-byte ledger overflows u64".into())
}

fn has_legacy_full_equirect_layer(layers: &ExportLayers) -> bool {
    layers.emission
}

pub fn estimated_export_preflight_bytes(
    face_resolution: u32,
    layers: &ExportLayers,
) -> Result<u64, String> {
    if has_legacy_full_equirect_layer(layers) {
        Ok(estimated_peak_streaming_bytes(face_resolution, 16))
    } else {
        estimated_staged_export_peak_bytes(face_resolution, layers)
    }
}

pub fn estimated_export_preflight_bytes_with_erosion(
    face_resolution: u32,
    layers: &ExportLayers,
    erosion_iterations: u32,
    limits: &wgpu::Limits,
) -> Result<u64, String> {
    let staged_peak = estimated_export_preflight_bytes(face_resolution, layers)?;
    if erosion_iterations == 0 {
        return Ok(staged_peak);
    }
    let erosion_resolution = erosion_resolution_for_export(face_resolution);
    let terrain_faces = u64::from(face_resolution)
        .checked_mul(u64::from(face_resolution))
        .and_then(|pixels| pixels.checked_mul(6))
        .and_then(|values| values.checked_mul(std::mem::size_of::<f32>() as u64))
        .ok_or("erosion terrain ledger overflows u64")?;
    let erosion_peak = terrain_faces
        .checked_add(
            ErosionPipeline::aggregate_tiled_bytes(erosion_resolution, limits)
                .map_err(|error| error.to_string())?,
        )
        .ok_or("erosion aggregate ledger overflows u64")?;
    let reconstruction_peak = if face_resolution == 8192 {
        let meso_faces = u64::from(MESO_EROSION_RESOLUTION)
            .checked_mul(u64::from(MESO_EROSION_RESOLUTION))
            .and_then(|pixels| pixels.checked_mul(6))
            .and_then(|values| values.checked_mul(std::mem::size_of::<f32>() as u64))
            .ok_or("meso reconstruction ledger overflows u64")?;
        terrain_faces
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(meso_faces.checked_mul(2)?))
            .ok_or("meso reconstruction aggregate ledger overflows u64")?
    } else {
        0
    };
    Ok(staged_peak.max(erosion_peak).max(reconstruction_peak))
}

pub fn validate_8k_owned_live_bytes(bytes: u64) -> Result<(), String> {
    if bytes > MAX_8K_OWNED_LIVE_BYTES {
        return Err(
            "current streaming and erosion buffers exceed the 8K owned live-byte budget".into(),
        );
    }
    Ok(())
}

#[derive(Default)]
struct LayerStream {
    active_faces: u32,
    peak_faces: u32,
}

impl LayerStream {
    fn begin_face(&mut self) {
        self.active_faces += 1;
        self.peak_faces = self.peak_faces.max(self.active_faces);
    }

    fn end_face(&mut self) {
        self.active_faces -= 1;
    }
}

pub fn crop_region_bytes(region: TileRegion, data: &[u8], element_bytes: usize) -> Vec<u8> {
    let source_row_bytes = region.width as usize * element_bytes;
    let output_row_bytes = region.crop_width as usize * element_bytes;
    let mut output = vec![0; output_row_bytes * region.crop_height as usize];
    for row in 0..region.crop_height as usize {
        let source = (region.crop_y as usize + row) * source_row_bytes
            + region.crop_x as usize * element_bytes;
        let destination = row * output_row_bytes;
        output[destination..destination + output_row_bytes]
            .copy_from_slice(&data[source..source + output_row_bytes]);
    }
    output
}

fn region_f32(region: TileRegion, data: &[f32], resolution: u32) -> Vec<f32> {
    let mut output = Vec::with_capacity((region.width * region.height) as usize);
    for y in region.origin_y..region.origin_y + region.height {
        let start = (y * resolution + region.origin_x) as usize;
        output.extend_from_slice(&data[start..start + region.width as usize]);
    }
    output
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
    pub local_height: u32,
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
    pub local_height: u32,
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
    pub local_height: u32,
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
    pub local_height: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct CloudMapParams {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub ocean_fraction: f32,
    pub axial_tilt_rad: f32,
    pub season: f32,
    pub cloud_coverage: f32,
    pub cloud_type: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub local_height: u32,
    pub _pad1: u32,
    pub _pad2: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct EmissionMapParams {
    pub face: u32,
    pub resolution: u32,
    pub seed: u32,
    pub base_temp_c: f32,
    pub ocean_level: f32,
    pub night_lights: f32,
    pub axial_tilt_rad: f32,
    pub tile_offset_x: u32,
    pub tile_offset_y: u32,
    pub full_resolution: u32,
    pub local_height: u32,
    pub _pad1: u32,
}

// ============ Generic Map Compute Pipeline ============

struct MapPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    workgroup_size: u32,
}

pub const MAP_BATCH_SIZE: usize = 8;

pub fn map_batch_fits(current_bytes: u64, next_bytes: u64, cap_bytes: u64) -> bool {
    current_bytes
        .checked_add(next_bytes)
        .is_some_and(|total| total <= cap_bytes)
}

pub fn map_tile_owned_bytes(
    heightmap_bytes: u64,
    params_bytes: u64,
    tile_width: u32,
    tile_height: u32,
    output_element_bytes: usize,
) -> Result<u64, String> {
    let output_bytes = u64::from(tile_width)
        .checked_mul(u64::from(tile_height))
        .and_then(|pixels| pixels.checked_mul(output_element_bytes as u64))
        .ok_or("map tile output byte size overflows u64")?;
    heightmap_bytes
        .checked_add(params_bytes)
        .and_then(|bytes| bytes.checked_add(output_bytes))
        .and_then(|bytes| bytes.checked_add(output_bytes))
        .ok_or("map tile owned-byte ledger overflows u64".into())
}

pub fn map_tile_fits_device(
    heightmap_bytes: u64,
    output_bytes: u64,
    limits: &wgpu::Limits,
) -> Result<(), String> {
    let storage_cap = u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size);
    if heightmap_bytes > storage_cap
        || output_bytes > storage_cap
        || output_bytes > limits.max_buffer_size
    {
        return Err("map tile allocation exceeds queried device buffer limits".into());
    }
    Ok(())
}

#[allow(dead_code)]
struct MapTileDispatch {
    _heightmap: wgpu::Buffer,
    _params: wgpu::Buffer,
    output: wgpu::Buffer,
    staging: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    output_size: u64,
    owned_bytes: u64,
    tile_width: u32,
    tile_height: u32,
}

#[allow(dead_code)]
struct MapTileBatch {
    tiles: Vec<MapTileDispatch>,
    owned_bytes: u64,
    reserved_bytes: u64,
    pending_reservations: usize,
    byte_cap: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MapBatchMetrics {
    pub reached: bool,
    pub failed: bool,
    pub completed: bool,
    pub elapsed_ms: f64,
    pub submissions: u64,
    pub polls: u64,
    pub map_requests: u64,
    pub owned_bytes: u64,
    pub retained_bytes_after_consume: u64,
}

struct MapBatchRead {
    results: Vec<Vec<u8>>,
    metrics: MapBatchMetrics,
}

struct MapBatchFailure {
    message: String,
    metrics: MapBatchMetrics,
}

#[allow(dead_code)]
impl MapTileBatch {
    fn new(byte_cap: u64) -> Self {
        Self {
            tiles: Vec::with_capacity(MAP_BATCH_SIZE),
            owned_bytes: 0,
            reserved_bytes: 0,
            pending_reservations: 0,
            byte_cap,
        }
    }

    fn push(&mut self, tile: MapTileDispatch) -> Result<(), String> {
        self.reserve(tile.owned_bytes)?;
        self.push_reserved(tile)
    }

    fn push_reserved(&mut self, tile: MapTileDispatch) -> Result<(), String> {
        self.consume_reservation(tile.owned_bytes)?;
        self.tiles.push(tile);
        Ok(())
    }

    fn consume_reservation(&mut self, tile_bytes: u64) -> Result<(), String> {
        if self.reserved_bytes < tile_bytes || self.pending_reservations == 0 {
            return Err("map tile was not reserved before insertion".into());
        }
        self.reserved_bytes -= tile_bytes;
        self.pending_reservations -= 1;
        Ok(())
    }

    fn release_reservation(&mut self, tile_bytes: u64) -> Result<(), String> {
        if self.reserved_bytes < tile_bytes || self.pending_reservations == 0 {
            return Err("map tile reservation is not active".into());
        }
        self.reserved_bytes -= tile_bytes;
        self.owned_bytes -= tile_bytes;
        self.pending_reservations -= 1;
        Ok(())
    }

    fn reserve(&mut self, tile_bytes: u64) -> Result<(), String> {
        if self.tiles.len() + self.pending_reservations == MAP_BATCH_SIZE
            || !map_batch_fits(self.owned_bytes, tile_bytes, self.byte_cap)
        {
            return Err("map tile batch reached its resource cap; submit or flush before adding another tile".into());
        }
        self.owned_bytes = self
            .owned_bytes
            .checked_add(tile_bytes)
            .ok_or("map tile batch byte ledger overflows u64")?;
        self.reserved_bytes = self
            .reserved_bytes
            .checked_add(tile_bytes)
            .ok_or("map tile batch reservation ledger overflows u64")?;
        self.pending_reservations += 1;
        Ok(())
    }

    fn can_fit(&self, tile_bytes: u64) -> bool {
        self.tiles.len() + self.pending_reservations < MAP_BATCH_SIZE
            && map_batch_fits(self.owned_bytes, tile_bytes, self.byte_cap)
    }

    fn submit_and_read(
        self,
        gpu: &GpuContext,
        pipeline: &MapPipeline,
        cancel: &AtomicBool,
    ) -> Result<MapBatchRead, MapBatchFailure> {
        if self.tiles.is_empty() {
            return Ok(MapBatchRead {
                results: Vec::new(),
                metrics: MapBatchMetrics::default(),
            });
        }
        let metrics = MapBatchMetrics {
            submissions: 1,
            map_requests: self.tiles.len() as u64,
            owned_bytes: self.owned_bytes,
            ..Default::default()
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("map tile batch encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("map tile batch pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline.pipeline);
            for tile in &self.tiles {
                pass.set_bind_group(0, &tile.bind_group, &[]);
                pass.dispatch_workgroups(
                    tile.tile_width.div_ceil(pipeline.workgroup_size),
                    tile.tile_height.div_ceil(pipeline.workgroup_size),
                    1,
                );
            }
        }
        for tile in &self.tiles {
            encoder.copy_buffer_to_buffer(&tile.output, 0, &tile.staging, 0, tile.output_size);
        }
        gpu.queue.submit(Some(encoder.finish()));

        let (sender, receiver) = std::sync::mpsc::channel();
        for (index, tile) in self.tiles.iter().enumerate() {
            let sender = sender.clone();
            tile.staging
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = sender.send((index, result.map_err(|error| error.to_string())));
                });
        }
        drop(sender);
        let mut completed = 0;
        let mut polls = 0;
        while completed < self.tiles.len() {
            if cancel.load(Ordering::Relaxed) {
                return Err(MapBatchFailure {
                    message: "Cancelled while waiting for map batch readback".into(),
                    metrics: MapBatchMetrics { polls, ..metrics },
                });
            }
            match receiver.try_recv() {
                Ok((_, Ok(()))) => completed += 1,
                Ok((_, Err(error))) => {
                    return Err(MapBatchFailure {
                        message: format!("GPU batch readback failed: {error}"),
                        metrics: MapBatchMetrics { polls, ..metrics },
                    });
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    return Err(MapBatchFailure {
                        message: "GPU batch readback callback disconnected".into(),
                        metrics: MapBatchMetrics { polls, ..metrics },
                    });
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    let _ = gpu.device.poll(wgpu::PollType::Poll);
                    polls += 1;
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        }

        let results = self
            .tiles
            .iter()
            .map(|tile| {
                let mapped = tile.staging.slice(..).get_mapped_range();
                let bytes = mapped.to_vec();
                drop(mapped);
                tile.staging.unmap();
                bytes
            })
            .collect();
        Ok(MapBatchRead {
            results,
            metrics: MapBatchMetrics {
                polls,
                retained_bytes_after_consume: 0,
                ..metrics
            },
        })
    }
}

impl MapPipeline {
    #[allow(dead_code)]
    fn begin_batch(&self, byte_cap: u64) -> MapTileBatch {
        MapTileBatch::new(byte_cap)
    }

    #[allow(dead_code)]
    fn prepare_owned_tile(
        &self,
        gpu: &GpuContext,
        heightmap: &[f32],
        params_bytes: &[u8],
        tile_width: u32,
        tile_height: u32,
        output_element_bytes: usize,
        batch: &mut MapTileBatch,
    ) -> Result<MapTileDispatch, String> {
        let output_size = u64::from(tile_width)
            .checked_mul(u64::from(tile_height))
            .and_then(|pixels| pixels.checked_mul(output_element_bytes as u64))
            .ok_or("map tile output byte size overflows u64")?;
        let heightmap_bytes = u64::try_from(heightmap.len())
            .ok()
            .and_then(|values| values.checked_mul(std::mem::size_of::<f32>() as u64))
            .ok_or("map tile heightmap byte size overflows u64")?;
        map_tile_fits_device(heightmap_bytes, output_size, &gpu.device.limits())?;
        let owned_bytes = map_tile_owned_bytes(
            heightmap_bytes,
            params_bytes.len() as u64,
            tile_width,
            tile_height,
            output_element_bytes,
        )?;
        batch.reserve(owned_bytes)?;
        let heightmap = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batched map heightmap"),
                contents: bytemuck::cast_slice(heightmap),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let params = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("batched map params"),
                contents: params_bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("batched map output"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("batched map staging"),
            size: output_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("batched map bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: heightmap.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        Ok(MapTileDispatch {
            _heightmap: heightmap,
            _params: params,
            output,
            staging,
            bind_group,
            output_size,
            owned_bytes,
            tile_width,
            tile_height,
        })
    }
    fn new(gpu: &GpuContext, shader_source: &str, label: &str, workgroup_size: u32) -> Self {
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

        let pipeline_layout = gpu
            .device
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
            workgroup_size,
        }
    }

    fn dispatch_tile(
        &self,
        gpu: &GpuContext,
        heightmap_buffer: &wgpu::Buffer,
        params_bytes: &[u8],
        tile_width: u32,
        tile_height: u32,
        output_element_bytes: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<u8>, String> {
        let total_pixels = (tile_width * tile_height) as usize;
        let output_size = (total_pixels * output_element_bytes) as u64;

        let params_buffer = gpu
            .device
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
            pass.dispatch_workgroups(
                tile_width.div_ceil(self.workgroup_size),
                tile_height.div_ceil(self.workgroup_size),
                1,
            );
        }

        encoder.copy_buffer_to_buffer(&output_buffer, 0, &staging_buffer, 0, output_size);
        gpu.queue.submit(Some(encoder.finish()));

        wait_for_readback(gpu, staging_buffer.slice(..), cancel)?;

        let mapped = staging_buffer.slice(..).get_mapped_range();
        let result = mapped.to_vec();
        drop(mapped);
        staging_buffer.unmap();

        Ok(result)
    }
}

fn wait_for_readback(
    gpu: &GpuContext,
    slice: wgpu::BufferSlice<'_>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result.map_err(|error| error.to_string()));
    });
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled while waiting for GPU readback".into());
        }
        match receiver.try_recv() {
            Ok(result) => return result.map_err(|error| format!("GPU readback failed: {error}")),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("GPU readback callback disconnected".into());
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        let _ = gpu.device.poll(wgpu::PollType::Poll);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

// ============ Tiled Terrain Generation ============

#[allow(clippy::too_many_arguments)]
fn generate_terrain_tiled(
    gpu: &GpuContext,
    terrain_pipeline: &TerrainComputePipeline,
    plates_buffer: &wgpu::Buffer,
    coordinator: &TileCoordinator,
    num_plates: u32,
    terrain_params: &TerrainGenerationParams,
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

                let params = TerrainGenParams::for_tile(
                    terrain_params,
                    face_idx,
                    tile_size,
                    num_plates,
                    offset_x,
                    offset_y,
                    res,
                );

                let tile_data =
                    terrain_pipeline.dispatch_tile(gpu, plates_buffer, &params, cancel)?;

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

#[allow(clippy::too_many_arguments)]
fn generate_map_tiled<P: Pod>(
    gpu: &GpuContext,
    pipeline: &MapPipeline,
    heightmap: &[f32],
    coordinator: &TileCoordinator,
    make_params: impl Fn(TileRegion) -> P,
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
    let batch_cap = MAX_OWNED_LIVE_BYTES.min(gpu.device.limits().max_buffer_size);
    let mut batch = pipeline.begin_batch(batch_cap);
    let mut batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);

    let mut consume = |batch: MapTileBatch, regions: Vec<TileRegion>| -> Result<(), String> {
        let read = match batch.submit_and_read(gpu, pipeline, cancel) {
            Ok(read) => read,
            Err(failure) => return Err(failure.message),
        };
        for (region, tile_data) in regions.into_iter().zip(read.results) {
            let tile_row_bytes = tile_size as usize * output_element_bytes;
            let cropped = crop_region_bytes(region, &tile_data, output_element_bytes);
            for row in 0..region.crop_height as usize {
                let src_start = row * tile_row_bytes;
                let dst_row = region.origin_y as usize + region.crop_y as usize + row;
                let dst_start = dst_row * row_bytes
                    + (region.origin_x as usize + region.crop_x as usize) * output_element_bytes;
                face_data[dst_start..dst_start + tile_row_bytes]
                    .copy_from_slice(&cropped[src_start..src_start + tile_row_bytes]);
            }
            progress.advance(&format!("{map_name} face {face}"));
        }
        Ok(())
    };

    for ty in 0..tiles_per_axis {
        for tx in 0..tiles_per_axis {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }

            let region = coordinator.region(tx, ty);
            let params = make_params(region);
            let height_tile = region_f32(region, heightmap, full_res);
            let tile_bytes = map_tile_owned_bytes(
                (height_tile.len() * std::mem::size_of::<f32>()) as u64,
                bytemuck::bytes_of(&params).len() as u64,
                region.width,
                region.height,
                output_element_bytes,
            )?;
            if !batch.tiles.is_empty() && !batch.can_fit(tile_bytes) {
                consume(batch, batch_regions)?;
                batch = pipeline.begin_batch(batch_cap);
                batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);
            }
            let tile = pipeline.prepare_owned_tile(
                gpu,
                &height_tile,
                bytemuck::bytes_of(&params),
                region.width,
                region.height,
                output_element_bytes,
                &mut batch,
            )?;
            batch.push_reserved(tile)?;
            batch_regions.push(region);
            if batch.tiles.len() == MAP_BATCH_SIZE {
                consume(batch, batch_regions)?;
                batch = pipeline.begin_batch(batch_cap);
                batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);
            }
        }
    }

    if !batch.tiles.is_empty() {
        consume(batch, batch_regions)?;
    }

    Ok(face_data)
}

#[allow(clippy::too_many_arguments)]
fn stream_map_layer<P: Pod>(
    gpu: &GpuContext,
    pipeline: &MapPipeline,
    terrain: &TectonicTerrain,
    coordinator: &TileCoordinator,
    make_params: impl Fn(u32, TileRegion) -> P,
    output_element_bytes: usize,
    progress: &mut ProgressTracker,
    map_name: &str,
    cancel: &AtomicBool,
) -> Result<(Vec<f32>, u32, u32), String> {
    let full_res = coordinator.face_resolution;
    let eq_w = full_res * 2;
    let eq_h = full_res;
    let mut equirect = vec![0.0; (eq_w * eq_h) as usize * (output_element_bytes / 4)];
    let mut stream = LayerStream::default();
    for face in 0..6u32 {
        stream.begin_face();
        {
            let bytes = generate_map_tiled(
                gpu,
                pipeline,
                &terrain.faces[face as usize],
                coordinator,
                |region| make_params(face, region),
                output_element_bytes,
                progress,
                map_name,
                face,
                cancel,
            )?;
            write_face_to_equirect(
                bytemuck::cast_slice(&bytes),
                face,
                full_res,
                output_element_bytes / 4,
                &mut equirect,
            );
        }
        stream.end_face();
    }
    debug_assert_eq!(stream.peak_faces, 1);
    Ok((equirect, eq_w, eq_h))
}

fn stage_terrain_faces(
    output_dir: &Path,
    terrain: &TectonicTerrain,
    coordinator: &TileCoordinator,
    cancel: &AtomicBool,
) -> Result<ExportStage, String> {
    let stage = ExportStage::create(
        output_dir,
        StageMetadata {
            face_resolution: coordinator.face_resolution,
            components: 1,
            halo: 0,
        },
    )?;
    for face in 0..6u32 {
        for tile_y in 0..coordinator.tiles_per_axis {
            for tile_x in 0..coordinator.tiles_per_axis {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let origin_x = tile_x * coordinator.tile_size;
                let origin_y = tile_y * coordinator.tile_size;
                let mut values =
                    Vec::with_capacity((coordinator.tile_size * coordinator.tile_size) as usize);
                for y in origin_y..origin_y + coordinator.tile_size {
                    let start = (y * coordinator.face_resolution + origin_x) as usize;
                    values.extend_from_slice(
                        &terrain.faces[face as usize]
                            [start..start + coordinator.tile_size as usize],
                    );
                }
                stage.write_tile(
                    face,
                    origin_x,
                    origin_y,
                    coordinator.tile_size,
                    coordinator.tile_size,
                    &values,
                )?;
            }
        }
    }
    Ok(stage)
}

fn stage_ocean_mask_faces(
    output_dir: &Path,
    terrain: &TectonicTerrain,
    coordinator: &TileCoordinator,
    ocean_level: f32,
    cancel: &AtomicBool,
) -> Result<ExportStage, String> {
    let stage = ExportStage::create(
        output_dir,
        StageMetadata {
            face_resolution: coordinator.face_resolution,
            components: 1,
            halo: 0,
        },
    )?;
    for face in 0..6u32 {
        for tile_y in 0..coordinator.tiles_per_axis {
            for tile_x in 0..coordinator.tiles_per_axis {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let origin_x = tile_x * coordinator.tile_size;
                let origin_y = tile_y * coordinator.tile_size;
                let mut values =
                    Vec::with_capacity((coordinator.tile_size * coordinator.tile_size) as usize);
                for y in origin_y..origin_y + coordinator.tile_size {
                    let start = (y * coordinator.face_resolution + origin_x) as usize;
                    values.extend(
                        terrain.faces[face as usize][start..start + coordinator.tile_size as usize]
                            .iter()
                            .map(|height| if *height < ocean_level { 1.0 } else { 0.0 }),
                    );
                }
                stage.write_tile(
                    face,
                    origin_x,
                    origin_y,
                    coordinator.tile_size,
                    coordinator.tile_size,
                    &values,
                )?;
            }
        }
    }
    Ok(stage)
}

#[allow(clippy::too_many_arguments)]
fn stage_map_layer<P: Pod>(
    gpu: &GpuContext,
    pipeline: &MapPipeline,
    terrain: &TectonicTerrain,
    coordinator: &TileCoordinator,
    make_params: impl Fn(u32, TileRegion) -> P,
    output_element_bytes: usize,
    progress: &mut ProgressTracker,
    map_name: &str,
    cancel: &AtomicBool,
    output_dir: &Path,
    metrics: &mut MapBatchMetrics,
) -> Result<ExportStage, String> {
    let components = u32::try_from(output_element_bytes / std::mem::size_of::<f32>())
        .map_err(|_| "staged map component count exceeds u32")?;
    if components == 0 || output_element_bytes % std::mem::size_of::<f32>() != 0 {
        return Err("staged map output must contain f32 components".into());
    }
    let stage = ExportStage::create(
        output_dir,
        StageMetadata {
            face_resolution: coordinator.face_resolution,
            components,
            halo: coordinator.stencil_radius,
        },
    )?;
    let batch_cap = MAX_OWNED_LIVE_BYTES.min(gpu.device.limits().max_buffer_size);
    let mut batch = pipeline.begin_batch(batch_cap);
    let mut batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);
    let mut consume =
        |batch: MapTileBatch, regions: Vec<(u32, TileRegion)>| -> Result<(), String> {
            metrics.reached = true;
            let started = std::time::Instant::now();
            let read = match batch.submit_and_read(gpu, pipeline, cancel) {
                Ok(read) => read,
                Err(failure) => {
                    metrics.failed = true;
                    metrics.submissions += failure.metrics.submissions;
                    metrics.polls += failure.metrics.polls;
                    metrics.map_requests += failure.metrics.map_requests;
                    metrics.owned_bytes = metrics.owned_bytes.max(failure.metrics.owned_bytes);
                    metrics.retained_bytes_after_consume +=
                        failure.metrics.retained_bytes_after_consume;
                    metrics.elapsed_ms += started.elapsed().as_secs_f64() * 1000.0;
                    return Err(failure.message);
                }
            };
            metrics.submissions += read.metrics.submissions;
            metrics.polls += read.metrics.polls;
            metrics.map_requests += read.metrics.map_requests;
            metrics.owned_bytes = metrics.owned_bytes.max(read.metrics.owned_bytes);
            metrics.retained_bytes_after_consume += read.metrics.retained_bytes_after_consume;
            metrics.elapsed_ms += started.elapsed().as_secs_f64() * 1000.0;
            for ((face, region), tile_data) in regions.into_iter().zip(read.results) {
                let cropped = crop_region_bytes(region, &tile_data, output_element_bytes);
                let values = bytemuck::try_cast_slice(&cropped)
                    .map_err(|_| "staged map output is not aligned f32 data")?;
                stage.write_tile(
                    face,
                    region.origin_x + region.crop_x,
                    region.origin_y + region.crop_y,
                    region.crop_width,
                    region.crop_height,
                    values,
                )?;
                progress.advance(&format!("{map_name} face {face}"));
            }
            Ok(())
        };
    for face in 0..6u32 {
        for tile_y in 0..coordinator.tiles_per_axis {
            for tile_x in 0..coordinator.tiles_per_axis {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Cancelled".into());
                }
                let region = coordinator.region(tile_x, tile_y);
                let params = make_params(face, region);
                let height_tile = region_f32(
                    region,
                    &terrain.faces[face as usize],
                    coordinator.face_resolution,
                );
                let tile_bytes = map_tile_owned_bytes(
                    (height_tile.len() * std::mem::size_of::<f32>()) as u64,
                    bytemuck::bytes_of(&params).len() as u64,
                    region.width,
                    region.height,
                    output_element_bytes,
                )?;
                if !batch.tiles.is_empty() && !batch.can_fit(tile_bytes) {
                    consume(batch, batch_regions)?;
                    batch = pipeline.begin_batch(batch_cap);
                    batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);
                }
                let tile = pipeline.prepare_owned_tile(
                    gpu,
                    &height_tile,
                    bytemuck::bytes_of(&params),
                    region.width,
                    region.height,
                    output_element_bytes,
                    &mut batch,
                )?;
                batch.push_reserved(tile)?;
                batch_regions.push((face, region));
                if batch.tiles.len() == MAP_BATCH_SIZE {
                    consume(batch, batch_regions)?;
                    batch = pipeline.begin_batch(batch_cap);
                    batch_regions = Vec::with_capacity(MAP_BATCH_SIZE);
                }
            }
        }
    }
    if !batch.tiles.is_empty() {
        consume(batch, batch_regions)?;
    }
    Ok(stage)
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

/// Inverse of cube_to_sphere: given a 3D direction, find which cube face and UV.
pub(crate) fn direction_to_face_uv(dx: f32, dy: f32, dz: f32) -> (usize, f32, f32) {
    let ax = dx.abs();
    let ay = dy.abs();
    let az = dz.abs();

    let (face, s, t) = if ax >= ay && ax >= az {
        if dx > 0.0 {
            (0, -dz / ax, -dy / ax) // +X
        } else {
            (1, dz / ax, -dy / ax) // -X
        }
    } else if ay >= ax && ay >= az {
        if dy > 0.0 {
            (2, dx / ay, dz / ay) // +Y
        } else {
            (3, dx / ay, -dz / ay) // -Y
        }
    } else if dz > 0.0 {
        (4, dx / az, -dy / az) // +Z
    } else {
        (5, -dx / az, -dy / az) // -Z
    };

    let u = ((s + 1.0) * 0.5).clamp(0.0, 1.0);
    let v = ((t + 1.0) * 0.5).clamp(0.0, 1.0);
    (face, u, v)
}

pub(crate) fn equirect_pixel_to_direction(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> [f32; 3] {
    let lat = std::f32::consts::PI * (0.5 - y as f32 / (height - 1).max(1) as f32);
    let lon = 2.0 * std::f32::consts::PI * (x as f32 / width as f32) - std::f32::consts::PI;
    [lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos()]
}

/// Convert 6 cubemap faces to a single equirectangular image with bilinear interpolation.
/// `channels` is 1 for grayscale, 4 for RGBA.
#[cfg(test)]
pub(crate) fn cubemap_to_equirect(
    faces: &[Vec<f32>; 6],
    face_res: u32,
    channels: usize,
) -> (Vec<f32>, u32, u32) {
    let eq_w = (face_res * 2) as usize;
    let eq_h = face_res as usize;
    let res = face_res as usize;
    let mut result = vec![0.0f32; eq_w * eq_h * channels];

    for y in 0..eq_h {
        for x in 0..eq_w {
            let [dx, dy, dz] = equirect_pixel_to_direction(x, y, eq_w, eq_h);

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

fn write_face_to_equirect(
    face_data: &[f32],
    face_index: u32,
    face_res: u32,
    channels: usize,
    output: &mut [f32],
) {
    let eq_w = face_res as usize * 2;
    let eq_h = face_res as usize;
    let res = face_res as usize;
    for y in 0..eq_h {
        for x in 0..eq_w {
            let [dx, dy, dz] = equirect_pixel_to_direction(x, y, eq_w, eq_h);
            let (face, u, v) = direction_to_face_uv(dx, dy, dz);
            if face != face_index as usize {
                continue;
            }
            let fx = u * (res - 1) as f32;
            let fy = v * (res - 1) as f32;
            let ix = (fx as usize).min(res - 2);
            let iy = (fy as usize).min(res - 2);
            let frac_x = fx - ix as f32;
            let frac_y = fy - iy as f32;
            for channel in 0..channels {
                let top = face_data[(iy * res + ix) * channels + channel]
                    + (face_data[(iy * res + ix + 1) * channels + channel]
                        - face_data[(iy * res + ix) * channels + channel])
                        * frac_x;
                let bottom = face_data[((iy + 1) * res + ix) * channels + channel]
                    + (face_data[((iy + 1) * res + ix + 1) * channels + channel]
                        - face_data[((iy + 1) * res + ix) * channels + channel])
                        * frac_x;
                output[(y * eq_w + x) * channels + channel] = top + (bottom - top) * frac_y;
            }
        }
    }
}

/// Write RGBA EXR with DWAB compression (lossy, ~80% quality).
#[cfg(test)]
fn export_equirect_exr_rgba(
    data: &[f32],
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), String> {
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
fn export_equirect_exr_gray(
    data: &[f32],
    width: u32,
    height: u32,
    path: &Path,
) -> Result<(), String> {
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

#[allow(clippy::too_many_arguments)]
fn materialize_staged_equirect<F>(
    layer: &'static str,
    stage: &ExportStage,
    width: u32,
    height: u32,
    channels: u32,
    output_dir: &Path,
    cancel: &AtomicBool,
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
    finish: F,
) -> Result<(EquirectStage, StagedIoMetrics), StagedExportFailure>
where
    F: FnOnce(&mut EquirectStage) -> Result<(), String>,
{
    let worker_cap = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_EQUIRECT_ROW_WORKERS);
    materialize_staged_equirect_with_worker_cap(
        layer,
        stage,
        width,
        height,
        channels,
        output_dir,
        cancel,
        checkpoints,
        worker_cap,
        finish,
    )
}

#[allow(clippy::too_many_arguments)]
fn materialize_staged_equirect_with_worker_cap<F>(
    layer: &'static str,
    stage: &ExportStage,
    width: u32,
    height: u32,
    channels: u32,
    output_dir: &Path,
    cancel: &AtomicBool,
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
    worker_cap: usize,
    finish: F,
) -> Result<(EquirectStage, StagedIoMetrics), StagedExportFailure>
where
    F: FnOnce(&mut EquirectStage) -> Result<(), String>,
{
    if estimated_staged_equirect_bytes(stage.metadata().face_resolution, channels).map_err(
        |message| StagedExportFailure {
            message,
            metrics: StagedIoMetrics::default(),
        },
    )? > MAX_OWNED_LIVE_BYTES
    {
        return Err(StagedExportFailure {
            message: "staged equirect rows exceed the owned live-byte budget".into(),
            metrics: StagedIoMetrics::default(),
        });
    }
    let (region_side, cache_regions) =
        staged_sampler_config(stage.metadata().face_resolution, channels).map_err(|message| {
            StagedExportFailure {
                message,
                metrics: StagedIoMetrics::default(),
            }
        })?;
    let row_config = equirect_row_materialization_config(
        width,
        height,
        channels,
        region_side,
        cache_regions,
        worker_cap,
    )
    .map_err(|message| StagedExportFailure {
        message,
        metrics: StagedIoMetrics::default(),
    })?;
    let materialization_started = Instant::now();
    let mut intermediate =
        EquirectStage::create(output_dir, width, height, channels).map_err(|message| {
            StagedExportFailure {
                message,
                metrics: StagedIoMetrics::default(),
            }
        })?;
    let mut metrics = StagedIoMetrics {
        cache_capacity_values: row_config.aggregate_cache_values,
        ..Default::default()
    };
    let mut rows_written = 0;
    let worker_peaks = Arc::new(Mutex::new(vec![0; row_config.workers]));
    let parallel_result = std::thread::scope(|scope| {
        let (job_tx, job_rx) = mpsc::sync_channel(row_config.queue_bound);
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (row_tx, row_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));

        for worker in 0..row_config.workers {
            let job_rx = Arc::clone(&job_rx);
            let row_tx = row_tx.clone();
            let stop = Arc::clone(&stop);
            let worker_peaks = Arc::clone(&worker_peaks);
            scope.spawn(move || {
                let sampler = match StagedCubemapSampler::new(stage, region_side, cache_regions) {
                    Ok(sampler) => sampler,
                    Err(message) => {
                        stop.store(true, Ordering::Relaxed);
                        let _ = row_tx.send(ParallelRow::failure(message));
                        return;
                    }
                };
                let mut generator =
                    match StagedEquirectRowGenerator::new(sampler, width, height, channels) {
                        Ok(generator) => generator,
                        Err(message) => {
                            stop.store(true, Ordering::Relaxed);
                            let _ = row_tx.send(ParallelRow::failure(message.to_string()));
                            return;
                        }
                    };
                loop {
                    if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let next_job = job_rx
                        .lock()
                        .map_err(|_| "parallel equirect job queue lock poisoned")
                        .and_then(|receiver| {
                            receiver
                                .recv_timeout(WORKER_POLL_INTERVAL)
                                .map_err(|error| match error {
                                    mpsc::RecvTimeoutError::Timeout => {
                                        "parallel equirect job queue timed out"
                                    }
                                    mpsc::RecvTimeoutError::Disconnected => {
                                        "parallel equirect job queue closed"
                                    }
                                })
                        });
                    let y = match next_job {
                        Ok(y) => y,
                        Err("parallel equirect job queue timed out") => continue,
                        Err("parallel equirect job queue closed") => return,
                        Err(message) => {
                            stop.store(true, Ordering::Relaxed);
                            let _ = row_tx.send(ParallelRow::failure(message.to_string()));
                            return;
                        }
                    };
                    let before = generator.sampler_diagnostics();
                    let started = Instant::now();
                    let result = generator.generate_row_with_cancel(y, cancel);
                    let diagnostics =
                        sampler_diagnostics_delta(before, generator.sampler_diagnostics());
                    if let Ok(mut peaks) = worker_peaks.lock() {
                        peaks[worker] = peaks[worker].max(diagnostics.peak_cached_values);
                    }
                    if result.is_err() {
                        stop.store(true, Ordering::Relaxed);
                    }
                    if row_tx
                        .send(ParallelRow {
                            y: Some(y),
                            result,
                            diagnostics,
                            elapsed: started.elapsed(),
                        })
                        .is_err()
                    {
                        return;
                    }
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                }
            });
        }
        drop(row_tx);

        let mut job_tx = Some(job_tx);
        let mut rows_submitted = 0;
        let mut pending = BTreeMap::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                stop.store(true, Ordering::Relaxed);
                drop(job_tx.take());
                return Err("Cancelled".into());
            }
            while rows_submitted < height
                && rows_submitted - rows_written < row_config.queue_bound as u32
                && !stop.load(Ordering::Relaxed)
            {
                job_tx
                    .as_ref()
                    .ok_or("parallel equirect job queue closed")?
                    .send(rows_submitted)
                    .map_err(|_| "parallel equirect workers stopped before completing rows")?;
                rows_submitted += 1;
            }
            if rows_submitted == height {
                drop(job_tx.take());
            }
            if rows_written == height {
                return Ok(());
            }
            let row = match row_rx.recv_timeout(WORKER_POLL_INTERVAL) {
                Ok(row) => row,
                Err(mpsc::RecvTimeoutError::Timeout) if cancel.load(Ordering::Relaxed) => {
                    return Err("Cancelled".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) if stop.load(Ordering::Relaxed) => {
                    return Err("parallel equirect worker stopped before completing rows".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("parallel equirect workers stopped before completing rows".into());
                }
            };
            metrics.worker_row_generation_ms += row.elapsed.as_secs_f64() * 1000.0;
            record_sampler_diagnostics(&mut metrics, row.diagnostics);
            merge_worker_cache_peaks(&mut metrics, &worker_peaks);
            let y = row
                .y
                .ok_or("parallel equirect worker failed before selecting a row")?;
            let values = match row.result {
                Ok(values) => values,
                Err(message) => {
                    stop.store(true, Ordering::Relaxed);
                    drop(job_tx.take());
                    return Err(message);
                }
            };
            pending.insert(y, values);
            while let Some(values) = pending.remove(&rows_written) {
                let write_started = Instant::now();
                intermediate.write_row(&values)?;
                metrics.intermediate_write_ms += write_started.elapsed().as_secs_f64() * 1000.0;
                metrics.intermediate_write_bytes +=
                    values.len() as u64 * std::mem::size_of::<f32>() as u64;
                rows_written += 1;
                if rows_written.is_multiple_of(MATERIALIZATION_CHECKPOINT_ROW_INTERVAL)
                    || rows_written == height
                {
                    emit_materialization_checkpoint(
                        checkpoints,
                        layer,
                        &metrics,
                        rows_written,
                        height,
                        false,
                        None,
                    )
                    .map_err(|failure| failure.message)?;
                }
            }
        }
    });
    merge_worker_cache_peaks(&mut metrics, &worker_peaks);
    metrics.row_generation_ms = materialization_started.elapsed().as_secs_f64() * 1000.0;
    if let Err(message) = parallel_result {
        let reason = if cancel.load(Ordering::Relaxed) {
            "Cancelled"
        } else {
            &message
        };
        emit_materialization_checkpoint(
            checkpoints,
            layer,
            &metrics,
            rows_written,
            height,
            false,
            Some(reason),
        )?;
        return Err(StagedExportFailure {
            message: reason.into(),
            metrics,
        });
    }
    if let Err(message) = finish(&mut intermediate) {
        metrics.row_generation_ms = materialization_started.elapsed().as_secs_f64() * 1000.0;
        emit_materialization_checkpoint(
            checkpoints,
            layer,
            &metrics,
            height,
            height,
            false,
            Some(&message),
        )?;
        return Err(StagedExportFailure { message, metrics });
    }
    metrics.row_generation_ms = materialization_started.elapsed().as_secs_f64() * 1000.0;
    emit_materialization_checkpoint(checkpoints, layer, &metrics, height, height, true, None)?;
    Ok((intermediate, metrics))
}

fn emit_materialization_checkpoint(
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
    layer: &'static str,
    metrics: &StagedIoMetrics,
    rows_completed: u32,
    rows_total: u32,
    completed: bool,
    reason: Option<&str>,
) -> Result<(), StagedExportFailure> {
    checkpoints(&LayerMaterializationCheckpoint {
        layer,
        rows_completed,
        rows_total,
        cache_hits: metrics.cache_hits,
        cache_misses: metrics.region_reads,
        region_reads: metrics.region_reads,
        peak_cached_bytes: metrics.peak_cached_bytes(),
        cache_capacity_bytes: metrics.cache_capacity_bytes(),
        intermediate_write_ms: metrics.intermediate_write_ms,
        intermediate_write_bytes: metrics.intermediate_write_bytes,
        completed,
        reason: reason.map(str::to_owned),
    })
    .map_err(|message| StagedExportFailure {
        message,
        metrics: *metrics,
    })
}

fn export_staged_equirect_exr(
    layer: &'static str,
    stage: &ExportStage,
    width: u32,
    height: u32,
    channels: u32,
    path: &Path,
    cancel: &AtomicBool,
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
) -> Result<StagedIoMetrics, StagedExportFailure> {
    if channels != 1 && channels != 4 {
        return Err(StagedExportFailure {
            message: "staged EXR export supports grayscale or RGBA maps".into(),
            metrics: StagedIoMetrics::default(),
        });
    }
    let output_dir = path.parent().ok_or_else(|| StagedExportFailure {
        message: "EXR output path has no parent directory".into(),
        metrics: StagedIoMetrics::default(),
    })?;
    let (mut intermediate, mut metrics) = materialize_staged_equirect(
        layer,
        stage,
        width,
        height,
        channels,
        output_dir,
        cancel,
        checkpoints,
        EquirectStage::finish,
    )?;
    let mut writer = match AtomicScanlineExrWriter::create(path, width, height) {
        Ok(writer) => writer,
        Err(message) => return Err(StagedExportFailure { message, metrics }),
    };
    let mut rgba = if channels == 1 {
        vec![0.0; width as usize * 4]
    } else {
        Vec::new()
    };
    for y in 0..height {
        if cancel.load(Ordering::Relaxed) {
            return Err(StagedExportFailure {
                message: "Cancelled".into(),
                metrics,
            });
        }
        let row = match intermediate.read_row(y) {
            Ok(row) => row,
            Err(message) => {
                return Err(StagedExportFailure { message, metrics });
            }
        };
        let write_started = Instant::now();
        if channels == 4 {
            if let Err(message) = writer.write_rgba_scanline(&row) {
                return Err(StagedExportFailure { message, metrics });
            }
        } else {
            for (pixel, value) in rgba.chunks_exact_mut(4).zip(row) {
                pixel.copy_from_slice(&[value, value, value, 1.0]);
            }
            if let Err(message) = writer.write_rgba_scanline(&rgba) {
                return Err(StagedExportFailure { message, metrics });
            }
        }
        metrics.output_write_ms += write_started.elapsed().as_secs_f64() * 1000.0;
        metrics.output_write_bytes += u64::from(width) * 4 * std::mem::size_of::<f32>() as u64;
    }
    let finish_started = Instant::now();
    if let Err(message) = writer.finish() {
        return Err(StagedExportFailure { message, metrics });
    }
    metrics.output_finish_ms = finish_started.elapsed().as_secs_f64() * 1000.0;
    metrics.published_file_bytes = std::fs::metadata(path)
        .map_err(|error| StagedExportFailure {
            message: format!("published EXR metadata failed: {error}"),
            metrics,
        })?
        .len();
    Ok(metrics)
}

fn quantize_unit(value: f32, max: u32) -> Result<u32, String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("quantized PNG source value must be finite and within [0, 1]".into());
    }
    Ok((value * max as f32).round() as u32)
}

fn export_staged_equirect_png(
    layer: &'static str,
    stage: &ExportStage,
    width: u32,
    height: u32,
    output_layer: QuantizedLayer,
    path: &Path,
    cancel: &AtomicBool,
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
) -> Result<StagedIoMetrics, StagedExportFailure> {
    let channels = output_layer.channels();
    let output_dir = path.parent().ok_or_else(|| StagedExportFailure {
        message: "PNG output path has no parent directory".into(),
        metrics: StagedIoMetrics::default(),
    })?;
    let (mut intermediate, mut metrics) = materialize_staged_equirect(
        layer,
        stage,
        width,
        height,
        channels,
        output_dir,
        cancel,
        checkpoints,
        EquirectStage::finish,
    )?;
    let format = output_layer.png_format();
    let mut writer = match AtomicScanlinePngWriter::create(path, width, height, format) {
        Ok(writer) => writer,
        Err(message) => return Err(StagedExportFailure { message, metrics }),
    };
    let bytes_per_pixel = match format {
        PngRowFormat::Gray8 => 1,
        PngRowFormat::Gray16 => 2,
        PngRowFormat::Rgba8 => 4,
    };
    let mut encoded = vec![0; width as usize * bytes_per_pixel];
    macro_rules! quantize_or_return {
        ($value:expr, $max:expr) => {
            match quantize_unit($value, $max) {
                Ok(value) => value,
                Err(message) => {
                    return Err(StagedExportFailure { message, metrics });
                }
            }
        };
    }
    for y in 0..height {
        if cancel.load(Ordering::Relaxed) {
            return Err(StagedExportFailure {
                message: "Cancelled".into(),
                metrics,
            });
        }
        let row = match intermediate.read_row(y) {
            Ok(row) => row,
            Err(message) => {
                return Err(StagedExportFailure { message, metrics });
            }
        };
        let write_started = Instant::now();
        match format {
            PngRowFormat::Gray8 => {
                for (output, value) in encoded.iter_mut().zip(row) {
                    *output = quantize_or_return!(value, u8::MAX as u32) as u8;
                }
            }
            PngRowFormat::Gray16 => {
                for (output, value) in encoded.chunks_exact_mut(2).zip(row) {
                    output.copy_from_slice(
                        &(quantize_or_return!(value, u16::MAX as u32) as u16).to_be_bytes(),
                    );
                }
            }
            PngRowFormat::Rgba8 => {
                for (output, value) in encoded.iter_mut().zip(row) {
                    *output = quantize_or_return!(value, u8::MAX as u32) as u8;
                }
            }
        }
        if let Err(message) = writer.write_scanline(&encoded) {
            return Err(StagedExportFailure { message, metrics });
        }
        metrics.output_write_ms += write_started.elapsed().as_secs_f64() * 1000.0;
        metrics.output_write_bytes += encoded.len() as u64;
    }
    let finish_started = Instant::now();
    if let Err(message) = writer.finish() {
        return Err(StagedExportFailure { message, metrics });
    }
    metrics.output_finish_ms = finish_started.elapsed().as_secs_f64() * 1000.0;
    metrics.published_file_bytes = std::fs::metadata(path)
        .map_err(|error| StagedExportFailure {
            message: format!("published PNG metadata failed: {error}"),
            metrics,
        })?
        .len();
    Ok(metrics)
}

// ============ Main Export Orchestrator ============

fn face_uv_to_direction(face: usize, u: f32, v: f32) -> [f32; 3] {
    let s = u.mul_add(2.0, -1.0);
    let t = v.mul_add(2.0, -1.0);
    match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        _ => [-s, -t, -1.0],
    }
}

fn sample_cubemap_height(terrain: &TectonicTerrain, direction: [f32; 3]) -> f32 {
    let (face, u, v) = direction_to_face_uv(direction[0], direction[1], direction[2]);
    let resolution = terrain.resolution as usize;
    let x = u * (resolution - 1) as f32;
    let y = v * (resolution - 1) as f32;
    let x0 = (x as usize).min(resolution - 2);
    let y0 = (y as usize).min(resolution - 2);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let values = &terrain.faces[face];
    let at = |x, y| values[y * resolution + x];
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * tx;
    let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * tx;
    top + (bottom - top) * ty
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReconstructionError {
    #[error("reconstruction requires a full terrain four times the meso resolution")]
    InvalidDimensions,
    #[error("8K delta reconstruction cancelled")]
    Cancelled,
}

pub fn reconstruct_8k_from_meso_delta(
    full_uneroded: &TectonicTerrain,
    meso_uneroded: &TectonicTerrain,
    meso_eroded: &TectonicTerrain,
) -> Result<TectonicTerrain, ReconstructionError> {
    let cancel = AtomicBool::new(false);
    reconstruct_8k_from_meso_delta_with_cancel(full_uneroded, meso_uneroded, meso_eroded, &cancel)
}

fn reconstruct_8k_from_meso_delta_with_cancel(
    full_uneroded: &TectonicTerrain,
    meso_uneroded: &TectonicTerrain,
    meso_eroded: &TectonicTerrain,
    cancel: &AtomicBool,
) -> Result<TectonicTerrain, ReconstructionError> {
    if full_uneroded.resolution != meso_uneroded.resolution.saturating_mul(4)
        || meso_uneroded.resolution != meso_eroded.resolution
    {
        return Err(ReconstructionError::InvalidDimensions);
    }
    let resolution = full_uneroded.resolution as usize;
    let mut faces = std::array::from_fn(|_| Vec::with_capacity(resolution * resolution));
    for (face, values) in faces.iter_mut().enumerate() {
        for y in 0..resolution {
            if cancel.load(Ordering::Relaxed) {
                return Err(ReconstructionError::Cancelled);
            }
            for x in 0..resolution {
                let index = y * resolution + x;
                let direction = face_uv_to_direction(
                    face,
                    x as f32 / (resolution - 1) as f32,
                    y as f32 / (resolution - 1) as f32,
                );
                values.push(
                    sample_cubemap_height(meso_eroded, direction)
                        + full_uneroded.faces[face][index]
                        - sample_cubemap_height(meso_uneroded, direction),
                );
            }
        }
    }
    Ok(TectonicTerrain {
        faces,
        resolution: full_uneroded.resolution,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_export(
    gpu: &GpuContext,
    config: &ExportConfig,
    params: &PlanetParams,
    derived: &DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: TerrainGenerationParams,
    progress_tx: &Sender<ExportProgress>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let mut timings = ExportTimings::default();
    run_export_with_timings(
        gpu,
        config,
        params,
        derived,
        continental_scale,
        water_loss,
        terrain_params,
        progress_tx,
        cancel,
        &mut timings,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_export_with_timings(
    gpu: &GpuContext,
    config: &ExportConfig,
    params: &PlanetParams,
    derived: &DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: TerrainGenerationParams,
    progress_tx: &Sender<ExportProgress>,
    cancel: &AtomicBool,
    timings: &mut ExportTimings,
) -> Result<PathBuf, String> {
    let mut no_checkpoints = discard_layer_materialization_checkpoint;
    run_export_with_timings_and_checkpoints(
        gpu,
        config,
        params,
        derived,
        continental_scale,
        water_loss,
        terrain_params,
        progress_tx,
        cancel,
        timings,
        &mut no_checkpoints,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn run_export_with_timings_and_checkpoints(
    gpu: &GpuContext,
    config: &ExportConfig,
    params: &PlanetParams,
    derived: &DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: TerrainGenerationParams,
    progress_tx: &Sender<ExportProgress>,
    cancel: &AtomicBool,
    timings: &mut ExportTimings,
    checkpoints: &mut (dyn for<'a> FnMut(&'a LayerMaterializationCheckpoint) -> Result<(), String>
              + '_),
) -> Result<PathBuf, String> {
    *timings = ExportTimings::default();
    let tile_size = select_tile_size(
        config.face_resolution,
        config.tile_size,
        &gpu.device.limits(),
        16,
    )?;
    let coordinator = TileCoordinator::try_new(config.face_resolution, tile_size)?;
    let projected_live_bytes = estimated_export_preflight_bytes_with_erosion(
        config.face_resolution,
        &config.layers,
        config.erosion_iterations,
        &gpu.device.limits(),
    )?;
    validate_8k_owned_live_bytes(projected_live_bytes)?;

    // Create output directory
    let planet_dir = config.output_dir.join(&config.planet_name);
    std::fs::create_dir_all(&planet_dir)
        .map_err(|e| format!("Failed to create output dir: {e}"))?;

    let effective_ocean = derived.ocean_fraction * (1.0 - water_loss);
    let ocean_level = -0.5 + 1.7 * effective_ocean; // match app.rs formula

    // Compute total steps for progress based on selected layers
    let layers = &config.layers;
    let tiles_per_face = coordinator.tiles_per_face();
    let map_count = [
        layers.normals,
        layers.roughness,
        layers.albedo,
        layers.albedo, /*ao bundled with albedo*/
        layers.clouds,
    ]
    .iter()
    .filter(|&&b| b)
    .count() as u32;
    let export_count = [
        layers.height,
        layers.albedo,
        layers.normals,
        layers.roughness,
        layers.albedo, /*ao*/
        layers.water_mask,
        layers.clouds,
    ]
    .iter()
    .filter(|&&b| b)
    .count() as u32;
    let total_steps = coordinator.total_tiles() // terrain generation
        + 6 // erosion
        + map_count * 6 * tiles_per_face // map tiles for selected layers
        + 6 * export_count; // file writes
    let mut progress = ProgressTracker::new(progress_tx, total_steps);

    let generation_started = Instant::now();
    // --- Phase 1: Generate plates ---
    let plates = generate_plates(&PlateGenParams {
        seed: params.seed,
        mass_earth: params.mass_earth,
        ocean_fraction: effective_ocean,
        tectonics_factor: derived.tectonics_factor,
        continental_scale,
        num_plates_override: terrain_params.num_plates_override,
        num_continents: terrain_params.num_continents,
        continent_size_variety: terrain_params.continent_size_variety,
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
        &terrain_params,
        &mut progress,
        cancel,
    )?;
    timings.generation_inclusive_ms = generation_started.elapsed().as_secs_f64() * 1000.0;
    timings.generation_completed = true;

    // --- Phase 3: Erosion ---
    let erosion_started = Instant::now();
    let erosion_pipeline = ErosionPipeline::new(gpu);
    for face in 0..6u32 {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        progress.advance(&format!("Eroding face {face}"));
    }
    if config.face_resolution == 8192 {
        let meso_tile_size = select_tile_size(
            MESO_EROSION_RESOLUTION,
            config.tile_size,
            &gpu.device.limits(),
            16,
        )?;
        let meso_coordinator = TileCoordinator::try_new(MESO_EROSION_RESOLUTION, meso_tile_size)?;
        let mut meso_terrain = generate_terrain_tiled(
            gpu,
            &terrain_pipeline,
            &plates_buffer,
            &meso_coordinator,
            plates.len() as u32,
            &terrain_params,
            &mut progress,
            cancel,
        )?;
        let meso_uneroded = meso_terrain.clone();
        erosion_pipeline
            .erode_with_cancel(
                gpu,
                &mut meso_terrain,
                config.erosion_iterations,
                ocean_level,
                cancel,
            )
            .map_err(|error| format!("meso erosion preflight failed: {error}"))?;
        timings.meso_erosion_resolution = Some(MESO_EROSION_RESOLUTION);
        timings.meso_erosion_ms = erosion_started.elapsed().as_secs_f64() * 1000.0;
        timings.meso_erosion_completed = true;
        let reconstruction_started = Instant::now();
        terrain = reconstruct_8k_from_meso_delta_with_cancel(
            &terrain,
            &meso_uneroded,
            &meso_terrain,
            cancel,
        )
        .map_err(|error| error.to_string())?;
        timings.delta_reconstruction_ms = reconstruction_started.elapsed().as_secs_f64() * 1000.0;
        timings.delta_reconstruction_completed = true;
    } else {
        erosion_pipeline
            .erode_with_cancel(
                gpu,
                &mut terrain,
                config.erosion_iterations,
                ocean_level,
                cancel,
            )
            .map_err(|error| format!("erosion preflight failed: {error}"))?;
    }
    timings.erosion_inclusive_ms = erosion_started.elapsed().as_secs_f64() * 1000.0;
    timings.erosion_completed = true;

    // --- Phase 4: Create map pipelines (only for selected layers) ---
    let export_started = Instant::now();
    let normal_pipeline = if layers.normals {
        Some(MapPipeline::new(
            gpu,
            include_str!("shaders/normal_map.wgsl"),
            "normal map",
            16,
        ))
    } else {
        None
    };
    let roughness_pipeline = if layers.roughness {
        Some(MapPipeline::new(
            gpu,
            &format!(
                "{}\n{}\n{}",
                include_str!("shaders/cube_sphere.wgsl"),
                include_str!("shaders/noise.wgsl"),
                include_str!("shaders/roughness_map.wgsl"),
            ),
            "roughness map",
            16,
        ))
    } else {
        None
    };
    let albedo_pipeline = if layers.albedo {
        Some(MapPipeline::new(
            gpu,
            &format!(
                "{}\n{}\n{}",
                include_str!("shaders/cube_sphere.wgsl"),
                include_str!("shaders/noise.wgsl"),
                include_str!("shaders/albedo_map.wgsl"),
            ),
            "albedo map",
            16,
        ))
    } else {
        None
    };
    let ao_pipeline = if layers.albedo {
        Some(MapPipeline::new(
            gpu,
            include_str!("shaders/ao_map.wgsl"),
            "ao map",
            8,
        ))
    } else {
        None
    };
    let cloud_pipeline = if layers.clouds {
        Some(MapPipeline::new(
            gpu,
            &format!(
                "{}\n{}\n{}",
                include_str!("shaders/cube_sphere.wgsl"),
                include_str!("shaders/noise.wgsl"),
                include_str!("shaders/cloud_map.wgsl"),
            ),
            "cloud map",
            16,
        ))
    } else {
        None
    };
    let emission_pipeline = if layers.emission {
        Some(MapPipeline::new(
            gpu,
            &format!(
                "{}\n{}\n{}",
                include_str!("shaders/cube_sphere.wgsl"),
                include_str!("shaders/noise.wgsl"),
                include_str!("shaders/emission_map.wgsl"),
            ),
            "emission map",
            16,
        ))
    } else {
        None
    };

    // --- Phase 5: Stream one selected layer at a time ---
    let full_res = coordinator.face_resolution;
    let eq_w = full_res * 2;
    let eq_ht = full_res;

    if layers.height {
        progress.advance("Exporting height...");
        let stage = stage_terrain_faces(&planet_dir, &terrain, &coordinator, cancel)?;
        merge_staged_export(
            timings,
            export_staged_equirect_exr(
                "height",
                &stage,
                eq_w,
                eq_ht,
                1,
                &planet_dir.join("height.exr"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if let Some(pipeline) = normal_pipeline.as_ref() {
        let stage = stage_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |_, region| NormalMapParams {
                resolution: region.width,
                height_scale: 50.0,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: full_res,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
            16,
            &mut progress,
            "Normal",
            cancel,
            &planet_dir,
            &mut timings.map_batch_metrics,
        )?;
        progress.advance("Exporting normals...");
        merge_staged_export(
            timings,
            export_staged_equirect_exr(
                "normal",
                &stage,
                eq_w,
                eq_ht,
                4,
                &planet_dir.join("normal.exr"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if let Some(pipeline) = roughness_pipeline.as_ref() {
        let stage = stage_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |face, region| RoughnessMapParams {
                face,
                resolution: region.width,
                seed: params.seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                ocean_fraction: effective_ocean,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: full_res,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
            4,
            &mut progress,
            "Roughness",
            cancel,
            &planet_dir,
            &mut timings.map_batch_metrics,
        )?;
        progress.advance("Exporting roughness...");
        merge_staged_export(
            timings,
            export_staged_equirect_png(
                "roughness",
                &stage,
                eq_w,
                eq_ht,
                QuantizedLayer::Roughness,
                &planet_dir.join("roughness.png"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if let Some(pipeline) = albedo_pipeline.as_ref() {
        let stage = stage_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |face, region| AlbedoMapParams {
                face,
                resolution: region.width,
                seed: params.seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                ocean_fraction: effective_ocean,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                season: config.season,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: full_res,
                local_height: region.height,
            },
            16,
            &mut progress,
            "Albedo",
            cancel,
            &planet_dir,
            &mut timings.map_batch_metrics,
        )?;
        progress.advance("Exporting albedo...");
        merge_staged_export(
            timings,
            export_staged_equirect_png(
                "albedo",
                &stage,
                eq_w,
                eq_ht,
                QuantizedLayer::Albedo,
                &planet_dir.join("albedo.png"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if let Some(pipeline) = ao_pipeline.as_ref() {
        let stage = stage_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |face, region| AoMapParams {
                face,
                full_resolution: full_res,
                ao_strength: 30.0,
                ocean_level,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                resolution: region.width,
                local_height: region.height,
            },
            4,
            &mut progress,
            "AO",
            cancel,
            &planet_dir,
            &mut timings.map_batch_metrics,
        )?;
        progress.advance("Exporting AO...");
        merge_staged_export(
            timings,
            export_staged_equirect_png(
                "ao",
                &stage,
                eq_w,
                eq_ht,
                QuantizedLayer::AmbientOcclusion,
                &planet_dir.join("ao.png"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if layers.water_mask {
        let stage =
            stage_ocean_mask_faces(&planet_dir, &terrain, &coordinator, ocean_level, cancel)?;
        progress.advance("Exporting water mask...");
        merge_staged_export(
            timings,
            export_staged_equirect_png(
                "water_mask",
                &stage,
                eq_w,
                eq_ht,
                QuantizedLayer::WaterMask,
                &planet_dir.join("water_mask.png"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    if let Some(pipeline) = cloud_pipeline.as_ref() {
        let stage = stage_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |face, region| CloudMapParams {
                face,
                resolution: region.width,
                seed: config.cloud_seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                ocean_fraction: effective_ocean,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                season: config.season,
                cloud_coverage: config.cloud_coverage,
                cloud_type: config.cloud_type,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: full_res,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
            4,
            &mut progress,
            "Clouds",
            cancel,
            &planet_dir,
            &mut timings.map_batch_metrics,
        )?;
        progress.advance("Exporting clouds...");
        merge_staged_export(
            timings,
            export_staged_equirect_png(
                "clouds",
                &stage,
                eq_w,
                eq_ht,
                QuantizedLayer::CloudDensity,
                &planet_dir.join("clouds.png"),
                cancel,
                checkpoints,
            ),
        )?;
    }

    timings.map_batch_metrics.completed = timings.map_batch_metrics.reached;
    timings.staged_io_completed = timings.staged_io_metrics.rows_generated > 0;

    if let Some(pipeline) = emission_pipeline.as_ref() {
        let (emission, _, _) = stream_map_layer(
            gpu,
            pipeline,
            &terrain,
            &coordinator,
            |face, region| EmissionMapParams {
                face,
                resolution: region.width,
                seed: params.seed,
                base_temp_c: derived.base_temperature_c,
                ocean_level,
                night_lights: config.night_lights,
                axial_tilt_rad: params.axial_tilt_deg.to_radians(),
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: full_res,
                local_height: region.height,
                _pad1: 0,
            },
            4,
            &mut progress,
            "Emission",
            cancel,
        )?;
        progress.advance("Exporting emission...");
        export_equirect_exr_gray(&emission, eq_w, eq_ht, &planet_dir.join("emission.exr"))?;
    }

    timings.export_inclusive_ms = export_started.elapsed().as_secs_f64() * 1000.0;
    timings.export_completed = true;
    let _ = progress_tx.send(ExportProgress::Complete);
    Ok(planet_dir)
}

// ============ Background Thread Launcher ============

pub struct ExportHandle {
    pub progress_rx: std::sync::mpsc::Receiver<ExportProgress>,
    pub cancel: Arc<AtomicBool>,
    pub thread: std::thread::JoinHandle<()>,
}

/// Export a ring gradient texture as a 4K×1 RGBA PNG.
/// Radial position 0 (inner) to width-1 (outer) maps to color + alpha.
pub fn export_ring_gradient(output_dir: &Path, width: u32) -> Result<PathBuf, String> {
    let height = 1u32;
    let mut pixels = Vec::with_capacity((width * 4) as usize);

    for x in 0..width {
        let frac = x as f32 / (width - 1) as f32; // 0=inner, 1=outer

        // Density: inner bright, outer faint, with Cassini-like gaps
        let base_density = (1.0 - frac) * 0.85 + 0.15;
        let gap1 = 1.0 - (1.0 - ((frac - 0.37) * 30.0).abs().min(1.0)) * 0.7;
        let gap2 = 1.0 - (1.0 - ((frac - 0.67) * 40.0).abs().min(1.0)) * 0.5;
        let density = base_density * gap1 * gap2;

        // Color: warm ice/dust tones
        let r = (0.75 + 0.15 * frac).min(1.0);
        let g = (0.68 + 0.17 * frac).min(1.0);
        let b = (0.55 + 0.20 * frac).min(1.0);

        pixels.push((r * 255.0) as u8);
        pixels.push((g * 255.0) as u8);
        pixels.push((b * 255.0) as u8);
        pixels.push((density * 255.0) as u8); // alpha = density
    }

    let path = output_dir.join("ring_gradient.png");
    let img =
        image::RgbaImage::from_raw(width, height, pixels).ok_or("Failed to create ring image")?;
    img.save(&path)
        .map_err(|e| format!("Failed to save ring gradient: {e}"))?;
    Ok(path)
}

pub fn spawn_export(
    gpu: Arc<GpuContext>,
    config: ExportConfig,
    params: PlanetParams,
    derived: DerivedProperties,
    continental_scale: f32,
    water_loss: f32,
    terrain_params: TerrainGenerationParams,
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
    fn eight_k_staged_sampler_cache_remains_bounded_below_one_face() {
        let (region_side, cache_regions) = staged_sampler_config(8192, 4).unwrap();
        assert_eq!(region_side, 512);
        assert_eq!(cache_regions, STAGED_SAMPLE_CACHE_REGIONS);
        let cache_values = cache_regions * region_side as usize * region_side as usize * 4;
        assert!(cache_values < 8192_usize * 8192 * 4);
    }

    fn directional_terrain(resolution: u32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|face| {
                (0..resolution as usize * resolution as usize)
                    .map(|index| {
                        let x = index % resolution as usize;
                        let y = index / resolution as usize;
                        let direction = face_uv_to_direction(
                            face,
                            x as f32 / (resolution - 1) as f32,
                            y as f32 / (resolution - 1) as f32,
                        );
                        direction[0] + direction[1] + direction[2]
                    })
                    .collect()
            }),
            resolution,
        }
    }

    #[test]
    fn reconstruction_keeps_zero_delta_continuous_across_cube_edges() {
        let full = directional_terrain(8);
        let meso = directional_terrain(2);
        let reconstructed = reconstruct_8k_from_meso_delta(&full, &meso, &meso).unwrap();
        for (actual, expected) in reconstructed.faces.iter().zip(&full.faces) {
            assert!(actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| (actual - expected).abs() < 1e-6));
        }
        for direction in [[1.0, 0.0, 1.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0]] {
            let before = sample_cubemap_height(&full, direction);
            let after = sample_cubemap_height(&reconstructed, direction);
            assert!((before - after).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn reconstruction_preserves_nonzero_delta_across_all_cube_edges_and_corners() {
        let full = directional_terrain(8);
        let meso = directional_terrain(2);
        let mut eroded = meso.clone();
        for (face, values) in eroded.faces.iter_mut().enumerate() {
            for (index, value) in values.iter_mut().enumerate() {
                let x = index % meso.resolution as usize;
                let y = index / meso.resolution as usize;
                let direction = face_uv_to_direction(
                    face,
                    x as f32 / (meso.resolution - 1) as f32,
                    y as f32 / (meso.resolution - 1) as f32,
                );
                let length = (direction[0] * direction[0]
                    + direction[1] * direction[1]
                    + direction[2] * direction[2])
                    .sqrt();
                *value += 0.15 * (direction[0] / length) + 0.1 * (direction[1] / length).powi(2)
                    - 0.05 * (direction[2] / length);
            }
        }
        let first = reconstruct_8k_from_meso_delta(&full, &meso, &eroded).unwrap();
        let second = reconstruct_8k_from_meso_delta(&full, &meso, &eroded).unwrap();
        assert_eq!(first.faces, second.faces);
        let mut boundary_values = std::collections::BTreeMap::<(i32, i32, i32), Vec<f32>>::new();
        for (face, values) in first.faces.iter().enumerate() {
            for y in 0..first.resolution as usize {
                for x in 0..first.resolution as usize {
                    if x != 0
                        && y != 0
                        && x + 1 != first.resolution as usize
                        && y + 1 != first.resolution as usize
                    {
                        continue;
                    }
                    let direction = face_uv_to_direction(
                        face,
                        x as f32 / (first.resolution - 1) as f32,
                        y as f32 / (first.resolution - 1) as f32,
                    );
                    let length = (direction[0] * direction[0]
                        + direction[1] * direction[1]
                        + direction[2] * direction[2])
                        .sqrt();
                    let key = (
                        (direction[0] / length * 1_000_000.0).round() as i32,
                        (direction[1] / length * 1_000_000.0).round() as i32,
                        (direction[2] / length * 1_000_000.0).round() as i32,
                    );
                    boundary_values
                        .entry(key)
                        .or_default()
                        .push(values[y * first.resolution as usize + x]);
                }
            }
        }
        let shared_boundaries = boundary_values
            .values()
            .filter(|values| values.len() > 1)
            .collect::<Vec<_>>();
        assert!(shared_boundaries.len() >= 12);
        for values in shared_boundaries {
            let min = values.iter().copied().fold(f32::INFINITY, f32::min);
            let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                (max - min).abs() < 1e-5,
                "shared edge/corner discontinuity: {values:?}"
            );
        }
    }

    #[test]
    fn reconstruction_returns_structured_cancellation_before_work() {
        let full = directional_terrain(8);
        let meso = directional_terrain(2);
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            reconstruct_8k_from_meso_delta_with_cancel(&full, &meso, &meso, &cancel),
            Err(ReconstructionError::Cancelled)
        ));
    }

    #[test]
    fn reserved_tile_at_exact_cap_is_inserted_without_double_counting() {
        let mut batch = MapTileBatch::new(128);
        batch.reserve(128).unwrap();
        batch.consume_reservation(128).unwrap();
        assert_eq!(batch.owned_bytes, 128);
        assert_eq!(batch.reserved_bytes, 0);
        assert!(batch.reserve(1).is_err());
    }

    #[test]
    fn pending_reservations_count_toward_batch_capacity() {
        let mut batch = MapTileBatch::new(1024);
        for _ in 0..MAP_BATCH_SIZE {
            batch.reserve(1).unwrap();
        }
        assert!(batch.reserve(1).is_err());
        batch.release_reservation(1).unwrap();
        assert!(batch.reserve(1).is_ok());
    }

    fn terrain_params() -> TerrainGenerationParams {
        TerrainGenerationParams {
            seed: 42,
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

    fn staged_fixture(
        root: &Path,
        resolution: u32,
        components: u32,
    ) -> (ExportStage, [Vec<f32>; 6]) {
        let faces = std::array::from_fn(|face| {
            (0..resolution * resolution * components)
                .map(|index| face as f32 * 1_000.0 + index as f32 * 0.125)
                .collect::<Vec<_>>()
        });
        let stage = ExportStage::create(
            root,
            StageMetadata {
                face_resolution: resolution,
                components,
                halo: 0,
            },
        )
        .unwrap();
        for (face, values) in faces.iter().enumerate() {
            stage
                .write_tile(face as u32, 0, 0, resolution, resolution, values)
                .unwrap();
        }
        (stage, faces)
    }

    fn read_rgba(path: &Path) -> Vec<[f32; 4]> {
        let image = exr::prelude::read_first_rgba_layer_from_file(
            path,
            |size, _| vec![vec![[0.0; 4]; size.width()]; size.height()],
            |pixels, position, rgba: (f32, f32, f32, f32)| {
                pixels[position.y()][position.x()] = rgba.into()
            },
        )
        .unwrap();
        image
            .layer_data
            .channel_data
            .pixels
            .into_iter()
            .flatten()
            .collect()
    }

    fn normalized_staged_fixture(
        root: &Path,
        resolution: u32,
        components: u32,
    ) -> (ExportStage, [Vec<f32>; 6]) {
        let faces = std::array::from_fn(|face| {
            (0..resolution * resolution * components)
                .map(|index| ((face as u32 * 41 + index) % 251) as f32 / 250.0)
                .collect::<Vec<_>>()
        });
        let stage = ExportStage::create(
            root,
            StageMetadata {
                face_resolution: resolution,
                components,
                halo: 0,
            },
        )
        .unwrap();
        for (face, values) in faces.iter().enumerate() {
            stage
                .write_tile(face as u32, 0, 0, resolution, resolution, values)
                .unwrap();
        }
        (stage, faces)
    }

    fn read_png(path: &Path) -> (png::ColorType, png::BitDepth, Vec<u8>) {
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let color = reader.info().color_type;
        let depth = reader.info().bit_depth;
        let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
        let output = reader.next_frame(&mut bytes).unwrap();
        bytes.truncate(output.buffer_size());
        (color, depth, bytes)
    }

    fn assert_staged_png_matches_monolithic(layer: QuantizedLayer) {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-staged-png-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let resolution = 8;
        let (stage, faces) = normalized_staged_fixture(&root, resolution, layer.channels());
        let (monolithic, width, height) =
            cubemap_to_equirect(&faces, resolution, layer.channels() as usize);
        let path = root.join("layer.png");
        let mut checkpoints = discard_layer_materialization_checkpoint;
        let diagnostics = export_staged_equirect_png(
            "test",
            &stage,
            width,
            height,
            layer,
            &path,
            &AtomicBool::new(false),
            &mut checkpoints,
        )
        .unwrap();
        assert_eq!(diagnostics.rows_generated, u64::from(height));
        assert!(diagnostics.cache_hits > diagnostics.region_reads);
        let (region_side, cache_regions) =
            staged_sampler_config(resolution, layer.channels()).unwrap();
        let worker_cap = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(MAX_EQUIRECT_ROW_WORKERS);
        let row_config = equirect_row_materialization_config(
            width,
            height,
            layer.channels(),
            region_side,
            cache_regions,
            worker_cap,
        )
        .unwrap();
        assert!(diagnostics.peak_cached_values > 0);
        assert!(diagnostics.peak_cached_values <= row_config.aggregate_cache_values);
        assert_eq!(
            diagnostics.published_file_bytes,
            std::fs::metadata(&path).unwrap().len()
        );
        let (color, depth, actual) = read_png(&path);
        let expected: Vec<u8> = match layer.png_format() {
            PngRowFormat::Gray8 => monolithic
                .iter()
                .map(|value| (value * 255.0).round() as u8)
                .collect(),
            PngRowFormat::Gray16 => monolithic
                .iter()
                .flat_map(|value| ((value * 65535.0).round() as u16).to_be_bytes())
                .collect(),
            PngRowFormat::Rgba8 => monolithic
                .iter()
                .map(|value| (value * 255.0).round() as u8)
                .collect(),
        };
        assert_eq!(actual, expected);
        assert_eq!(
            (color, depth),
            match layer.png_format() {
                PngRowFormat::Gray8 => (png::ColorType::Grayscale, png::BitDepth::Eight),
                PngRowFormat::Gray16 => (png::ColorType::Grayscale, png::BitDepth::Sixteen),
                PngRowFormat::Rgba8 => (png::ColorType::Rgba, png::BitDepth::Eight),
            }
        );
        assert!(!path.with_extension("png.part").exists());
        drop(stage);
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.starts_with(".planet-gen-stage-")
                && !name.starts_with(".planet-gen-equirect-stage-")
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn staged_png_cancellation_keeps_atomic_output_unpublished() {
        let root =
            std::env::temp_dir().join(format!("planet-gen-staged-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (stage, _) = normalized_staged_fixture(&root, 8, 1);
        let path = root.join("cancelled.png");
        let cancel = AtomicBool::new(true);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            Ok(())
        };
        let failure = export_staged_equirect_png(
            "roughness",
            &stage,
            16,
            8,
            QuantizedLayer::Roughness,
            &path,
            &cancel,
            &mut checkpoints,
        )
        .unwrap_err();
        assert_eq!(failure.message, "Cancelled");
        assert_eq!(failure.metrics.rows_generated, 0);
        assert_eq!(failure.metrics.published_file_bytes, 0);
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].layer, "roughness");
        assert_eq!(recorded[0].rows_completed, 0);
        assert_eq!(recorded[0].rows_total, 8);
        assert_eq!(recorded[0].reason.as_deref(), Some("Cancelled"));
        assert!(!path.exists());
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".planet-gen-equirect-stage-")
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_intermediate_finish_never_emits_a_completion_checkpoint() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-staged-finish-failure-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let (stage, _) = staged_fixture(&root, 8, 1);
        let cancel = AtomicBool::new(false);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            Ok(())
        };
        let failure = match materialize_staged_equirect_with_worker_cap(
            "height",
            &stage,
            16,
            8,
            1,
            &root,
            &cancel,
            &mut checkpoints,
            2,
            |_| Err("flush failed".into()),
        ) {
            Ok(_) => panic!("injected finish failure unexpectedly succeeded"),
            Err(failure) => failure,
        };
        assert_eq!(failure.message, "flush failed");
        assert_eq!(failure.metrics.rows_generated, 8);
        assert_eq!(failure.metrics.intermediate_write_bytes, 16 * 8 * 4);
        assert_eq!(recorded.len(), 2);
        assert!(recorded.iter().all(|checkpoint| !checkpoint.completed));
        assert_eq!(recorded[0].reason, None);
        assert_eq!(recorded[1].reason.as_deref(), Some("flush failed"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_materialization_matches_sequential_sampling_order() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-parallel-equirect-parity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let resolution = 8;
        let channels = 4;
        let (stage, faces) = staged_fixture(&root, resolution, channels);
        let (expected, width, height) = cubemap_to_equirect(&faces, resolution, channels as usize);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            Ok(())
        };
        let (mut intermediate, metrics) = materialize_staged_equirect_with_worker_cap(
            "normal",
            &stage,
            width,
            height,
            channels,
            &root,
            &AtomicBool::new(false),
            &mut checkpoints,
            2,
            EquirectStage::finish,
        )
        .unwrap();
        let actual = (0..height)
            .flat_map(|y| intermediate.read_row(y).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert_eq!(metrics.rows_generated, u64::from(height));
        assert!(metrics.peak_cached_values > 0);
        assert!(metrics.peak_cached_values <= 2 * 7 * 7 * channels as usize);
        assert_eq!(
            recorded.last().unwrap().peak_cached_bytes,
            metrics.peak_cached_bytes()
        );
        drop(intermediate);
        drop(stage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_materialization_is_byte_exact_across_runs() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-parallel-equirect-determinism-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (stage, _) = staged_fixture(&root, 8, 1);
        let materialize = |stage: &ExportStage| {
            let mut checkpoints = discard_layer_materialization_checkpoint;
            let (mut intermediate, _) = materialize_staged_equirect_with_worker_cap(
                "height",
                stage,
                16,
                8,
                1,
                &root,
                &AtomicBool::new(false),
                &mut checkpoints,
                2,
                EquirectStage::finish,
            )
            .unwrap();
            (0..8)
                .flat_map(|y| intermediate.read_row(y).unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(materialize(&stage), materialize(&stage));
        drop(stage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parallel_materialization_resources_are_bounded_under_four_gib() {
        let config = equirect_row_materialization_config(16_384, 8_192, 4, 512, 8, 8).unwrap();
        assert_eq!(config.workers, 8);
        assert_eq!(config.queue_bound, 16);
        assert_eq!(config.aggregate_cache_values, 8 * 8 * 512 * 512 * 4);
        assert!(config.resource_bytes <= MAX_OWNED_LIVE_BYTES);
        assert!(equirect_row_materialization_config(16, 8, 1, 2, 1, 0).is_err());
    }

    #[test]
    fn parallel_materialization_cancellation_preserves_ordered_progress_and_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-parallel-equirect-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (stage, _) = staged_fixture(&root, 8, 1);
        let cancel = AtomicBool::new(true);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            Ok(())
        };
        let failure = match materialize_staged_equirect_with_worker_cap(
            "height",
            &stage,
            16,
            8,
            1,
            &root,
            &cancel,
            &mut checkpoints,
            2,
            EquirectStage::finish,
        ) {
            Ok(_) => panic!("cancelled parallel materialization unexpectedly succeeded"),
            Err(failure) => failure,
        };
        assert_eq!(failure.message, "Cancelled");
        assert_eq!(failure.metrics.rows_generated, 0);
        assert_eq!(failure.metrics.peak_cached_values, 0);
        assert!(failure.metrics.cache_capacity_values > 0);
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].rows_completed, 0);
        assert_eq!(recorded[0].peak_cached_bytes, 0);
        assert!(recorded[0].cache_capacity_bytes > 0);
        assert_eq!(recorded[0].reason.as_deref(), Some("Cancelled"));
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".planet-gen-equirect-stage-")
        }));
        drop(stage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn midflight_cancellation_merges_worker_cache_peaks_before_terminal_checkpoint() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-parallel-equirect-midflight-cancel-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (stage, _) = staged_fixture(&root, 8, 1);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_at_checkpoint = Arc::clone(&cancel);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            if checkpoint.rows_completed == MATERIALIZATION_CHECKPOINT_ROW_INTERVAL {
                cancel_at_checkpoint.store(true, Ordering::Relaxed);
            }
            Ok(())
        };
        let failure = match materialize_staged_equirect_with_worker_cap(
            "height",
            &stage,
            16,
            MATERIALIZATION_CHECKPOINT_ROW_INTERVAL * 2,
            1,
            &root,
            &cancel,
            &mut checkpoints,
            2,
            EquirectStage::finish,
        ) {
            Ok(_) => panic!("mid-flight cancellation unexpectedly succeeded"),
            Err(failure) => failure,
        };
        let terminal = recorded.last().unwrap();
        assert_eq!(failure.message, "Cancelled");
        assert_eq!(
            terminal.rows_completed,
            MATERIALIZATION_CHECKPOINT_ROW_INTERVAL
        );
        assert_eq!(terminal.reason.as_deref(), Some("Cancelled"));
        assert!(failure.metrics.peak_cached_values > 0);
        assert_eq!(
            terminal.peak_cached_bytes,
            failure.metrics.peak_cached_bytes()
        );
        drop(stage);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn staged_export_failure_merges_partial_metrics_before_propagating() {
        let mut timings = ExportTimings::default();
        let mut metrics = StagedIoMetrics::default();
        record_sampler_diagnostics(
            &mut metrics,
            crate::export_staging::StagedSamplerDiagnostics {
                cache_hits: 8,
                region_reads: 2,
                rows_generated: 3,
                row_values: 48,
                peak_cached_values: 16,
                ..Default::default()
            },
        );
        metrics.output_write_bytes = 96;
        let failure = StagedExportFailure {
            message: "write failed".into(),
            metrics,
        };
        assert_eq!(
            merge_staged_export(&mut timings, Err(failure)),
            Err("write failed".into())
        );
        assert_eq!(timings.staged_io_metrics.rows_generated, 3);
        assert_eq!(timings.staged_io_metrics.output_write_bytes, 96);
        assert_eq!(timings.staged_io_metrics.cache_hits, 8);
        assert_eq!(timings.staged_io_metrics.region_reads, 2);
    }

    #[test]
    fn staged_wall_elapsed_excludes_parallel_worker_cpu_time() {
        let metrics = StagedIoMetrics {
            row_generation_ms: 240_000.0,
            worker_row_generation_ms: 1_301_222.0,
            output_write_ms: 800.0,
            output_finish_ms: 67.0,
            ..Default::default()
        };
        assert_eq!(metrics.stage_wall_elapsed_ms(), 240_867.0);
        assert!(metrics.stage_wall_elapsed_ms() < metrics.worker_row_generation_ms);
    }

    #[test]
    fn staged_png_policy_preserves_masks_and_uses_documented_scalar_depths() {
        assert_eq!(QuantizedLayer::WaterMask.png_format(), PngRowFormat::Gray8);
        assert_eq!(QuantizedLayer::Roughness.png_format(), PngRowFormat::Gray16);
        assert_eq!(
            QuantizedLayer::AmbientOcclusion.png_format(),
            PngRowFormat::Gray16
        );
        assert_eq!(
            QuantizedLayer::CloudDensity.png_format(),
            PngRowFormat::Gray16
        );
        assert_eq!(QuantizedLayer::Albedo.png_format(), PngRowFormat::Rgba8);
        assert_staged_png_matches_monolithic(QuantizedLayer::WaterMask);
        assert_staged_png_matches_monolithic(QuantizedLayer::Roughness);
        assert_staged_png_matches_monolithic(QuantizedLayer::Albedo);
    }

    #[test]
    fn staged_png_ledger_is_smaller_than_a_full_8k_rgba_image() {
        let staged = estimated_staged_equirect_bytes(8192, 4).unwrap();
        let full = 8192_u64 * 16_384 * 4 * std::mem::size_of::<f32>() as u64;
        assert!(staged < MAX_OWNED_LIVE_BYTES);
        assert!(staged < full);
    }

    #[test]
    fn owned_live_gate_accepts_four_gib_and_rejects_more_at_8k() {
        assert!(validate_8k_owned_live_bytes(MAX_8K_OWNED_LIVE_BYTES).is_ok());
        assert!(validate_8k_owned_live_bytes(MAX_8K_OWNED_LIVE_BYTES + 1).is_err());
    }

    #[test]
    fn preflight_includes_aggregate_tiled_erosion_buffers_at_8k() {
        let limits = wgpu::Limits::default();
        let layers = ExportLayers {
            height: true,
            albedo: true,
            normals: true,
            roughness: true,
            water_mask: true,
            clouds: true,
            emission: false,
        };
        let staged = estimated_export_preflight_bytes(8192, &layers).unwrap();
        let with_erosion =
            estimated_export_preflight_bytes_with_erosion(8192, &layers, 25, &limits).unwrap();
        assert!(with_erosion > staged);
        assert!(with_erosion > 2 * 1024 * 1024 * 1024);
        assert!(with_erosion <= MAX_8K_OWNED_LIVE_BYTES);
    }

    #[test]
    fn staged_height_and_normal_exr_match_current_full_export_and_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-staged-exr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let resolution = 8;
        let height_full = root.join("height-full.exr");
        let height_staged = root.join("height-staged.exr");
        let normal_full = root.join("normal-full.exr");
        let normal_staged = root.join("normal-staged.exr");
        let cancel = AtomicBool::new(false);
        let mut recorded = Vec::new();
        let mut checkpoints = |checkpoint: &LayerMaterializationCheckpoint| {
            recorded.push(checkpoint.clone());
            Ok(())
        };

        let (height_stage, height_faces) = staged_fixture(&root, resolution, 1);
        let (height, width, height_px) = cubemap_to_equirect(&height_faces, resolution, 1);
        export_equirect_exr_gray(&height, width, height_px, &height_full).unwrap();
        export_staged_equirect_exr(
            "height",
            &height_stage,
            width,
            height_px,
            1,
            &height_staged,
            &cancel,
            &mut checkpoints,
        )
        .unwrap();
        drop(height_stage);

        let (normal_stage, normal_faces) = staged_fixture(&root, resolution, 4);
        let (normal, width, height_px) = cubemap_to_equirect(&normal_faces, resolution, 4);
        export_equirect_exr_rgba(&normal, width, height_px, &normal_full).unwrap();
        export_staged_equirect_exr(
            "normal",
            &normal_stage,
            width,
            height_px,
            4,
            &normal_staged,
            &cancel,
            &mut checkpoints,
        )
        .unwrap();
        drop(normal_stage);

        assert_eq!(read_rgba(&height_staged), read_rgba(&height_full));
        assert_eq!(read_rgba(&normal_staged), read_rgba(&normal_full));
        assert!(height_staged.is_file());
        assert!(normal_staged.is_file());
        assert!(!height_staged.with_extension("exr.part").exists());
        assert!(!normal_staged.with_extension("exr.part").exists());
        assert_eq!(recorded.len(), 4);
        assert!(recorded
            .chunks_exact(2)
            .all(|pair| !pair[0].completed && pair[1].completed));
        assert!(recorded.iter().all(|checkpoint| {
            checkpoint.rows_completed == height_px
                && checkpoint.rows_total == height_px
                && checkpoint.intermediate_write_bytes > 0
        }));
        assert!(std::fs::read_dir(&root).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            !name.starts_with(".planet-gen-stage-")
                && !name.starts_with(".planet-gen-equirect-stage-")
        }));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn staged_equirect_ledger_is_bounded_at_8k() {
        let staged = estimated_staged_equirect_bytes(8192, 4).unwrap();
        let full = 8192_u64 * 16_384 * 4 * std::mem::size_of::<f32>() as u64;
        let layers = ExportLayers {
            height: true,
            albedo: false,
            normals: true,
            roughness: false,
            water_mask: false,
            clouds: false,
            emission: false,
        };
        assert!(staged < MAX_OWNED_LIVE_BYTES);
        assert!(staged < full);
        assert!(estimated_staged_export_peak_bytes(8192, &layers).unwrap() < MAX_OWNED_LIVE_BYTES);
        assert!(estimated_peak_streaming_bytes(8192, 16) > MAX_OWNED_LIVE_BYTES);
    }

    fn assert_map_parity<P: Pod>(
        gpu: &GpuContext,
        map_name: &str,
        shader_source: String,
        output_element_bytes: usize,
        make_params: impl Fn(TileRegion) -> P,
    ) {
        let resolution = 128;
        let heightmap = (0..resolution * resolution)
            .map(|index| ((index % resolution) as f32 / resolution as f32) - 0.5)
            .collect::<Vec<_>>();
        let (tx, _rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);
        let pipeline = MapPipeline::new(
            gpu,
            &shader_source,
            "map parity",
            if map_name == "ao" { 8 } else { 16 },
        );
        let full = TileCoordinator::new(resolution, resolution);
        let tiled = TileCoordinator::new(resolution, 32);
        let mut full_progress = ProgressTracker::new(&tx, full.tiles_per_face());
        let mut tiled_progress = ProgressTracker::new(&tx, tiled.tiles_per_face());
        let monolithic = generate_map_tiled(
            gpu,
            &pipeline,
            &heightmap,
            &full,
            &make_params,
            output_element_bytes,
            &mut full_progress,
            "parity",
            0,
            &cancel,
        )
        .unwrap();
        let tiled_output = generate_map_tiled(
            gpu,
            &pipeline,
            &heightmap,
            &tiled,
            make_params,
            output_element_bytes,
            &mut tiled_progress,
            "parity",
            0,
            &cancel,
        )
        .unwrap();
        let difference = monolithic
            .iter()
            .zip(&tiled_output)
            .position(|(left, right)| left != right);
        if let Some(byte) = difference {
            let start = byte / output_element_bytes * output_element_bytes;
            panic!(
                "{map_name} first differing output byte {byte}, monolithic={:?}, tiled={:?}",
                &monolithic[start..start + output_element_bytes],
                &tiled_output[start..start + output_element_bytes]
            );
        }
    }

    #[test]
    fn tile_local_gpu_maps_match_monolithic_output() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let resolution = 128;
        let coordinator = TileCoordinator::new(resolution, 32);
        assert_eq!(
            coordinator.stencil_radius,
            max_map_stencil_radius(resolution)
        );
        assert_ne!(
            coordinator.region(1, 0).width,
            coordinator.region(1, 0).height
        );
        assert_ne!(
            coordinator.region(0, 1).width,
            coordinator.region(0, 1).height
        );
        assert_ne!(
            coordinator.region(3, 1).width,
            coordinator.region(3, 1).height
        );
        assert_ne!(
            coordinator.region(1, 3).width,
            coordinator.region(1, 3).height
        );
        let shader = |map: &str| {
            format!(
                "{}\n{}\n{}",
                include_str!("shaders/cube_sphere.wgsl"),
                include_str!("shaders/noise.wgsl"),
                map,
            )
        };

        assert_map_parity(
            &gpu,
            "normal",
            shader(include_str!("shaders/normal_map.wgsl")),
            16,
            |region| NormalMapParams {
                resolution: region.width,
                height_scale: 50.0,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: resolution,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
        );
        assert_map_parity(
            &gpu,
            "roughness",
            shader(include_str!("shaders/roughness_map.wgsl")),
            4,
            |region| RoughnessMapParams {
                face: 0,
                resolution: region.width,
                seed: 42,
                base_temp_c: 15.0,
                ocean_level: 0.0,
                ocean_fraction: 0.7,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: resolution,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
        );
        assert_map_parity(
            &gpu,
            "albedo",
            shader(include_str!("shaders/albedo_map.wgsl")),
            16,
            |region| AlbedoMapParams {
                face: 0,
                resolution: region.width,
                seed: 42,
                base_temp_c: 15.0,
                ocean_level: 0.0,
                ocean_fraction: 0.7,
                axial_tilt_rad: 0.4,
                season: 0.5,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: resolution,
                local_height: region.height,
            },
        );
        assert_map_parity(
            &gpu,
            "ao",
            shader(include_str!("shaders/ao_map.wgsl")),
            4,
            |region| AoMapParams {
                face: 0,
                full_resolution: resolution,
                ao_strength: 30.0,
                ocean_level: 0.0,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                resolution: region.width,
                local_height: region.height,
            },
        );
        assert_map_parity(
            &gpu,
            "cloud",
            shader(include_str!("shaders/cloud_map.wgsl")),
            4,
            |region| CloudMapParams {
                face: 0,
                resolution: region.width,
                seed: 42,
                base_temp_c: 15.0,
                ocean_level: 0.0,
                ocean_fraction: 0.7,
                axial_tilt_rad: 0.4,
                season: 0.5,
                cloud_coverage: 0.5,
                cloud_type: 0.5,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: resolution,
                local_height: region.height,
                _pad1: 0,
                _pad2: 0,
            },
        );
        assert_map_parity(
            &gpu,
            "emission",
            shader(include_str!("shaders/emission_map.wgsl")),
            4,
            |region| EmissionMapParams {
                face: 0,
                resolution: region.width,
                seed: 42,
                base_temp_c: 15.0,
                ocean_level: 0.0,
                night_lights: 0.5,
                axial_tilt_rad: 0.4,
                tile_offset_x: region.origin_x,
                tile_offset_y: region.origin_y,
                full_resolution: resolution,
                local_height: region.height,
                _pad1: 0,
            },
        );
    }

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
    fn tiled_terrain_uses_preview_generation_params() {
        let gpu = GpuContext::new().expect("GPU init failed");
        let pipeline = TerrainComputePipeline::new(&gpu);

        let plates = generate_plates(&PlateGenParams {
            seed: 42,
            mass_earth: 1.0,
            ocean_fraction: 0.7,
            tectonics_factor: 0.85,
            continental_scale: 1.0,
            num_plates_override: 0,
            num_continents: 0,
            continent_size_variety: 0.0,
        });

        let terrain_params = TerrainGenerationParams {
            amplitude: 0.8,
            frequency: 1.4,
            octaves: 10,
            gain: 0.6,
            lacunarity: 2.1,
            mountain_scale: 1.3,
            boundary_width: 0.08,
            warp_strength: 0.7,
            detail_scale: 0.9,
            surface_gravity: 7.2,
            tectonics_factor: 0.4,
            surface_age: 0.6,
            continental_scale: 1.2,
            ..terrain_params()
        };

        let direct = pipeline.generate(
            &gpu,
            &plates,
            64,
            terrain_params.seed,
            terrain_params.amplitude,
            terrain_params.frequency,
            terrain_params.octaves,
            terrain_params.gain,
            terrain_params.lacunarity,
            terrain_params.mountain_scale,
            terrain_params.boundary_width,
            terrain_params.warp_strength,
            terrain_params.detail_scale,
            terrain_params.surface_gravity,
            terrain_params.tectonics_factor,
            terrain_params.surface_age,
            terrain_params.continental_scale,
        );

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
            &terrain_params,
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
            layers: ExportLayers::default(),
            cloud_coverage: 0.5,
            cloud_type: 0.5,
            cloud_seed: 42,
            night_lights: 0.5,
        };

        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = AtomicBool::new(false);

        let result = run_export(
            &gpu,
            &config,
            &params,
            &derived,
            1.0, // continental_scale
            0.0, // water_loss
            terrain_params(),
            &tx,
            &cancel,
        );

        assert!(result.is_ok(), "Export failed: {:?}", result.err());

        let planet_dir = tmp_dir.join("test_planet");
        // Equirectangular EXR output files
        assert!(planet_dir.join("height.exr").exists());
        assert!(planet_dir.join("albedo.png").exists());
        assert!(planet_dir.join("normal.exr").exists());
        assert!(planet_dir.join("roughness.png").exists());
        assert!(planet_dir.join("water_mask.png").exists());
        assert!(planet_dir.join("ao.png").exists());

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
            layers: ExportLayers::default(),
            cloud_coverage: 0.5,
            cloud_type: 0.5,
            cloud_seed: 42,
            night_lights: 0.5,
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
            terrain_params(),
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
            layers: ExportLayers::default(),
            cloud_coverage: 0.5,
            cloud_type: 0.5,
            cloud_seed: 42,
            night_lights: 0.5,
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
            terrain_params(),
            &tx,
            &cancel,
        );
        let elapsed = start.elapsed();

        assert!(
            result.is_ok(),
            "Benchmark export failed: {:?}",
            result.err()
        );
        println!("2K export completed in {:.2}s", elapsed.as_secs_f64());
        println!("GPU: {}", gpu.adapter_name());

        // At 2K, should complete well under 30s even on modest hardware
        assert!(
            elapsed.as_secs() < 60,
            "2K export took too long: {:.1}s (target: <60s)",
            elapsed.as_secs_f64()
        );

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
