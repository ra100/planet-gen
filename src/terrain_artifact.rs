use std::{
    env,
    ffi::{OsStr, OsString},
    fmt, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::terrain_compute::TectonicTerrain;

pub const CANONICAL_FACE_NAMES: [&str; 6] = [
    "posx.f32le",
    "negx.f32le",
    "posy.f32le",
    "negy.f32le",
    "posz.f32le",
    "negz.f32le",
];
pub const KNOWN_NO_GO_CONTROL_FNV: u64 = 0x243e1887675e77a8;
pub const KNOWN_NO_GO_VALIDATOR_COMMIT: &str = "0cba9579e68f0fc72a75d21db2d655496ef76d09";
const MAX_APPROVAL_BYTES: u64 = 16 * 1024;
const MAX_EVIDENCE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainResolution {
    R512,
    R1024,
    R2048,
    R4096,
    R8192,
}

impl TerrainResolution {
    fn parse(value: &str) -> Result<Self, TerrainArtifactError> {
        match value {
            "512" => Ok(Self::R512),
            "1024" => Ok(Self::R1024),
            "2048" => Ok(Self::R2048),
            "4096" => Ok(Self::R4096),
            "8192" => Ok(Self::R8192),
            _ => Err(TerrainArtifactError::Resolution),
        }
    }

    pub fn value(self) -> u32 {
        match self {
            Self::R512 => 512,
            Self::R1024 => 1024,
            Self::R2048 => 2048,
            Self::R4096 => 4096,
            Self::R8192 => 8192,
        }
    }
}

#[derive(Debug)]
pub enum TerrainArtifactError {
    Usage,
    Path,
    Schema,
    Approval,
    Size,
    Resolution,
    NonFinite,
    Identity,
    Evidence,
    Io(std::io::Error),
}

impl fmt::Display for TerrainArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Usage => "invalid terrain import arguments",
            Self::Path => "invalid terrain import path",
            Self::Schema => "invalid terrain approval schema",
            Self::Approval => "terrain artifact is not approved",
            Self::Size => "invalid terrain artifact size",
            Self::Resolution => "unsupported terrain artifact resolution",
            Self::NonFinite => "terrain artifact contains non-finite values",
            Self::Identity => "terrain artifact identity does not match approval",
            Self::Evidence => "terrain approval evidence does not match",
            Self::Io(_) => "cannot read terrain import input",
        })
    }
}

impl std::error::Error for TerrainArtifactError {}

impl From<std::io::Error> for TerrainArtifactError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug)]
pub struct TerrainArtifactApproval {
    pub artifact_path: String,
    pub resolution: TerrainResolution,
    pub face_fnv1a64: [u64; 6],
    pub candidate_fnv1a64: u64,
    pub control_fnv1a64: u64,
    pub validator_commit: String,
    pub evidence_path: String,
    pub evidence_fnv1a64: u64,
    pub control_ocean_level: f32,
}

pub type ApprovalRecord = TerrainArtifactApproval;

pub struct ApprovedTerrainArtifact {
    pub terrain: Arc<TectonicTerrain>,
    pub approval: TerrainArtifactApproval,
}

#[derive(Clone)]
pub enum TerrainSource {
    Procedural,
    Imported {
        terrain: Arc<TectonicTerrain>,
        control_ocean_level: f32,
    },
}

pub fn export_refusal(source: &TerrainSource) -> Option<&'static str> {
    matches!(source, TerrainSource::Imported { .. })
        .then_some("Export is unavailable for imported terrain artifacts.")
}

pub enum CliMode {
    Help,
    Procedural,
    Import {
        artifact: PathBuf,
        approval: PathBuf,
    },
}

pub fn parse_cli_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<CliMode, TerrainArtifactError> {
    let args: Vec<_> = args.into_iter().collect();
    if args.is_empty() {
        return Ok(CliMode::Procedural);
    }
    if args.len() == 1 && args[0] == "--help" {
        return Ok(CliMode::Help);
    }
    let mut artifact = None;
    let mut approval = None;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].to_str().ok_or(TerrainArtifactError::Usage)?;
        if index + 1 == args.len() {
            return Err(TerrainArtifactError::Usage);
        }
        if args[index + 1].is_empty() || starts_with_flag(&args[index + 1]) {
            return Err(TerrainArtifactError::Usage);
        }
        let value = PathBuf::from(&args[index + 1]);
        match flag {
            "--terrain-artifact" if artifact.replace(value.clone()).is_none() => {}
            "--terrain-approval" if approval.replace(value).is_none() => {}
            _ => return Err(TerrainArtifactError::Usage),
        }
        index += 2;
    }
    match (artifact, approval) {
        (Some(artifact), Some(approval)) => Ok(CliMode::Import { artifact, approval }),
        _ => Err(TerrainArtifactError::Usage),
    }
}

