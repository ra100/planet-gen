use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const ROW_CANCEL_CHECK_INTERVAL: u32 = 128;
pub const MAX_EQUIRECT_INTERMEDIATE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

static STAGE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StageMetadata {
    pub face_resolution: u32,
    pub components: u32,
    pub halo: u32,
}

pub struct FaceStage {
    root: PathBuf,
    metadata: StageMetadata,
    expected_face_bytes: u64,
    coverage_lock: Mutex<()>,
}

/// The on-disk backing store for a streamed cubemap export.
pub type ExportStage = FaceStage;

/// A bounded, temporary equirectangular f32 store used between cubemap sampling and encoding.
pub struct EquirectStage {
    root: PathBuf,
    width: u32,
    height: u32,
    components: u32,
    row_bytes: usize,
    next_row: u32,
    complete: bool,
    file: File,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StagedSamplerDiagnostics {
    pub cache_capacity: usize,
    pub max_cached_values: usize,
    pub cached_regions: usize,
    pub peak_cached_regions: usize,
    pub cached_values: usize,
    pub peak_cached_values: usize,
    pub cache_hits: u64,
    pub region_reads: u64,
    pub rows_generated: u64,
    pub row_values: u64,
}

struct CachedRegion {
    face: usize,
    origin_x: u32,
    origin_y: u32,
    width: u32,
    values: Vec<f32>,
    last_used: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RegionKey {
    face: usize,
    x: u32,
    y: u32,
}

#[derive(Clone, Copy)]
struct BilinearStencil {
    face: usize,
    x: u32,
    y: u32,
    frac_x: f32,
    frac_y: f32,
}

/// Samples staged cubemap faces without materializing any complete face in memory.
pub struct StagedCubemapSampler<'stage> {
    stage: &'stage ExportStage,
    region_side: u32,
    cache: Vec<CachedRegion>,
    region_slots: HashMap<RegionKey, usize>,
    clock: u64,
    diagnostics: StagedSamplerDiagnostics,
}

impl<'stage> StagedCubemapSampler<'stage> {
    pub fn new(
        stage: &'stage ExportStage,
        region_side: u32,
        cache_capacity: usize,
    ) -> Result<Self, String> {
        let resolution = stage.metadata.face_resolution;
        if resolution <= 2 || region_side == 0 || cache_capacity == 0 {
            return Err(
                "staged sampler requires faces larger than 2 pixels and a nonzero cache".into(),
            );
        }
        let region_side = region_side.min(resolution - 1).max(2);
        let max_cached_values = cache_capacity
            .checked_mul(region_side as usize)
            .and_then(|value| value.checked_mul(region_side as usize))
            .and_then(|value| value.checked_mul(stage.metadata.components as usize))
            .ok_or("staged sampler cache size overflows usize")?;
        let face_values = (resolution as usize)
            .checked_mul(resolution as usize)
            .and_then(|value| value.checked_mul(stage.metadata.components as usize))
            .ok_or("staged face size exceeds usize")?;
        if max_cached_values >= face_values {
            return Err("staged sampler cache must remain smaller than one face".into());
        }
        for face in 0..6 {
            stage.validate_face_coverage(face)?;
        }
        Ok(Self {
            stage,
            region_side,
            cache: Vec::with_capacity(cache_capacity),
            region_slots: HashMap::with_capacity(cache_capacity),
            clock: 0,
            diagnostics: StagedSamplerDiagnostics {
                cache_capacity,
                max_cached_values,
                ..Default::default()
            },
        })
    }

    pub fn sample_equirect_pixel(&mut self, x: u32, y: u32, channel: u32) -> Result<f32, String> {
        let resolution = self.stage.metadata.face_resolution;
        let equirect_width = resolution
            .checked_mul(2)
            .ok_or("staged equirect width overflows u32")?;
        if x >= equirect_width || y >= resolution || channel >= self.stage.metadata.components {
            return Err("staged equirect sample is outside its bounds".into());
        }
        let [dx, dy, dz] = crate::export::equirect_pixel_to_direction(
            x as usize,
            y as usize,
            equirect_width as usize,
            resolution as usize,
        );
        self.sample_direction(dx, dy, dz, channel)
    }

    pub fn sample_direction(
        &mut self,
        dx: f32,
        dy: f32,
        dz: f32,
        channel: u32,
    ) -> Result<f32, String> {
        if channel >= self.stage.metadata.components {
            return Err("staged cubemap channel is outside its bounds".into());
        }
        if !dx.is_finite()
            || !dy.is_finite()
            || !dz.is_finite()
            || (dx == 0.0 && dy == 0.0 && dz == 0.0)
        {
            return Err("staged cubemap direction must be finite and nonzero".into());
        }
        self.sample_stencil_channel(self.bilinear_stencil(dx, dy, dz)?, channel)
    }

    /// Samples each leading component with one direction-to-face conversion and stencil.
    pub fn sample_equirect_pixel_channels(
        &mut self,
        x: u32,
        y: u32,
        values: &mut [f32],
    ) -> Result<(), String> {
        let resolution = self.stage.metadata.face_resolution;
        let equirect_width = resolution
            .checked_mul(2)
            .ok_or("staged equirect width overflows u32")?;
        if x >= equirect_width || y >= resolution {
            return Err("staged equirect sample is outside its bounds".into());
        }
        let [dx, dy, dz] = crate::export::equirect_pixel_to_direction(
            x as usize,
            y as usize,
            equirect_width as usize,
            resolution as usize,
        );
        self.sample_direction_channels(dx, dy, dz, values)
    }

    /// Samples each leading component with one direction-to-face conversion and stencil.
    pub fn sample_direction_channels(
        &mut self,
        dx: f32,
        dy: f32,
        dz: f32,
        values: &mut [f32],
    ) -> Result<(), String> {
        if values.is_empty() || values.len() > self.stage.metadata.components as usize {
            return Err("staged cubemap component count is outside its bounds".into());
        }
        self.sample_stencil_channels(self.bilinear_stencil(dx, dy, dz)?, values)
    }

    pub fn diagnostics(&self) -> StagedSamplerDiagnostics {
        self.diagnostics
    }

