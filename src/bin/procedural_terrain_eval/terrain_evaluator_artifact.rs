use std::{fmt, fs, io::Read, path::Path};

use planet_gen::terrain_compute::TectonicTerrain;

pub const CANONICAL_FACE_NAMES: [&str; 6] = [
    "posx.f32le",
    "negx.f32le",
    "posy.f32le",
    "negy.f32le",
    "posz.f32le",
    "negz.f32le",
];

#[derive(Debug)]
pub enum CubemapError {
    Io,
    Size,
    Resolution,
    NonFinite,
}

impl fmt::Display for CubemapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Io => "cannot read cubemap artifact",
            Self::Size => "invalid cubemap artifact size",
            Self::Resolution => "unsupported cubemap artifact resolution",
            Self::NonFinite => "cubemap artifact contains non-finite values",
        })
    }
}

impl std::error::Error for CubemapError {}

impl From<std::io::Error> for CubemapError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv_update(14695981039346656037, bytes)
}

fn fnv_update(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |value, byte| {
        (value ^ u64::from(*byte)).wrapping_mul(1099511628211)
    })
}

pub fn infer_canonical_resolution(dir: &Path, max_resolution: u32) -> Result<u32, CubemapError> {
    if dir.join(".incomplete").exists() {
        return Err(CubemapError::Size);
    }
    let mut byte_len = None;
    for name in CANONICAL_FACE_NAMES {
        let metadata = fs::metadata(dir.join(name))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() % 4 != 0 {
            return Err(CubemapError::Size);
        }
        if byte_len
            .replace(metadata.len())
            .is_some_and(|previous| previous != metadata.len())
        {
            return Err(CubemapError::Size);
        }
    }
    let samples = byte_len.ok_or(CubemapError::Size)? / 4;
    let resolution = (samples as f64).sqrt() as u64;
    if resolution.checked_mul(resolution) != Some(samples) {
        return Err(CubemapError::Size);
    }
    let resolution = u32::try_from(resolution).map_err(|_| CubemapError::Resolution)?;
    if resolution > max_resolution {
        return Err(CubemapError::Resolution);
    }
    Ok(resolution)
}

pub fn load_canonical_terrain(
    dir: &Path,
    resolution: u32,
    max_resolution: u32,
) -> Result<TectonicTerrain, CubemapError> {
    if resolution > max_resolution || infer_canonical_resolution(dir, max_resolution)? != resolution
    {
        return Err(CubemapError::Resolution);
    }
    let face_bytes = usize::try_from(resolution)
        .map_err(|_| CubemapError::Resolution)?
        .checked_mul(resolution as usize)
        .and_then(|samples| samples.checked_mul(4))
        .ok_or(CubemapError::Size)?;
    let mut faces = std::array::from_fn(|_| Vec::with_capacity(face_bytes / 4));
    for (index, name) in CANONICAL_FACE_NAMES.iter().enumerate() {
        let path = dir.join(name);
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() != face_bytes as u64 {
            return Err(CubemapError::Size);
        }
        let mut bytes = vec![0; face_bytes];
        fs::File::open(path)?.read_exact(&mut bytes)?;
        for value in bytes.chunks_exact(4) {
            let value = f32::from_le_bytes(value.try_into().map_err(|_| CubemapError::Size)?);
            if !value.is_finite() {
                return Err(CubemapError::NonFinite);
            }
            faces[index].push(value);
        }
    }
    Ok(TectonicTerrain { faces, resolution })
}