#[cfg(unix)]
fn starts_with_flag(value: &OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    value.as_bytes().starts_with(b"--")
}

#[cfg(not(unix))]
fn starts_with_flag(value: &OsStr) -> bool {
    value.to_str().is_some_and(|value| value.starts_with("--"))
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    fnv_update(14695981039346656037, bytes)
}

fn fnv_update(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |value, byte| {
        (value ^ u64::from(*byte)).wrapping_mul(1099511628211)
    })
}

pub fn infer_resolution(dir: &Path) -> Result<u32, TerrainArtifactError> {
    let resolution = infer_canonical_resolution(dir, TerrainResolution::R8192.value())?;
    TerrainResolution::parse(&resolution.to_string())?;
    Ok(resolution)
}

pub fn infer_canonical_resolution(
    dir: &Path,
    max_resolution: u32,
) -> Result<u32, TerrainArtifactError> {
    if dir.join(".incomplete").exists() {
        return Err(TerrainArtifactError::Approval);
    }
    let mut byte_len = None;
    for name in CANONICAL_FACE_NAMES {
        let metadata = fs::metadata(dir.join(name))?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() % 4 != 0 {
            return Err(TerrainArtifactError::Size);
        }
        if byte_len
            .replace(metadata.len())
            .is_some_and(|previous| previous != metadata.len())
        {
            return Err(TerrainArtifactError::Size);
        }
    }
    let samples = byte_len.ok_or(TerrainArtifactError::Size)? / 4;
    let resolution = (samples as f64).sqrt() as u64;
    if resolution.checked_mul(resolution) != Some(samples) {
        return Err(TerrainArtifactError::Size);
    }
    let resolution = u32::try_from(resolution).map_err(|_| TerrainArtifactError::Resolution)?;
    if resolution > max_resolution {
        return Err(TerrainArtifactError::Resolution);
    }
    Ok(resolution)
}

fn normalized_relative(path: &Path) -> Result<(PathBuf, String), TerrainArtifactError> {
    if path.is_absolute() {
        return Err(TerrainArtifactError::Path);
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {}
            _ => return Err(TerrainArtifactError::Path),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(TerrainArtifactError::Path);
    }
    let display = normalized
        .to_str()
        .ok_or(TerrainArtifactError::Path)?
        .replace('\\', "/");
    Ok((normalized, display))
}

fn cwd() -> Result<PathBuf, TerrainArtifactError> {
    env::current_dir()?.canonicalize().map_err(Into::into)
}

fn resolve_under_cwd(
    path: &Path,
    directory: bool,
) -> Result<(PathBuf, String), TerrainArtifactError> {
    let (relative, display) = normalized_relative(path)?;
    let cwd = cwd()?;
    let mut current = cwd.clone();
    for component in relative.components() {
        current.push(component);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(TerrainArtifactError::Path);
        }
    }
    let resolved = current.canonicalize()?;
    if !resolved.starts_with(&cwd)
        || if directory {
            !resolved.is_dir()
        } else {
            !resolved.is_file()
        }
    {
        return Err(TerrainArtifactError::Path);
    }
    Ok((resolved, display))
}

fn exact_artifact_entries(dir: &Path) -> Result<(), TerrainArtifactError> {
    let mut names = fs::read_dir(dir)?
        .map(|entry| entry.map_err(TerrainArtifactError::from))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort_by_key(|entry| entry.file_name());
    if names.len() != CANONICAL_FACE_NAMES.len() {
        return Err(TerrainArtifactError::Schema);
    }
    for name in CANONICAL_FACE_NAMES {
        let entry = names
            .iter()
            .find(|entry| entry.file_name() == name)
            .ok_or(TerrainArtifactError::Schema)?;
        if entry.file_type()?.is_symlink() || !entry.file_type()?.is_file() {
            return Err(TerrainArtifactError::Path);
        }
    }
    Ok(())
}

fn expected_face_bytes(resolution: u32) -> Result<usize, TerrainArtifactError> {
    let n = usize::try_from(resolution).map_err(|_| TerrainArtifactError::Resolution)?;
    n.checked_mul(n)
        .and_then(|samples| samples.checked_mul(4))
        .ok_or(TerrainArtifactError::Size)
}