    fn bilinear_stencil(&self, dx: f32, dy: f32, dz: f32) -> Result<BilinearStencil, String> {
        if !dx.is_finite()
            || !dy.is_finite()
            || !dz.is_finite()
            || (dx == 0.0 && dy == 0.0 && dz == 0.0)
        {
            return Err("staged cubemap direction must be finite and nonzero".into());
        }
        let (face, u, v) = crate::export::direction_to_face_uv(dx, dy, dz);
        let resolution = self.stage.metadata.face_resolution as usize;
        let fx = u * (resolution - 1) as f32;
        let fy = v * (resolution - 1) as f32;
        let ix = (fx as usize).min(resolution - 2) as u32;
        let iy = (fy as usize).min(resolution - 2) as u32;
        Ok(BilinearStencil {
            face,
            x: ix,
            y: iy,
            frac_x: fx - ix as f32,
            frac_y: fy - iy as f32,
        })
    }

    fn sample_stencil_channel(
        &mut self,
        stencil: BilinearStencil,
        channel: u32,
    ) -> Result<f32, String> {
        let top_left = self.read_value(stencil.face, stencil.x, stencil.y, channel)?;
        let top_right = self.read_value(stencil.face, stencil.x + 1, stencil.y, channel)?;
        let bottom_left = self.read_value(stencil.face, stencil.x, stencil.y + 1, channel)?;
        let bottom_right = self.read_value(stencil.face, stencil.x + 1, stencil.y + 1, channel)?;
        let top = top_left + (top_right - top_left) * stencil.frac_x;
        let bottom = bottom_left + (bottom_right - bottom_left) * stencil.frac_x;
        Ok(top + (bottom - top) * stencil.frac_y)
    }

    fn sample_stencil_channels(
        &mut self,
        stencil: BilinearStencil,
        values: &mut [f32],
    ) -> Result<(), String> {
        let mut corners = [[0.0; 4]; 4];
        if values.len() > corners[0].len() {
            return self.sample_stencil_channels_heap(stencil, values);
        }
        self.copy_components(
            stencil.face,
            stencil.x,
            stencil.y,
            &mut corners[0][..values.len()],
        )?;
        self.copy_components(
            stencil.face,
            stencil.x + 1,
            stencil.y,
            &mut corners[1][..values.len()],
        )?;
        self.copy_components(
            stencil.face,
            stencil.x,
            stencil.y + 1,
            &mut corners[2][..values.len()],
        )?;
        self.copy_components(
            stencil.face,
            stencil.x + 1,
            stencil.y + 1,
            &mut corners[3][..values.len()],
        )?;
        for (channel, value) in values.iter_mut().enumerate() {
            let top =
                corners[0][channel] + (corners[1][channel] - corners[0][channel]) * stencil.frac_x;
            let bottom =
                corners[2][channel] + (corners[3][channel] - corners[2][channel]) * stencil.frac_x;
            *value = top + (bottom - top) * stencil.frac_y;
        }
        Ok(())
    }

    fn sample_stencil_channels_heap(
        &mut self,
        stencil: BilinearStencil,
        values: &mut [f32],
    ) -> Result<(), String> {
        let corner_values = values
            .len()
            .checked_mul(4)
            .ok_or("staged cubemap component scratch size overflows usize")?;
        let mut corners = vec![0.0; corner_values];
        let (top_left, rest) = corners.split_at_mut(values.len());
        let (top_right, rest) = rest.split_at_mut(values.len());
        let (bottom_left, bottom_right) = rest.split_at_mut(values.len());
        self.copy_components(stencil.face, stencil.x, stencil.y, top_left)?;
        self.copy_components(stencil.face, stencil.x + 1, stencil.y, top_right)?;
        self.copy_components(stencil.face, stencil.x, stencil.y + 1, bottom_left)?;
        self.copy_components(stencil.face, stencil.x + 1, stencil.y + 1, bottom_right)?;
        for (channel, value) in values.iter_mut().enumerate() {
            let top = top_left[channel] + (top_right[channel] - top_left[channel]) * stencil.frac_x;
            let bottom = bottom_left[channel]
                + (bottom_right[channel] - bottom_left[channel]) * stencil.frac_x;
            *value = top + (bottom - top) * stencil.frac_y;
        }
        Ok(())
    }

    fn read_value(&mut self, face: usize, x: u32, y: u32, channel: u32) -> Result<f32, String> {
        let (index, offset) = self.cached_pixel_location(face, x, y)?;
        Ok(self.cache[index].values[offset + channel as usize])
    }

    fn copy_components(
        &mut self,
        face: usize,
        x: u32,
        y: u32,
        values: &mut [f32],
    ) -> Result<(), String> {
        let (index, offset) = self.cached_pixel_location(face, x, y)?;
        values.copy_from_slice(&self.cache[index].values[offset..offset + values.len()]);
        Ok(())
    }

    fn cached_pixel_location(
        &mut self,
        face: usize,
        x: u32,
        y: u32,
    ) -> Result<(usize, usize), String> {
        self.clock = self.clock.wrapping_add(1);
        let key = RegionKey {
            face,
            x: x / self.region_side,
            y: y / self.region_side,
        };
        if let Some(index) = self.region_slots.get(&key).copied() {
            self.diagnostics.cache_hits += 1;
            let region = &mut self.cache[index];
            region.last_used = self.clock;
            return Ok((
                index,
                ((y - region.origin_y) * region.width + (x - region.origin_x)) as usize
                    * self.stage.metadata.components as usize,
            ));
        }

        self.load_region(key, x, y)?;
        self.cached_pixel_location(face, x, y)
    }

    fn load_region(&mut self, key: RegionKey, x: u32, y: u32) -> Result<(), String> {
        let resolution = self.stage.metadata.face_resolution;
        let origin_x = (x / self.region_side) * self.region_side;
        let origin_y = (y / self.region_side) * self.region_side;
        let width = self.region_side.min(resolution - origin_x);
        let height = self.region_side.min(resolution - origin_y);
        let values =
            self.stage
                .read_verified_region(key.face as u32, origin_x, origin_y, width, height)?;
        self.diagnostics.region_reads += 1;

        let index = if self.cache.len() == self.diagnostics.cache_capacity {
            let oldest = self
                .cache
                .iter()
                .enumerate()
                .min_by_key(|(_, region)| region.last_used)
                .map(|(index, _)| index)
                .ok_or("staged sampler cache unexpectedly empty")?;
            let evicted = &self.cache[oldest];
            self.region_slots.remove(&RegionKey {
                face: evicted.face,
                x: evicted.origin_x / self.region_side,
                y: evicted.origin_y / self.region_side,
            });
            self.cache[oldest] = CachedRegion {
                face: key.face,
                origin_x,
                origin_y,
                width,
                values,
                last_used: self.clock,
            };
            oldest
        } else {
            self.cache.push(CachedRegion {
                face: key.face,
                origin_x,
                origin_y,
                width,
                values,
                last_used: self.clock,
            });
            self.cache.len() - 1
        };
        self.region_slots.insert(key, index);
        self.diagnostics.cached_regions = self.cache.len();
        self.diagnostics.peak_cached_regions = self
            .diagnostics
            .peak_cached_regions
            .max(self.diagnostics.cached_regions);
        self.diagnostics.cached_values = self.cache.iter().map(|region| region.values.len()).sum();
        self.diagnostics.peak_cached_values = self
            .diagnostics
            .peak_cached_values
            .max(self.diagnostics.cached_values);
        Ok(())
    }
}

/// Produces one typed equirectangular row at a time from a staged cubemap.
pub struct StagedEquirectRowGenerator<'stage> {
    sampler: StagedCubemapSampler<'stage>,
    width: u32,
    height: u32,
    channels: u32,
}

impl<'stage> StagedEquirectRowGenerator<'stage> {
    pub fn new(
        sampler: StagedCubemapSampler<'stage>,
        width: u32,
        height: u32,
        channels: u32,
    ) -> Result<Self, String> {
        if width == 0
            || height == 0
            || channels == 0
            || channels > sampler.stage.metadata.components
        {
            return Err("staged equirect row metadata is invalid".into());
        }
        width
            .checked_mul(channels)
            .ok_or("staged equirect row size overflows u32")?;
        Ok(Self {
            sampler,
            width,
            height,
            channels,
        })
    }

    pub fn generate_row(&mut self, y: u32) -> Result<Vec<f32>, String> {
        let cancel = AtomicBool::new(false);
        self.generate_row_with_cancel(y, &cancel)
    }

    pub fn generate_row_with_cancel(
        &mut self,
        y: u32,
        cancel: &AtomicBool,
    ) -> Result<Vec<f32>, String> {
        if y >= self.height {
            return Err("staged equirect row is outside its bounds".into());
        }
        let values = self
            .width
            .checked_mul(self.channels)
            .ok_or("staged equirect row size overflows u32")?;
        let mut row = vec![0.0; values as usize];
        for x in 0..self.width {
            if x.is_multiple_of(ROW_CANCEL_CHECK_INTERVAL) && cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let start = (x * self.channels) as usize;
            let [dx, dy, dz] = crate::export::equirect_pixel_to_direction(
                x as usize,
                y as usize,
                self.width as usize,
                self.height as usize,
            );
            self.sampler.sample_direction_channels(
                dx,
                dy,
                dz,
                &mut row[start..start + self.channels as usize],
            )?;
        }
        self.sampler.diagnostics.rows_generated += 1;
        self.sampler.diagnostics.row_values += u64::from(values);
        Ok(row)
    }

    pub fn sampler_diagnostics(&self) -> StagedSamplerDiagnostics {
        self.sampler.diagnostics()
    }
}

impl EquirectStage {
    pub fn create(
        output_dir: &Path,
        width: u32,
        height: u32,
        components: u32,
    ) -> Result<Self, String> {
        Self::create_with_file_name(output_dir, width, height, components, "equirect.raw")
    }

    fn create_with_file_name(
        output_dir: &Path,
        width: u32,
        height: u32,
        components: u32,
        file_name: &str,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 || components == 0 {
            return Err(
                "equirect staging metadata requires positive dimensions and components".into(),
            );
        }
        let bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|value| value.checked_mul(u64::from(components)))
            .and_then(|value| value.checked_mul(std::mem::size_of::<f32>() as u64))
            .ok_or("equirect staging size overflows u64")?;
        if bytes > MAX_EQUIRECT_INTERMEDIATE_BYTES {
            return Err("equirect staging exceeds the 4 GiB intermediate limit".into());
        }
        let row_bytes = usize::try_from(u64::from(width) * u64::from(components) * 4)
            .map_err(|_| "equirect staging row size exceeds usize")?;
        std::fs::create_dir_all(output_dir)
            .map_err(|error| format!("failed to create equirect staging parent: {error}"))?;
        let root = loop {
            let candidate = output_dir.join(format!(
                ".planet-gen-equirect-stage-{}-{}",
                std::process::id(),
                STAGE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("failed to create equirect staging: {error}")),
            }
        };
        let path = root.join(file_name);
        let file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) => {
                let _ = std::fs::remove_dir_all(&root);
                return Err(format!("failed to create equirect staging file: {error}"));
            }
        };
        Ok(Self {
            root,
            width,
            height,
            components,
            row_bytes,
            next_row: 0,
            complete: false,
            file,
        })
    }

    pub fn write_row(&mut self, values: &[f32]) -> Result<(), String> {
        if self.complete || self.next_row == self.height {
            return Err("all declared equirect staging rows were already written".into());
        }
        if values.len() * std::mem::size_of::<f32>() != self.row_bytes {
            return Err("equirect staging row has an unexpected component count".into());
        }
        self.file
            .write_all(bytemuck::cast_slice(values))
            .map_err(|error| format!("failed to write equirect staging row: {error}"))?;
        self.next_row += 1;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), String> {
        if self.next_row != self.height {
            return Err("cannot read equirect staging before all rows are written".into());
        }
        self.file
            .flush()
            .map_err(|error| format!("failed to finalize equirect staging: {error}"))?;
        self.complete = true;
        Ok(())
    }

    pub fn read_row(&mut self, y: u32) -> Result<Vec<f32>, String> {
        if !self.complete {
            return Err("cannot read unfinished equirect staging".into());
        }
        if y >= self.height {
            return Err("equirect staging row is outside its bounds".into());
        }
        self.file
            .seek(SeekFrom::Start(u64::from(y) * self.row_bytes as u64))
            .map_err(|error| format!("failed to seek equirect staging row: {error}"))?;
        let mut values = vec![0.0; self.row_bytes / std::mem::size_of::<f32>()];
        self.file
            .read_exact(bytemuck::cast_slice_mut(&mut values))
            .map_err(|error| format!("failed to read equirect staging row: {error}"))?;
        Ok(values)
    }

    pub fn bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * u64::from(self.components) * 4
    }

    #[cfg(test)]
    fn path(&self) -> PathBuf {
        self.root.join("equirect.raw")
    }
}