fn load_faces(
    dir: &Path,
    resolution: u32,
) -> Result<(TectonicTerrain, [u64; 6], u64), TerrainArtifactError> {
    let face_bytes = expected_face_bytes(resolution)?;
    let mut aggregate = 14695981039346656037;
    let mut hashes = [0; 6];
    let mut faces = std::array::from_fn(|_| Vec::with_capacity(face_bytes / 4));
    for (index, name) in CANONICAL_FACE_NAMES.iter().enumerate() {
        let path = dir.join(name);
        let before = fs::metadata(&path)?;
        if !before.is_file() || before.len() != face_bytes as u64 {
            return Err(TerrainArtifactError::Size);
        }
        let mut bytes = vec![0; face_bytes];
        fs::File::open(&path)?.read_exact(&mut bytes)?;
        let after = fs::metadata(&path)?;
        if after.len() != before.len() || after.modified()? != before.modified()? {
            return Err(TerrainArtifactError::Size);
        }
        let mut hash = 14695981039346656037;
        for value in bytes.chunks_exact(4) {
            let value =
                f32::from_le_bytes(value.try_into().map_err(|_| TerrainArtifactError::Size)?);
            if !value.is_finite() {
                return Err(TerrainArtifactError::NonFinite);
            }
            faces[index].push(value);
        }
        hash = fnv_update(hash, &bytes);
        hashes[index] = hash;
        aggregate = fnv_update(aggregate, &bytes);
    }
    Ok((TectonicTerrain { faces, resolution }, hashes, aggregate))
}

pub fn load_canonical_terrain(
    dir: &Path,
    resolution: u32,
    max_resolution: u32,
) -> Result<TectonicTerrain, TerrainArtifactError> {
    if resolution > max_resolution || infer_canonical_resolution(dir, max_resolution)? != resolution
    {
        return Err(TerrainArtifactError::Resolution);
    }
    Ok(load_faces(dir, resolution)?.0)
}

const KEYS: [&str; 32] = [
    "approval_version",
    "artifact_schema_version",
    "artifact_path",
    "resolution",
    "face.posx.fnv1a64",
    "face.negx.fnv1a64",
    "face.posy.fnv1a64",
    "face.negy.fnv1a64",
    "face.posz.fnv1a64",
    "face.negz.fnv1a64",
    "candidate_fnv1a64",
    "control_fnv1a64",
    "validator_commit",
    "evidence.path",
    "evidence.fnv1a64",
    "gate.artifact",
    "gate.control",
    "gate.byte_equality",
    "gate.orientation",
    "gate.seams",
    "gate.normals",
    "gate.poles",
    "gate.provenance",
    "gate.rights",
    "gate.resource_capture",
    "gate.human_review",
    "units.height",
    "units.horizontal",
    "sea_level",
    "control_ocean_level",
    "export_policy",
    "status",
];

fn lowercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn hex16(value: &str) -> Result<u64, TerrainArtifactError> {
    if !lowercase_hex(value, 16) {
        return Err(TerrainArtifactError::Schema);
    }
    u64::from_str_radix(value, 16).map_err(|_| TerrainArtifactError::Schema)
}

fn value<'a>(fields: &'a [(&'a str, &'a str)], key: &str) -> Result<&'a str, TerrainArtifactError> {
    fields
        .iter()
        .find_map(|(current, value)| (*current == key).then_some(*value))
        .ok_or(TerrainArtifactError::Schema)
}