impl Drop for EquirectStage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Copy)]
struct WrittenRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn coverage_bands(regions: &[WrittenRegion], y: u32, height: u32) -> Vec<u32> {
    let end_y = y + height;
    let mut boundaries = vec![y, end_y];
    for region in regions {
        let region_end_y = region.y + region.height;
        if region_end_y > y && region.y < end_y {
            boundaries.push(region.y.max(y));
            boundaries.push(region_end_y.min(end_y));
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries
}

impl FaceStage {
    pub fn metadata(&self) -> StageMetadata {
        self.metadata
    }

    pub fn create(output_dir: &Path, metadata: StageMetadata) -> Result<Self, String> {
        if metadata.face_resolution == 0 || metadata.components == 0 {
            return Err(
                "staging metadata requires a positive resolution and component count".into(),
            );
        }
        let expected_face_bytes = u64::from(metadata.face_resolution)
            .checked_mul(u64::from(metadata.face_resolution))
            .and_then(|value| value.checked_mul(u64::from(metadata.components)))
            .and_then(|value| value.checked_mul(4))
            .ok_or("staging face size overflows u64")?;
        std::fs::create_dir_all(output_dir)
            .map_err(|error| format!("failed to create export staging parent: {error}"))?;
        let root = loop {
            let candidate = output_dir.join(format!(
                ".planet-gen-stage-{}-{}",
                std::process::id(),
                STAGE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => break candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("failed to create export staging: {error}")),
            }
        };
        let manifest = root.join("metadata.v1");
        let part = root.join("metadata.v1.part");
        std::fs::write(&part, format!("version=1\nresolution={}\ncomponents={}\nhalo={}\nface_bytes={expected_face_bytes}\n", metadata.face_resolution, metadata.components, metadata.halo)).map_err(|error| format!("failed to write staging metadata: {error}"))?;
        std::fs::rename(part, manifest)
            .map_err(|error| format!("failed to publish staging metadata: {error}"))?;
        Ok(Self {
            root,
            metadata,
            expected_face_bytes,
            coverage_lock: Mutex::new(()),
        })
    }

    pub fn write_tile(
        &self,
        face: u32,
        origin_x: u32,
        origin_y: u32,
        width: u32,
        height: u32,
        data: &[f32],
    ) -> Result<(), String> {
        self.validate(face, origin_x, origin_y, width, height, data.len())?;
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(self.face_path(face))
            .map_err(|error| format!("failed to open staged face: {error}"))?;
        let row_bytes =
            width as usize * self.metadata.components as usize * std::mem::size_of::<f32>();
        let face_row_bytes = self.metadata.face_resolution as u64
            * self.metadata.components as u64
            * std::mem::size_of::<f32>() as u64;
        for row in 0..height {
            let offset = ((origin_y + row) as u64 * face_row_bytes)
                + origin_x as u64
                    * self.metadata.components as u64
                    * std::mem::size_of::<f32>() as u64;
            file.seek(SeekFrom::Start(offset))
                .map_err(|error| format!("failed to seek staged face: {error}"))?;
            let start = row as usize * row_bytes / 4;
            let bytes = bytemuck::cast_slice(&data[start..start + row_bytes / 4]);
            file.write_all(bytes)
                .map_err(|error| format!("failed to write staged tile: {error}"))?;
        }
        self.record_region(
            face,
            WrittenRegion {
                x: origin_x,
                y: origin_y,
                width,
                height,
            },
        )?;
        Ok(())
    }

    pub fn read_region(
        &self,
        face: u32,
        origin_x: u32,
        origin_y: u32,
        width: u32,
        height: u32,
    ) -> Result<Vec<f32>, String> {
        let values = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|value| value.checked_mul(u64::from(self.metadata.components)))
            .ok_or("staged region size overflows usize")?;
        let values = usize::try_from(values).map_err(|_| "staged region size exceeds usize")?;
        self.validate(face, origin_x, origin_y, width, height, values)?;
        self.validate_coverage(face, origin_x, origin_y, width, height)?;
        self.read_verified_region(face, origin_x, origin_y, width, height)
    }

    fn read_verified_region(
        &self,
        face: u32,
        origin_x: u32,
        origin_y: u32,
        width: u32,
        height: u32,
    ) -> Result<Vec<f32>, String> {
        let values = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|value| value.checked_mul(u64::from(self.metadata.components)))
            .ok_or("staged region size overflows usize")?;
        let values = usize::try_from(values).map_err(|_| "staged region size exceeds usize")?;
        self.validate(face, origin_x, origin_y, width, height, values)?;
        let mut file = File::open(self.face_path(face))
            .map_err(|error| format!("failed to open staged face: {error}"))?;
        let mut result = vec![0.0; values];
        let row_bytes = width as usize * self.metadata.components as usize * 4;
        let face_row_bytes =
            self.metadata.face_resolution as u64 * self.metadata.components as u64 * 4;
        for row in 0..height {
            let offset = (origin_y + row) as u64 * face_row_bytes
                + origin_x as u64 * self.metadata.components as u64 * 4;
            file.seek(SeekFrom::Start(offset))
                .map_err(|error| format!("failed to seek staged face: {error}"))?;
            let start = row as usize * row_bytes / 4;
            file.read_exact(bytemuck::cast_slice_mut(
                &mut result[start..start + row_bytes / 4],
            ))
            .map_err(|error| format!("failed to read staged region: {error}"))?;
        }
        Ok(result)
    }

    fn face_path(&self, face: u32) -> PathBuf {
        self.root.join(format!("face-{face}.raw"))
    }

    fn coverage_path(&self, face: u32) -> PathBuf {
        self.root.join(format!("face-{face}.regions"))
    }

    fn record_region(&self, face: u32, region: WrittenRegion) -> Result<(), String> {
        let _lock = self
            .coverage_lock
            .lock()
            .map_err(|_| "staged coverage lock poisoned")?;
        let mut regions = self.load_regions(face)?;
        regions.push(region);
        let part = self.coverage_path(face).with_extension("regions.part");
        let body = regions
            .iter()
            .map(|r| format!("{} {} {} {}\n", r.x, r.y, r.width, r.height))
            .collect::<String>();
        std::fs::write(&part, body)
            .map_err(|error| format!("failed to write staged coverage: {error}"))?;
        std::fs::rename(part, self.coverage_path(face))
            .map_err(|error| format!("failed to publish staged coverage: {error}"))
    }

    fn load_regions(&self, face: u32) -> Result<Vec<WrittenRegion>, String> {
        let path = self.coverage_path(face);
        if !path.exists() {
            return Ok(Vec::new());
        }
        std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read staged coverage: {error}"))?
            .lines()
            .map(|line| {
                let values = line
                    .split_whitespace()
                    .map(str::parse::<u32>)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| "invalid staged coverage metadata")?;
                if values.len() != 4 {
                    return Err("invalid staged coverage metadata".into());
                }
                Ok(WrittenRegion {
                    x: values[0],
                    y: values[1],
                    width: values[2],
                    height: values[3],
                })
            })
            .collect()
    }

    fn validate_coverage(
        &self,
        face: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let regions = self.load_regions(face)?;
        let end_x = x + width;
        let boundaries = coverage_bands(&regions, y, height);
        for window in boundaries.windows(2) {
            let (band_start, band_end) = (window[0], window[1]);
            let mut intervals = regions
                .iter()
                .filter(|region| region.y <= band_start && region.y + region.height >= band_end)
                .filter_map(|region| {
                    let start = region.x.max(x);
                    let end = (region.x + region.width).min(end_x);
                    (start < end).then_some((start, end))
                })
                .collect::<Vec<_>>();
            intervals.sort_unstable_by_key(|&(start, _)| start);
            let mut covered_until = x;
            for (start, end) in intervals {
                if start > covered_until {
                    return Err("staged region contains an uncovered hole".into());
                }
                covered_until = covered_until.max(end);
                if covered_until == end_x {
                    break;
                }
            }
            if covered_until < end_x {
                return Err("staged region contains an uncovered hole".into());
            }
        }
        Ok(())
    }

    pub fn validate_face_coverage(&self, face: u32) -> Result<(), String> {
        if face >= 6 {
            return Err("staged face index is invalid".into());
        }
        let length = std::fs::metadata(self.face_path(face))
            .map_err(|error| format!("failed to inspect staged face: {error}"))?
            .len();
        if length != self.expected_face_bytes {
            return Err("staged face coverage is incomplete or truncated".into());
        }
        self.validate_coverage(
            face,
            0,
            0,
            self.metadata.face_resolution,
            self.metadata.face_resolution,
        )?;
        Ok(())
    }

    fn validate(
        &self,
        face: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        len: usize,
    ) -> Result<(), String> {
        if face >= 6
            || width == 0
            || height == 0
            || x.checked_add(width)
                .is_none_or(|end| end > self.metadata.face_resolution)
            || y.checked_add(height)
                .is_none_or(|end| end > self.metadata.face_resolution)
        {
            return Err("staged tile is outside its face bounds".into());
        }
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|value| value.checked_mul(u64::from(self.metadata.components)))
            .ok_or("staged component count overflows usize")?;
        if usize::try_from(expected).map_err(|_| "staged component count exceeds usize")? != len {
            return Err("staged tile has an invalid component count".into());
        }
        Ok(())
    }
}

impl Drop for FaceStage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equirect_intermediate_is_bounded_and_cleans_up() {
        let root = std::env::temp_dir();
        assert!(EquirectStage::create(&root, 65_536, 16_385, 1).is_err());