pub fn parse_approval(bytes: &[u8]) -> Result<TerrainArtifactApproval, TerrainArtifactError> {
    if bytes.len() > MAX_APPROVAL_BYTES as usize
        || !bytes.ends_with(b"\n")
        || bytes.contains(&b'\r')
    {
        return Err(TerrainArtifactError::Schema);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| TerrainArtifactError::Schema)?;
    let lines: Vec<_> = text
        .strip_suffix('\n')
        .ok_or(TerrainArtifactError::Schema)?
        .split('\n')
        .collect();
    if lines.len() != KEYS.len() {
        return Err(TerrainArtifactError::Schema);
    }
    let mut fields = Vec::with_capacity(KEYS.len());
    for (line, expected) in lines.iter().zip(KEYS) {
        let (key, value) = line.split_once('=').ok_or(TerrainArtifactError::Schema)?;
        if key != expected
            || value.is_empty()
            || key.contains(char::is_whitespace)
            || value.contains(char::is_whitespace)
        {
            return Err(TerrainArtifactError::Schema);
        }
        fields.push((key, value));
    }
    if value(&fields, "approval_version")? != "1"
        || value(&fields, "artifact_schema_version")? != "1"
        || value(&fields, "units.height")? != "normalized_control_range"
        || value(&fields, "units.horizontal")? != "unit_sphere"
        || value(&fields, "sea_level")? != "control_ocean_level"
        || value(&fields, "export_policy")? != "REFUSE_IMPORTED"
        || value(&fields, "status")? != "APPROVED"
        || KEYS[15..26]
            .iter()
            .any(|key| !matches!(value(&fields, key), Ok("PASS")))
    {
        return Err(TerrainArtifactError::Approval);
    }
    let artifact_path = value(&fields, "artifact_path")?.to_owned();
    normalized_relative(Path::new(&artifact_path))?;
    let evidence_path = value(&fields, "evidence.path")?.to_owned();
    normalized_relative(Path::new(&evidence_path))?;
    let resolution = TerrainResolution::parse(value(&fields, "resolution")?)?;
    let face_fnv1a64 = [
        hex16(value(&fields, "face.posx.fnv1a64")?)?,
        hex16(value(&fields, "face.negx.fnv1a64")?)?,
        hex16(value(&fields, "face.posy.fnv1a64")?)?,
        hex16(value(&fields, "face.negy.fnv1a64")?)?,
        hex16(value(&fields, "face.posz.fnv1a64")?)?,
        hex16(value(&fields, "face.negz.fnv1a64")?)?,
    ];
    let candidate_fnv1a64 = hex16(value(&fields, "candidate_fnv1a64")?)?;
    let control_fnv1a64 = hex16(value(&fields, "control_fnv1a64")?)?;
    let validator_commit = value(&fields, "validator_commit")?;
    if !lowercase_hex(validator_commit, 40) {
        return Err(TerrainArtifactError::Schema);
    }
    if control_fnv1a64 == KNOWN_NO_GO_CONTROL_FNV
        || validator_commit == KNOWN_NO_GO_VALIDATOR_COMMIT
        || candidate_fnv1a64 == control_fnv1a64
        || candidate_fnv1a64 == KNOWN_NO_GO_CONTROL_FNV
    {
        return Err(TerrainArtifactError::Identity);
    }
    let control_ocean_level = value(&fields, "control_ocean_level").and_then(|value| {
        let level = value
            .parse::<f32>()
            .map_err(|_| TerrainArtifactError::Schema)?;
        (level.is_finite() && (-0.5..=1.2).contains(&level) && level.to_string() == value)
            .then_some(level)
            .ok_or(TerrainArtifactError::Schema)
    })?;
    Ok(TerrainArtifactApproval {
        artifact_path,
        resolution,
        face_fnv1a64,
        candidate_fnv1a64,
        control_fnv1a64,
        validator_commit: validator_commit.to_owned(),
        evidence_path,
        evidence_fnv1a64: hex16(value(&fields, "evidence.fnv1a64")?)?,
        control_ocean_level,
    })
}

pub fn load_approved_terrain(
    artifact_dir: &Path,
    approval_file: &Path,
) -> Result<ApprovedTerrainArtifact, TerrainArtifactError> {
    let (approval_file, _) = resolve_under_cwd(approval_file, false)?;
    let approval_metadata = fs::metadata(&approval_file)?;
    if approval_metadata.len() > MAX_APPROVAL_BYTES {
        return Err(TerrainArtifactError::Size);
    }
    let approval = parse_approval(&fs::read(&approval_file)?)?;
    let (artifact_dir, artifact_display) = resolve_under_cwd(artifact_dir, true)?;
    if artifact_display != approval.artifact_path {
        return Err(TerrainArtifactError::Identity);
    }
    exact_artifact_entries(&artifact_dir)?;
    if infer_resolution(&artifact_dir)? != approval.resolution.value() {
        return Err(TerrainArtifactError::Resolution);
    }
    let (evidence, evidence_display) =
        resolve_under_cwd(Path::new(&approval.evidence_path), false)?;
    if evidence_display != approval.evidence_path
        || fs::metadata(&evidence)?.len() > MAX_EVIDENCE_BYTES
    {
        return Err(TerrainArtifactError::Evidence);
    }
    let evidence_bytes = fs::read(evidence)?;
    if !evidence_bytes.ends_with(b"\n")
        || evidence_bytes.contains(&b'\r')
        || std::str::from_utf8(&evidence_bytes).is_err()
        || fnv1a64(&evidence_bytes) != approval.evidence_fnv1a64
    {
        return Err(TerrainArtifactError::Evidence);
    }
    let (terrain, face_hashes, candidate_hash) =
        load_faces(&artifact_dir, approval.resolution.value())?;
    if face_hashes != approval.face_fnv1a64 || candidate_hash != approval.candidate_fnv1a64 {
        return Err(TerrainArtifactError::Identity);
    }
    Ok(ApprovedTerrainArtifact {
        terrain: Arc::new(terrain),
        approval,
    })
}

pub fn load_approved(
    artifact_dir: &Path,
    approval_file: &Path,
) -> Result<ApprovedTerrainArtifact, TerrainArtifactError> {
    load_approved_terrain(artifact_dir, approval_file)
}