        let mut stage = EquirectStage::create(&root, 2, 2, 1).unwrap();
        let staging_root = stage.root.clone();
        assert_eq!(stage.bytes(), 16);
        stage.write_row(&[0.0, 1.0]).unwrap();
        stage.write_row(&[0.25, 0.75]).unwrap();
        stage.finish().unwrap();
        assert!(stage.path().is_file());
        assert_eq!(stage.read_row(1).unwrap(), vec![0.25, 0.75]);
        assert_eq!(stage.read_row(0).unwrap(), vec![0.0, 1.0]);
        drop(stage);
        assert!(!staging_root.exists());
    }

    #[test]
    fn equirect_intermediate_creation_failure_cleans_up_root() {
        let parent = std::env::temp_dir().join(format!(
            "planet-gen-equirect-create-failure-{}-{}",
            std::process::id(),
            STAGE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&parent);
        assert!(EquirectStage::create_with_file_name(&parent, 1, 1, 1, ".").is_err());
        assert!(std::fs::read_dir(&parent).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(parent);
    }

    #[test]
    fn equirect_intermediate_is_deterministic() {
        let root = std::env::temp_dir();
        let write = |stage: &mut EquirectStage| {
            stage.write_row(&[0.0, 1.0]).unwrap();
            stage.write_row(&[0.25, 0.75]).unwrap();
            stage.finish().unwrap();
            [stage.read_row(0).unwrap(), stage.read_row(1).unwrap()]
        };
        let first = write(&mut EquirectStage::create(&root, 2, 2, 1).unwrap());
        let second = write(&mut EquirectStage::create(&root, 2, 2, 1).unwrap());
        assert_eq!(first, second);
    }

    #[test]
    fn reads_staged_tile_regions_without_loading_the_face() {
        let root =
            std::env::temp_dir().join(format!("planet-gen-stage-test-{}", std::process::id()));
        let stage = FaceStage::create(
            &root,
            StageMetadata {
                face_resolution: 4,
                components: 1,
                halo: 6,
            },
        )
        .unwrap();
        stage
            .write_tile(0, 0, 0, 2, 2, &[1.0, 2.0, 3.0, 4.0])
            .unwrap();
        stage
            .write_tile(0, 2, 0, 2, 2, &[5.0, 6.0, 7.0, 8.0])
            .unwrap();
        assert_eq!(
            stage.read_region(0, 1, 0, 2, 2).unwrap(),
            vec![2.0, 5.0, 4.0, 7.0]
        );
        let staged_root = stage.root.clone();
        drop(stage);
        assert!(!staged_root.exists());
    }

    #[test]
    fn rejects_invalid_tile_metadata() {
        let root = std::env::temp_dir();
        assert!(FaceStage::create(
            &root,
            StageMetadata {
                face_resolution: 0,
                components: 1,
                halo: 0
            }
        )
        .is_err());
    }

    #[test]
    fn stages_are_isolated_and_reject_truncated_faces() {
        let root =
            std::env::temp_dir().join(format!("planet-gen-stage-isolation-{}", std::process::id()));
        let metadata = StageMetadata {
            face_resolution: 2,
            components: 1,
            halo: 0,
        };
        let first = FaceStage::create(&root, metadata).unwrap();
        let second = FaceStage::create(&root, metadata).unwrap();
        assert_ne!(first.root, second.root);
        first.write_tile(0, 0, 0, 2, 1, &[1.0, 2.0]).unwrap();
        assert!(first.validate_face_coverage(0).is_err());
        first.write_tile(0, 0, 1, 2, 1, &[3.0, 4.0]).unwrap();
        first.validate_face_coverage(0).unwrap();
    }

    #[test]
    fn rejects_reads_that_cross_a_sparse_coverage_hole() {
        let root = std::env::temp_dir();
        let stage = FaceStage::create(
            &root,
            StageMetadata {
                face_resolution: 3,
                components: 1,
                halo: 0,
            },
        )
        .unwrap();
        stage.write_tile(0, 0, 0, 1, 1, &[1.0]).unwrap();
        stage.write_tile(0, 2, 0, 1, 1, &[2.0]).unwrap();
        assert!(stage.read_region(0, 0, 0, 3, 1).is_err());
    }

    #[test]
    fn sampler_rejects_sparse_coverage_during_preflight() {
        let stage = FaceStage::create(
            &std::env::temp_dir(),
            StageMetadata {
                face_resolution: 3,
                components: 1,
                halo: 0,
            },
        )
        .unwrap();
        for face in 1..6 {
            stage.write_tile(face, 0, 0, 3, 3, &[0.0; 9]).unwrap();
        }
        stage.write_tile(0, 0, 0, 2, 3, &[0.0; 6]).unwrap();
        assert!(StagedCubemapSampler::new(&stage, 2, 1).is_err());
    }

    #[test]
    fn sampler_preflight_does_not_mutate_complete_stage_files() {
        let (stage, _) = staged_faces(8, 1);
        let before = (0..6)
            .map(|face| std::fs::read(stage.face_path(face)).unwrap())
            .collect::<Vec<_>>();
        let _sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        let after = (0..6)
            .map(|face| std::fs::read(stage.face_path(face)).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(before, after);
    }

    #[test]
    fn full_length_face_with_holes_is_not_complete() {
        let stage = FaceStage::create(
            &std::env::temp_dir(),
            StageMetadata {
                face_resolution: 2,
                components: 1,
                halo: 0,
            },
        )
        .unwrap();
        stage.write_tile(0, 1, 1, 1, 1, &[1.0]).unwrap();
        assert!(stage.validate_face_coverage(0).is_err());
    }

    #[test]
    fn coverage_validation_uses_tile_bands_not_per_pixel_scanning() {
        let tile_side = 64;
        let face_resolution = 1024;
        let regions = (0..face_resolution / tile_side)
            .flat_map(|tile_y| {
                (0..face_resolution / tile_side).map(move |tile_x| WrittenRegion {
                    x: tile_x * tile_side,
                    y: tile_y * tile_side,
                    width: tile_side,
                    height: tile_side,
                })
            })
            .collect::<Vec<_>>();
        let bands = coverage_bands(&regions, 0, face_resolution);
        assert_eq!(
            bands.len() - 1,
            face_resolution as usize / tile_side as usize
        );
        assert!(bands.len() - 1 < face_resolution as usize);
    }

    fn staged_faces(resolution: u32, components: u32) -> (FaceStage, [Vec<f32>; 6]) {
        let root = std::env::temp_dir().join(format!(
            "planet-gen-staged-sampler-{}-{}",
            std::process::id(),
            STAGE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let stage = FaceStage::create(
            &root,
            StageMetadata {
                face_resolution: resolution,
                components,
                halo: 0,
            },
        )
        .unwrap();
        let faces = std::array::from_fn(|face| {
            (0..resolution * resolution * components)
                .map(|index| face as f32 * 10_000.0 + index as f32 * 0.25)
                .collect::<Vec<_>>()
        });
        for (face, values) in faces.iter().enumerate() {
            stage
                .write_tile(face as u32, 0, 0, resolution, resolution, values)
                .unwrap();
        }
        (stage, faces)
    }

    struct LegacyStagedSampler<'stage> {
        stage: &'stage FaceStage,
        region_side: u32,
        cache_capacity: usize,
        cache: Vec<CachedRegion>,
        clock: u64,
    }

    impl<'stage> LegacyStagedSampler<'stage> {
        fn new(stage: &'stage FaceStage, region_side: u32, cache_capacity: usize) -> Self {
            Self {
                stage,
                region_side,
                cache_capacity,
                cache: Vec::with_capacity(cache_capacity),
                clock: 0,
            }
        }

        fn sample_equirect_pixel(&mut self, x: u32, y: u32, channel: u32) -> f32 {
            let resolution = self.stage.metadata.face_resolution;
            let [dx, dy, dz] = crate::export::equirect_pixel_to_direction(
                x as usize,
                y as usize,
                (resolution * 2) as usize,
                resolution as usize,
            );
            let (face, u, v) = crate::export::direction_to_face_uv(dx, dy, dz);
            let fx = u * (resolution - 1) as f32;
            let fy = v * (resolution - 1) as f32;
            let ix = (fx as u32).min(resolution - 2);
            let iy = (fy as u32).min(resolution - 2);
            let top_left = self.read_value(face, ix, iy, channel);
            let top_right = self.read_value(face, ix + 1, iy, channel);
            let bottom_left = self.read_value(face, ix, iy + 1, channel);
            let bottom_right = self.read_value(face, ix + 1, iy + 1, channel);
            let top = top_left + (top_right - top_left) * (fx - ix as f32);
            let bottom = bottom_left + (bottom_right - bottom_left) * (fx - ix as f32);
            top + (bottom - top) * (fy - iy as f32)
        }

        fn read_value(&mut self, face: usize, x: u32, y: u32, channel: u32) -> f32 {
            self.clock = self.clock.wrapping_add(1);
            let components = self.stage.metadata.components as usize;
            if let Some(index) = self.cache.iter().position(|region| {
                region.face == face
                    && x >= region.origin_x
                    && y >= region.origin_y
                    && x < region.origin_x + region.width
                    && y - region.origin_y
                        < (region.values.len() / components / region.width as usize) as u32
            }) {
                let region = &mut self.cache[index];
                region.last_used = self.clock;
                return region.values[((y - region.origin_y) * region.width + (x - region.origin_x))
                    as usize
                    * components
                    + channel as usize];
            }

            let origin_x = (x / self.region_side) * self.region_side;
            let origin_y = (y / self.region_side) * self.region_side;
            let resolution = self.stage.metadata.face_resolution;
            let width = self.region_side.min(resolution - origin_x);
            let height = self.region_side.min(resolution - origin_y);
            let values = self
                .stage
                .read_verified_region(face as u32, origin_x, origin_y, width, height)
                .unwrap();
            if self.cache.len() == self.cache_capacity {
                let oldest = self
                    .cache
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, region)| region.last_used)
                    .map(|(index, _)| index)
                    .unwrap();
                self.cache.swap_remove(oldest);
            }
            self.cache.push(CachedRegion {
                face,
                origin_x,
                origin_y,
                width,
                values,
                last_used: self.clock,
            });
            self.read_value(face, x, y, channel)
        }
    }

    fn in_memory_sample(
        faces: &[Vec<f32>; 6],
        resolution: u32,
        components: u32,
        dx: f32,
        dy: f32,
        dz: f32,
        channel: u32,
    ) -> f32 {
        let (face, u, v) = crate::export::direction_to_face_uv(dx, dy, dz);
        let res = resolution as usize;
        let fx = u * (res - 1) as f32;
        let fy = v * (res - 1) as f32;
        let ix = (fx as usize).min(res - 2);
        let iy = (fy as usize).min(res - 2);
        let frac_x = fx - ix as f32;
        let frac_y = fy - iy as f32;
        let value = |x: usize, y: usize| {
            faces[face][(y * res + x) * components as usize + channel as usize]
        };
        let top = value(ix, iy) + (value(ix + 1, iy) - value(ix, iy)) * frac_x;
        let bottom = value(ix, iy + 1) + (value(ix + 1, iy + 1) - value(ix, iy + 1)) * frac_x;
        top + (bottom - top) * frac_y
    }

    #[test]
    fn staged_sampler_matches_in_memory_at_regular_points_seams_and_poles() {
        let resolution = 8;
        let components = 3;
        let (stage, faces) = staged_faces(resolution, components);
        let mut sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        let sample_equirect =
            |sampler: &mut StagedCubemapSampler<'_>, x: u32, y: u32, channel: u32| {
                let [dx, dy, dz] = crate::export::equirect_pixel_to_direction(
                    x as usize,
                    y as usize,
                    (resolution * 2) as usize,
                    resolution as usize,
                );
                let expected =
                    in_memory_sample(&faces, resolution, components, dx, dy, dz, channel);
                assert_eq!(
                    sampler.sample_equirect_pixel(x, y, channel).unwrap(),
                    expected
                );
            };

        for (x, y) in [(1, 1), (4, 2), (7, 3), (10, 5), (15, 6)] {
            sample_equirect(&mut sampler, x, y, 1);
        }
        for (dx, dy, dz) in [
            (-1.0, -1.0, 0.0),
            (-1.0, 1.0, 0.0),
            (1.0, -1.0, 0.0),
            (1.0, 1.0, 0.0),
            (-1.0, 0.0, -1.0),
            (-1.0, 0.0, 1.0),
            (1.0, 0.0, -1.0),
            (1.0, 0.0, 1.0),
            (0.0, -1.0, -1.0),
            (0.0, -1.0, 1.0),
            (0.0, 1.0, -1.0),
            (0.0, 1.0, 1.0),
        ] {
            assert_eq!(
                sampler.sample_direction(dx, dy, dz, 2).unwrap(),
                in_memory_sample(&faces, resolution, components, dx, dy, dz, 2)
            );
        }
        for x in 0..resolution * 2 {
            sample_equirect(&mut sampler, x, 0, 0);
            sample_equirect(&mut sampler, x, resolution - 1, 0);
        }
    }

    #[test]
    fn staged_sampler_rejects_zero_and_non_finite_directions() {
        let (stage, _) = staged_faces(8, 1);
        let mut sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        for direction in [
            [0.0, 0.0, 0.0],
            [f32::NAN, 0.0, 1.0],
            [0.0, f32::INFINITY, 1.0],
            [0.0, 1.0, f32::NEG_INFINITY],
        ] {
            assert_eq!(
                sampler
                    .sample_direction(direction[0], direction[1], direction[2], 0)
                    .unwrap_err(),
                "staged cubemap direction must be finite and nonzero"
            );
        }
    }

    #[test]
    fn region_slot_index_evicts_the_replaced_cache_slot() {
        let (stage, faces) = staged_faces(8, 1);
        let mut sampler = StagedCubemapSampler::new(&stage, 2, 1).unwrap();

        assert_eq!(sampler.read_value(0, 0, 0, 0).unwrap(), faces[0][0]);
        assert_eq!(sampler.read_value(0, 3, 0, 0).unwrap(), faces[0][3]);
        assert_eq!(sampler.region_slots.len(), 1);
        assert_eq!(sampler.read_value(0, 0, 0, 0).unwrap(), faces[0][0]);

        let diagnostics = sampler.diagnostics();
        assert_eq!(diagnostics.region_reads, 3);
        assert_eq!(diagnostics.cached_regions, 1);
        assert_eq!(sampler.region_slots.len(), diagnostics.cached_regions);
    }

    #[test]
    fn multi_channel_sampling_matches_components_across_faces_and_boundaries() {
        let components = 5;
        let (stage, _) = staged_faces(8, components);
        let mut per_component = StagedCubemapSampler::new(&stage, 2, 2).unwrap();
        let mut multi_channel = StagedCubemapSampler::new(&stage, 2, 2).unwrap();
        let directions = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [-1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, -1.0, -1.0],
            [0.0, -1.0, 1.0],
            [0.0, 1.0, -1.0],
            [0.0, 1.0, 1.0],
        ];

        for [dx, dy, dz] in directions {
            let expected = (0..components)
                .map(|channel| per_component.sample_direction(dx, dy, dz, channel).unwrap())
                .collect::<Vec<_>>();
            let mut actual = vec![0.0; components as usize];
            multi_channel
                .sample_direction_channels(dx, dy, dz, &mut actual)
                .unwrap();
            assert_eq!(
                actual
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|value| value.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn multi_channel_equirect_sampling_is_bitwise_per_component_parity() {
        let resolution = 8;
        let components = 4;
        let (stage, _) = staged_faces(resolution, components);
        let mut per_component = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        let mut multi_channel = StagedCubemapSampler::new(&stage, 3, 2).unwrap();

        for y in 0..resolution {
            for x in 0..resolution * 2 {
                let expected = (0..components)
                    .map(|channel| per_component.sample_equirect_pixel(x, y, channel).unwrap())
                    .collect::<Vec<_>>();
                let mut actual = vec![0.0; components as usize];
                multi_channel
                    .sample_equirect_pixel_channels(x, y, &mut actual)
                    .unwrap();
                assert_eq!(
                    actual
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    expected
                        .iter()
                        .map(|value| value.to_bits())
                        .collect::<Vec<_>>(),
                    "sample ({x}, {y})"
                );
            }
        }
    }

    #[test]
    #[ignore = "manual PERF-013 sampler smoke benchmark"]
    fn benchmark_multi_channel_row_sampling() {
        let resolution = 512;
        let components = 4;
        let (stage, _) = staged_faces(resolution, components);

        let mut per_component = LegacyStagedSampler::new(&stage, 64, 8);
        let per_component_started = std::time::Instant::now();
        for y in 0..resolution {
            for x in 0..resolution * 2 {
                for channel in 0..components {
                    std::hint::black_box(per_component.sample_equirect_pixel(x, y, channel));
                }
            }
        }
        let per_component_elapsed = per_component_started.elapsed();

        let sampler = StagedCubemapSampler::new(&stage, 64, 8).unwrap();
        let mut rows =
            StagedEquirectRowGenerator::new(sampler, resolution * 2, resolution, components)
                .unwrap();
        let multi_channel_started = std::time::Instant::now();
        for y in 0..resolution {
            std::hint::black_box(rows.generate_row(y).unwrap());
        }
        let multi_channel_elapsed = multi_channel_started.elapsed();

        eprintln!(
            "PERF-013 sampler smoke: per_component={per_component_elapsed:?}, \
             multi_channel={multi_channel_elapsed:?}, speedup={:.2}x",
            per_component_elapsed.as_secs_f64() / multi_channel_elapsed.as_secs_f64()
        );
    }

    #[test]
    fn staged_sampler_cache_is_bounded_without_a_full_face() {
        let resolution = 8;
        let components = 3;
        let (stage, _) = staged_faces(resolution, components);
        let mut sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        for y in 0..resolution {
            for x in 0..resolution * 2 {
                sampler.sample_equirect_pixel(x, y, 0).unwrap();
            }
        }
        let diagnostics = sampler.diagnostics();
        assert_eq!(diagnostics.cache_capacity, 2);
        assert_eq!(
            diagnostics.max_cached_values,
            2 * 3 * 3 * components as usize
        );
        assert!(diagnostics.cached_regions <= diagnostics.cache_capacity);
        assert!(diagnostics.peak_cached_regions <= diagnostics.cache_capacity);
        assert!(diagnostics.peak_cached_values <= diagnostics.max_cached_values);
        assert!(diagnostics.peak_cached_values < (resolution * resolution * components) as usize);
        assert!(diagnostics.region_reads > diagnostics.cache_capacity as u64);
        assert!(diagnostics.cache_hits > 0);
    }

    #[test]
    fn row_generation_observes_cancellation_before_a_full_row() {
        let (stage, _) = staged_faces(8, 4);
        let sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        let mut rows = StagedEquirectRowGenerator::new(sampler, 256, 8, 4).unwrap();
        let cancel = AtomicBool::new(true);
        assert_eq!(
            rows.generate_row_with_cancel(0, &cancel),
            Err("Cancelled".into())
        );
        assert_eq!(rows.sampler_diagnostics().rows_generated, 0);
    }

    #[test]
    fn staged_equirect_rows_match_monolithic_rgba_without_full_image_allocation() {
        let resolution = 8;
        let channels = 4;
        let (stage, faces) = staged_faces(resolution, channels);
        let (monolithic, width, height) =
            crate::export::cubemap_to_equirect(&faces, resolution, channels as usize);
        let sampler = StagedCubemapSampler::new(&stage, 3, 2).unwrap();
        let mut rows = StagedEquirectRowGenerator::new(sampler, width, height, channels).unwrap();
        let row_values = width as usize * channels as usize;

        for y in 0..height {
            let row = rows.generate_row(y).unwrap();
            let start = y as usize * row_values;
            assert_eq!(row, monolithic[start..start + row_values]);
            assert_eq!(row.len(), row_values);
            assert!(row.len() < monolithic.len());
        }

        let diagnostics = rows.sampler_diagnostics();
        assert!(diagnostics.peak_cached_regions <= diagnostics.cache_capacity);
        assert!(diagnostics.peak_cached_values <= diagnostics.max_cached_values);
        assert!(
            diagnostics.peak_cached_values
                < resolution as usize * resolution as usize * channels as usize
        );
    }
}
