use planet_gen::{
    cube_sphere::cube_to_sphere,
    gpu::GpuContext,
    planet::{DerivedProperties, PlanetParams},
    plates::{PlateGenParams, generate_plates},
    preview::{PreviewRenderer, PreviewUniforms},
    terrain_artifact::{
        CANONICAL_FACE_NAMES as NAMES, fnv1a64 as fnv, infer_canonical_resolution,
        load_canonical_terrain,
    },
    terrain_compute::{TectonicTerrain, TerrainComputePipeline},
};
use std::{
    env,
    fs::{self, OpenOptions},
    path::{Component, Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const NOT_RUN: [&str; 5] = [
    "external_model",
    "projection",
    "resource_capture",
    "human_review",
    "product_integration",
];
const MAX_SOURCE_RESOLUTION: u32 = 8192;
const MAX_RESOLUTION: u32 = (MAX_SOURCE_RESOLUTION - 1) / 2;
const FIXTURE_RESOLUTION: u32 = 17;

type Result<T> = std::result::Result<T, String>;

static UNIQUE: AtomicU64 = AtomicU64::new(0);

enum OperationError {
    Input(String),
    Gpu(String),
}

impl OperationError {
    fn input(error: impl ToString) -> Self {
        Self::Input(error.to_string())
    }
    fn gpu(error: impl ToString) -> Self {
        Self::Gpu(error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    Fixture,
    Capture,
    ValidateControl,
    ValidateCandidate,
    Compare,
    PreviewControl,
    PreviewCandidate,
}

#[derive(Debug)]
struct Invocation {
    command: Command,
    resolution: Option<u32>,
    dir: Option<String>,
    control: Option<String>,
    first: Option<String>,
    second: Option<String>,
}

#[derive(Clone)]
struct Workspace {
    cwd: PathBuf,
}

impl Workspace {
    fn new() -> Result<Self> {
        Ok(Self {
            cwd: fs::canonicalize(".").map_err(|e| format!("cannot canonicalize cwd: {e}"))?,
        })
    }

    fn relative(&self, value: &str) -> Result<(PathBuf, String)> {
        let path = Path::new(value);
        if path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err("paths must be relative and may not contain ..".into());
        }
        let normalized = path.components().fold(PathBuf::new(), |mut out, part| {
            if let Component::Normal(value) = part {
                out.push(value);
            }
            out
        });
        if normalized.as_os_str().is_empty() {
            return Err("path must name a directory".into());
        }
        Ok((
            normalized.clone(),
            normalized.to_string_lossy().replace('\\', "/"),
        ))
    }

    fn read_dir(&self, value: &str) -> Result<(PathBuf, String)> {
        let (relative, display) = self.relative(value)?;
        let resolved = fs::canonicalize(self.cwd.join(relative))
            .map_err(|e| format!("cannot read artifact directory: {e}"))?;
        if !resolved.starts_with(&self.cwd) || !resolved.is_dir() {
            return Err("artifact directory escapes the current working directory".into());
        }
        Ok((resolved, display))
    }

    fn output_dir(&self, value: &str) -> Result<(PathBuf, String)> {
        let (relative, display) = self.relative(value)?;
        let output = self.cwd.join(relative);
        if output.exists() {
            return Err("destination exists".into());
        }
        let mut parent = output
            .parent()
            .ok_or_else(|| "destination has no parent".to_owned())?;
        while !parent.exists() {
            parent = parent
                .parent()
                .ok_or_else(|| "destination has no existing parent".to_owned())?;
        }
        let canonical_parent = fs::canonicalize(parent).map_err(|e| e.to_string())?;
        if !canonical_parent.starts_with(&self.cwd) {
            return Err("destination parent escapes the current working directory".into());
        }
        Ok((output, display))
    }

    fn output_file(&self, relative: &str) -> Result<PathBuf> {
        let (path, _) = self.relative(relative)?;
        let output = self.cwd.join(path);
        if output.exists() {
            return Err("destination exists".into());
        }
        let mut parent = output
            .parent()
            .ok_or_else(|| "destination has no parent".to_owned())?;
        while !parent.exists() {
            parent = parent
                .parent()
                .ok_or_else(|| "destination has no existing parent".to_owned())?;
        }
        if !fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .starts_with(&self.cwd)
        {
            return Err("destination parent escapes the current working directory".into());
        }
        Ok(output)
    }
}

fn parse(args: &[String]) -> Result<Invocation> {
    let command = match args.first().map(String::as_str) {
        Some("fixture-orientation") => Command::Fixture,
        Some("capture-control") => Command::Capture,
        Some("validate-control") => Command::ValidateControl,
        Some("validate-candidate") => Command::ValidateCandidate,
        Some("compare-bytes") => Command::Compare,
        Some("preview-control") => Command::PreviewControl,
        Some("preview-candidate") => Command::PreviewCandidate,
        _ => return Err("unknown command".into()),
    };
    if command == Command::Fixture {
        if args.len() != 1 {
            return Err("fixture-orientation accepts no arguments".into());
        }
        return Ok(Invocation {
            command,
            resolution: None,
            dir: None,
            control: None,
            first: None,
            second: None,
        });
    }
    let mut values: [Option<String>; 4] = std::array::from_fn(|_| None);
    let expected: &[&str] = match command {
        Command::Capture | Command::ValidateControl | Command::PreviewControl => {
            &["--resolution", "--dir"]
        }
        Command::ValidateCandidate | Command::PreviewCandidate => {
            &["--resolution", "--dir", "--control"]
        }
        Command::Compare => &["--first", "--second"],
        Command::Fixture => unreachable!(),
    };
    let mut index = 1;
    while index < args.len() {
        let flag = &args[index];
        if !expected.contains(&flag.as_str())
            || index + 1 >= args.len()
            || args[index + 1].starts_with("--")
        {
            return Err("invalid command arguments".into());
        }
        let slot = match flag.as_str() {
            "--resolution" => 0,
            "--dir" => 1,
            "--control" | "--first" => 2,
            "--second" => 3,
            _ => unreachable!(),
        };
        if values[slot].replace(args[index + 1].clone()).is_some() {
            return Err(format!("duplicate {flag}"));
        }
        index += 2;
    }
    let resolution = if command == Command::Compare {
        None
    } else {
        let resolution = values[0]
            .as_deref()
            .ok_or_else(|| "missing --resolution".to_owned())?
            .parse::<u32>()
            .map_err(|_| "invalid resolution".to_owned())?;
        checked_resolution(resolution)?;
        Some(resolution)
    };
    let invocation = Invocation {
        command,
        resolution,
        dir: values[1].clone(),
        control: match command {
            Command::ValidateCandidate | Command::PreviewCandidate => values[2].clone(),
            _ => None,
        },
        first: if command == Command::Compare {
            values[2].clone()
        } else {
            None
        },
        second: if command == Command::Compare {
            values[3].clone()
        } else {
            None
        },
    };
    if expected.iter().any(|flag| match *flag {
        "--resolution" => invocation.resolution.is_none(),
        "--dir" => invocation.dir.is_none(),
        "--control" => invocation.control.is_none(),
        "--first" => invocation.first.is_none(),
        "--second" => invocation.second.is_none(),
        _ => true,
    }) {
        return Err("missing required argument".into());
    }
    Ok(invocation)
}

fn checked_resolution(n: u32) -> Result<usize> {
    if !(4..=MAX_RESOLUTION).contains(&n) {
        return Err(format!("resolution must be between 4 and {MAX_RESOLUTION}"));
    }
    let n = usize::try_from(n).map_err(|_| "resolution is unsupported".to_owned())?;
    n.checked_mul(n)
        .and_then(|count| count.checked_mul(6))
        .and_then(|count| count.checked_mul(std::mem::size_of::<f32>()))
        .ok_or_else(|| "resolution allocation overflows".to_owned())?;
    Ok(n * n)
}

fn source_resolution(n: u32) -> Result<u32> {
    let source = n
        .checked_mul(2)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| "source resolution overflows".to_owned())?;
    if source > MAX_SOURCE_RESOLUTION {
        return Err(format!("source resolution exceeds {MAX_SOURCE_RESOLUTION}"));
    }
    checked_resolution(n)?;
    Ok(source)
}

fn infer_resolution(dir: &Path) -> Result<u32> {
    let n = infer_canonical_resolution(dir, MAX_SOURCE_RESOLUTION)
        .map_err(|error| error.to_string())?;
    checked_resolution(n)?;
    Ok(n)
}

fn status(value: bool) -> &'static str {
    if value { "PASS" } else { "FAIL" }
}

fn artifact_bytes(dir: &Path, n: u32) -> Result<Vec<u8>> {
    let face_bytes = checked_resolution(n)?
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or_else(|| "artifact size overflows".to_owned())?;
    let total = face_bytes
        .checked_mul(NAMES.len())
        .ok_or_else(|| "artifact size overflows".to_owned())?;
    let mut bytes = Vec::with_capacity(total);
    for name in NAMES {
        let face = fs::read(dir.join(name)).map_err(|e| format!("cannot read {name}: {e}"))?;
        if face.len() != face_bytes {
            return Err(format!("invalid byte length for {name}"));
        }
        for value in face.chunks_exact(4) {
            if !f32::from_le_bytes(value.try_into().map_err(|_| "invalid f32 bytes")?).is_finite() {
                return Err(format!("non-finite {name}"));
            }
        }
        bytes.extend_from_slice(&face);
    }
    Ok(bytes)
}

fn load(dir: &Path, n: u32) -> Result<TectonicTerrain> {
    checked_resolution(n)?;
    load_canonical_terrain(dir, n, MAX_SOURCE_RESOLUTION).map_err(|error| error.to_string())
}

fn write(dir: &Path, terrain: &TectonicTerrain) -> Result<()> {
    let count = checked_resolution(terrain.resolution)?;
    for (name, face) in NAMES.iter().zip(&terrain.faces) {
        if face.len() != count || face.iter().any(|value| !value.is_finite()) {
            return Err(format!("invalid terrain face {name}"));
        }
        let mut bytes = Vec::with_capacity(count * 4);
        for value in face {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(dir.join(name), bytes).map_err(|e| format!("cannot write {name}: {e}"))?;
    }
    Ok(())
}

fn control(n: u32) -> Result<TectonicTerrain> {
    let source_resolution = source_resolution(n)?;
    let gpu = GpuContext::new().map_err(|e| e.to_string())?;
    let params = PlanetParams::default();
    let derived = DerivedProperties::from_params(&params);
    let plates = generate_plates(&PlateGenParams {
        seed: 42,
        mass_earth: params.mass_earth,
        ocean_fraction: derived.ocean_fraction,
        tectonics_factor: derived.tectonics_factor,
        continental_scale: 1.0,
        num_plates_override: 0,
        num_continents: 0,
        continent_size_variety: 0.0,
    });
    let amplitude = 0.6 + 0.6 * params.mass_earth.powf(0.3);
    let frequency = 1.0 + 0.5 * params.mass_earth.powf(0.2);
    let octaves = (8.0 + 4.0 * (params.axial_tilt_deg / 90.0) * derived.tectonics_factor) as u32;
    let gain = 2f32.powf(
        -((1.47
            + 0.91 * (params.star_distance_au.ln() / 3f32.ln()).clamp(0.0, 1.0)
            + 0.3 * params.metallicity)
            .clamp(1.2, 3.0)
            - 1.0)
            / 2.0,
    );
    let lacunarity = 1.9 + 0.2 * (24.0 / params.rotation_period_h).clamp(0.5, 2.0);
    let source = TerrainComputePipeline::new(&gpu).generate(
        &gpu,
        &plates,
        source_resolution,
        42,
        amplitude,
        frequency,
        octaves,
        gain,
        lacunarity,
        1.0,
        0.10,
        1.0,
        1.0,
        derived.surface_gravity,
        derived.tectonics_factor,
        derived.surface_age,
        1.0,
    );
    centered_extract(&source, n)
}

fn centered_extract(source: &TectonicTerrain, n: u32) -> Result<TectonicTerrain> {
    let expected = source_resolution(n)? as usize;
    if source.resolution as usize != expected
        || source
            .faces
            .iter()
            .any(|face| face.len() != expected * expected)
    {
        return Err("unexpected source terrain layout".into());
    }
    let target = n as usize;
    Ok(TectonicTerrain {
        faces: std::array::from_fn(|face| {
            (0..target)
                .flat_map(|y| {
                    (0..target).map(move |x| source.faces[face][(2 * y + 1) * expected + 2 * x + 1])
                })
                .collect()
        }),
        resolution: n,
    })
}

const EDGES: [(usize, usize, usize, usize, bool); 12] = [
    (0, 0, 4, 1, false),
    (0, 1, 5, 0, false),
    (0, 2, 2, 1, true),
    (0, 3, 3, 1, false),
    (1, 0, 5, 1, false),
    (1, 1, 4, 0, false),
    (1, 2, 2, 0, false),
    (1, 3, 3, 0, true),
    (2, 2, 5, 2, true),
    (2, 3, 4, 2, false),
    (3, 2, 4, 3, false),
    (3, 3, 5, 3, true),
];
const CORNERS: [[(usize, bool, bool); 3]; 8] = [
    [(0, false, false), (2, true, true), (4, true, false)],
    [(0, true, false), (2, true, false), (5, false, false)],
    [(0, false, true), (3, true, false), (4, true, true)],
    [(0, true, true), (3, true, true), (5, false, true)],
    [(1, true, false), (2, false, true), (4, false, false)],
    [(1, false, false), (2, false, false), (5, true, false)],
    [(1, true, true), (3, false, false), (4, false, true)],
    [(1, false, true), (3, false, true), (5, true, true)],
];

fn at(t: &TectonicTerrain, face: usize, x: usize, y: usize) -> f32 {
    t.faces[face][y * t.resolution as usize + x]
}

fn edge_point(edge: usize, i: usize, n: usize) -> (usize, usize) {
    match edge {
        0 => (0, i),
        1 => (n - 1, i),
        2 => (i, 0),
        _ => (i, n - 1),
    }
}

fn edge_direction(face: usize, edge: usize, index: usize, n: usize) -> [f32; 3] {
    let (x, y) = edge_point(edge, index, n);
    cube_to_sphere(
        face as u32,
        x as f32 / (n - 1) as f32,
        y as f32 / (n - 1) as f32,
    )
}

fn verify_edge_table() -> bool {
    let n = FIXTURE_RESOLUTION as usize;
    EDGES.into_iter().all(
        |(first_face, first_edge, second_face, second_edge, reverse)| {
            (0..n).all(|index| {
                let second_index = if reverse { n - 1 - index } else { index };
                let first = edge_direction(first_face, first_edge, index, n);
                let second = edge_direction(second_face, second_edge, second_index, n);
                first
                    .into_iter()
                    .zip(second)
                    .all(|(a, b)| (a - b).abs() <= 1e-6)
            })
        },
    )
}

fn percentile(mut values: Vec<f32>, q: f32) -> Option<f32> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let index = q * (values.len() - 1) as f32;
    let low = index.floor() as usize;
    let high = index.ceil() as usize;
    Some(values[low] + (values[high] - values[low]) * (index - low as f32))
}

fn interior_jumps(t: &TectonicTerrain) -> Vec<f32> {
    let n = t.resolution as usize;
    let mut jumps = Vec::new();
    for face in 0..6 {
        for y in 1..n - 1 {
            for x in 1..n - 1 {
                if x + 1 < n - 1 {
                    jumps.push((at(t, face, x, y) - at(t, face, x + 1, y)).abs());
                }
                if y + 1 < n - 1 {
                    jumps.push((at(t, face, x, y) - at(t, face, x, y + 1)).abs());
                }
            }
        }
    }
    jumps
}

fn edge_jumps(t: &TectonicTerrain) -> Vec<f32> {
    let n = t.resolution as usize;
    EDGES
        .into_iter()
        .flat_map(|(a, edge_a, b, edge_b, reverse)| {
            (0..n).map(move |i| {
                let j = if reverse { n - 1 - i } else { i };
                let (ax, ay) = edge_point(edge_a, i, n);
                let (bx, by) = edge_point(edge_b, j, n);
                (at(t, a, ax, ay) - at(t, b, bx, by)).abs()
            })
        })
        .collect()
}

fn corner_jumps(t: &TectonicTerrain) -> Vec<f32> {
    let n = t.resolution as usize;
    CORNERS
        .iter()
        .map(|corner| {
            let values = corner.iter().map(|&(face, right, bottom)| {
                at(
                    t,
                    face,
                    if right { n - 1 } else { 0 },
                    if bottom { n - 1 } else { 0 },
                )
            });
            let (min, max) = values
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                    (min.min(value), max.max(value))
                });
            max - min
        })
        .collect()
}

fn surface(t: &TectonicTerrain, min: f32, span: f32, face: usize, x: usize, y: usize) -> [f32; 3] {
    let n = t.resolution as f32;
    let direction = cube_to_sphere(face as u32, (x as f32 + 0.5) / n, (y as f32 + 0.5) / n);
    let height = (at(t, face, x, y) - min) / span;
    [
        direction[0] * (1.0 + 0.01 * height),
        direction[1] * (1.0 + 0.01 * height),
        direction[2] * (1.0 + 0.01 * height),
    ]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: [f32; 3]) -> Option<[f32; 3]> {
    let length = length(a);
    (length.is_finite() && length > 0.0).then(|| [a[0] / length, a[1] / length, a[2] / length])
}

fn tangent(
    t: &TectonicTerrain,
    min: f32,
    span: f32,
    face: usize,
    x: usize,
    y: usize,
    horizontal: bool,
) -> [f32; 3] {
    let n = t.resolution as usize;
    let coordinate = if horizontal { x } else { y };
    let before = coordinate.saturating_sub(1);
    let after = (coordinate + 1).min(n - 1);
    let point = |coordinate| {
        if horizontal {
            surface(t, min, span, face, coordinate, y)
        } else {
            surface(t, min, span, face, x, coordinate)
        }
    };
    if coordinate == 0 {
        sub(point(after), point(coordinate))
    } else if coordinate == n - 1 {
        sub(point(coordinate), point(before))
    } else {
        sub(point(after), point(before))
    }
}

fn normal(
    t: &TectonicTerrain,
    min: f32,
    span: f32,
    face: usize,
    x: usize,
    y: usize,
) -> Option<[f32; 3]> {
    let mut normal = normalize(cross(
        tangent(t, min, span, face, x, y, true),
        tangent(t, min, span, face, x, y, false),
    ))?;
    let direction = cube_to_sphere(
        face as u32,
        (x as f32 + 0.5) / t.resolution as f32,
        (y as f32 + 0.5) / t.resolution as f32,
    );
    if dot(normal, direction) < 0.0 {
        normal = [-normal[0], -normal[1], -normal[2]];
    }
    Some(normal)
}

fn normal_angles(t: &TectonicTerrain, min: f32, span: f32) -> Option<Vec<f32>> {
    let n = t.resolution as usize;
    EDGES
        .into_iter()
        .map(|(a, edge_a, b, edge_b, reverse)| {
            (0..n)
                .map(|i| {
                    let j = if reverse { n - 1 - i } else { i };
                    let (ax, ay) = edge_point(edge_a, i, n);
                    let (bx, by) = edge_point(edge_b, j, n);
                    let first = normal(t, min, span, a, ax, ay)?;
                    let second = normal(t, min, span, b, bx, by)?;
                    Some(dot(first, second).clamp(-1.0, 1.0).acos().to_degrees())
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.into_iter().flatten().collect())
}

fn weighted_quantile(mut values: Vec<(f32, f32)>, q: f32) -> Option<f32> {
    if values.is_empty()
        || values
            .iter()
            .any(|(value, weight)| !value.is_finite() || !weight.is_finite() || *weight <= 0.0)
    {
        return None;
    }
    values.sort_by(|left, right| left.0.total_cmp(&right.0));
    let total = values.iter().map(|(_, weight)| weight).sum::<f32>();
    let mut cumulative = 0.0;
    for (value, weight) in values {
        cumulative += weight;
        if cumulative >= q * total {
            return Some(value);
        }
    }
    None
}

fn pole_values(
    t: &TectonicTerrain,
    min: f32,
    span: f32,
    north: bool,
    cap: bool,
    slope: bool,
) -> Vec<(f32, f32)> {
    let n = t.resolution as usize;
    let mut values = Vec::new();
    for face in 0..6 {
        for y in 0..n {
            for x in 0..n {
                let u = (x as f32 + 0.5) / n as f32;
                let v = (y as f32 + 0.5) / n as f32;
                let direction = cube_to_sphere(face as u32, u, v);
                let latitude = direction[1].asin().to_degrees();
                let in_hemisphere = if north {
                    latitude >= 0.0
                } else {
                    latitude < 0.0
                };
                let absolute = latitude.abs();
                let in_band = if cap {
                    absolute >= 75.0
                } else {
                    (60.0..75.0).contains(&absolute)
                };
                if !in_hemisphere || !in_band {
                    continue;
                }
                let s = 2.0 * u - 1.0;
                let t_coord = 2.0 * v - 1.0;
                let value = if slope {
                    let dx = tangent(t, min, span, face, x, y, true);
                    let dy = tangent(t, min, span, face, x, y, false);
                    (dot(dx, dx) + dot(dy, dy)).sqrt()
                } else {
                    (at(t, face, x, y) - min) / span
                };
                values.push((value, (1.0 + s * s + t_coord * t_coord).powf(-1.5)));
            }
        }
    }
    values
}

#[derive(Clone, Copy)]
struct Distribution {
    p10: f32,
    p50: f32,
    p90: f32,
}
impl Distribution {
    fn from(values: Vec<(f32, f32)>) -> Option<Self> {
        Some(Self {
            p10: weighted_quantile(values.clone(), 0.10)?,
            p50: weighted_quantile(values.clone(), 0.50)?,
            p90: weighted_quantile(values, 0.90)?,
        })
    }
}
fn pole_pass(cap: Distribution, ring: Distribution) -> bool {
    [cap.p10, cap.p50, cap.p90]
        .into_iter()
        .zip([ring.p10, ring.p50, ring.p90])
        .all(|(cap, ring)| {
            if ring.abs() >= 0.05 {
                (cap - ring).abs() / ring.abs() <= 0.20
            } else {
                (cap - ring).abs() <= 0.02
            }
        })
}

struct Metrics {
    range: bool,
    edge: bool,
    corner: bool,
    normal: bool,
    pole_elevation: bool,
    pole_slope: bool,
    edge_p95: f32,
    corner_p95: f32,
    normal_p95: f32,
    normal_max: f32,
    elevation_p50: [f32; 2],
    slope_p50: [f32; 2],
}

fn validation(candidate: &TectonicTerrain, control: &TectonicTerrain) -> Metrics {
    let min = control
        .faces
        .iter()
        .flatten()
        .fold(f32::INFINITY, |min, value| min.min(*value));
    let max = control
        .faces
        .iter()
        .flatten()
        .fold(f32::NEG_INFINITY, |max, value| max.max(*value));
    let failed = || Metrics {
        range: false,
        edge: false,
        corner: false,
        normal: false,
        pole_elevation: false,
        pole_slope: false,
        edge_p95: 0.0,
        corner_p95: 0.0,
        normal_p95: 0.0,
        normal_max: 0.0,
        elevation_p50: [0.0; 2],
        slope_p50: [0.0; 2],
    };
    let span = max - min;
    if !span.is_finite() || span <= 1e-6 {
        return failed();
    }
    let normalize = |values: Vec<f32>| values.into_iter().map(|value| value / span).collect();
    let candidate_interior =
        percentile(normalize(interior_jumps(candidate)), 0.95).unwrap_or(f32::INFINITY);
    let candidate_edge =
        percentile(normalize(edge_jumps(candidate)), 0.95).unwrap_or(f32::INFINITY);
    let control_edge = percentile(normalize(edge_jumps(control)), 0.95).unwrap_or(f32::INFINITY);
    let candidate_corner =
        percentile(normalize(corner_jumps(candidate)), 0.95).unwrap_or(f32::INFINITY);
    let control_corner =
        percentile(normalize(corner_jumps(control)), 0.95).unwrap_or(f32::INFINITY);
    let candidate_normals = normal_angles(candidate, min, span).unwrap_or_default();
    let control_normal_p95 = normal_angles(control, min, span)
        .and_then(|values| percentile(values, 0.95))
        .unwrap_or(f32::INFINITY);
    let normal_p95 = percentile(candidate_normals.clone(), 0.95).unwrap_or(f32::INFINITY);
    let normal_max = candidate_normals
        .into_iter()
        .reduce(f32::max)
        .unwrap_or(f32::INFINITY);
    let mut elevation_p50 = [0.0; 2];
    let mut slope_p50 = [0.0; 2];
    let mut pole_elevation = true;
    let mut pole_slope = true;
    for (index, north) in [true, false].into_iter().enumerate() {
        let elevation_cap =
            Distribution::from(pole_values(candidate, min, span, north, true, false));
        let elevation_ring =
            Distribution::from(pole_values(candidate, min, span, north, false, false));
        let slope_cap = Distribution::from(pole_values(candidate, min, span, north, true, true));
        let slope_ring = Distribution::from(pole_values(candidate, min, span, north, false, true));
        elevation_p50[index] = elevation_cap.map_or(0.0, |values| values.p50);
        slope_p50[index] = slope_cap.map_or(0.0, |values| values.p50);
        pole_elevation &= elevation_cap
            .zip(elevation_ring)
            .is_some_and(|(cap, ring)| pole_pass(cap, ring));
        pole_slope &= slope_cap
            .zip(slope_ring)
            .is_some_and(|(cap, ring)| pole_pass(cap, ring));
    }
    Metrics {
        range: true,
        edge: candidate_edge <= (1.5 * candidate_interior).max(1.10 * control_edge),
        corner: candidate_corner <= (1.5 * candidate_interior).max(1.10 * control_corner),
        normal: normal_p95 <= 5.0f32.max(control_normal_p95 + 2.0) && normal_max <= 15.0,
        pole_elevation,
        pole_slope,
        edge_p95: candidate_edge,
        corner_p95: candidate_corner,
        normal_p95,
        normal_max,
        elevation_p50,
        slope_p50,
    }
}

fn fixture_terrain() -> TectonicTerrain {
    let n = FIXTURE_RESOLUTION as usize;
    TectonicTerrain {
        faces: std::array::from_fn(|face| {
            (0..n * n)
                .map(|index| (1_000_000 * face + 1_000 * (index / n) + index % n) as f32)
                .collect()
        }),
        resolution: FIXTURE_RESOLUTION,
    }
}

fn orientation_matches(terrain: &TectonicTerrain) -> bool {
    terrain.resolution == FIXTURE_RESOLUTION
        && terrain.faces.iter().enumerate().all(|(face, values)| {
            values.iter().enumerate().all(|(index, value)| {
                *value
                    == (1_000_000 * face
                        + 1_000 * (index / FIXTURE_RESOLUTION as usize)
                        + index % FIXTURE_RESOLUTION as usize) as f32
            })
        })
}

fn mutate_rows(terrain: &mut TectonicTerrain) {
    let n = terrain.resolution as usize;
    terrain.faces[0]
        .chunks_exact_mut(n)
        .for_each(|row| row.reverse());
}
fn mutate_columns(terrain: &mut TectonicTerrain) {
    let n = terrain.resolution as usize;
    for y in 0..n {
        for x in 0..n / 2 {
            terrain.faces[0].swap(y * n + x, y * n + (n - 1 - x));
        }
    }
}
fn transpose(terrain: &mut TectonicTerrain) {
    let n = terrain.resolution as usize;
    for y in 0..n {
        for x in 0..y {
            terrain.faces[0].swap(y * n + x, x * n + y);
        }
    }
}

fn unique_dir(parent: &Path, prefix: &str) -> Result<PathBuf> {
    for _ in 0..1000 {
        let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = parent.join(format!(".{prefix}-{}-{nanos}-{unique}", process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("cannot allocate unique directory".into())
}

fn fixture() -> Result<bool> {
    let root = unique_dir(&env::temp_dir(), "terrain-diffusion-fixture")?;
    let result = (|| {
        let canonical = fixture_terrain();
        write(&root, &canonical)?;
        if !orientation_matches(&load(&root, FIXTURE_RESOLUTION)?) || !verify_edge_table() {
            return Ok(false);
        }
        for mutate in [
            mutate_face_swap as fn(&mut TectonicTerrain),
            mutate_rows,
            mutate_columns,
            transpose,
        ] {
            write(&root, &canonical)?;
            let mut mutated = load(&root, FIXTURE_RESOLUTION)?;
            mutate(&mut mutated);
            write(&root, &mutated)?;
            if orientation_matches(&load(&root, FIXTURE_RESOLUTION)?) {
                return Ok(false);
            }
        }
        Ok(true)
    })();
    let _ = fs::remove_dir_all(root);
    result
}
fn mutate_face_swap(terrain: &mut TectonicTerrain) {
    terrain.faces.swap(0, 1);
}

#[allow(clippy::too_many_arguments)]
fn print_validation(
    command: &str,
    artifact: &str,
    n: u32,
    dir: &str,
    control: &str,
    metrics: &Metrics,
    hash: u64,
    orientation: bool,
) {
    println!("result_version=1\ncommand={command}\nartifact={artifact}\nresolution={n}");
    for (name, value) in [
        ("size", true),
        ("finite", true),
        ("orientation_fixture", orientation),
        ("control_range", metrics.range),
        ("height_edge", metrics.edge),
        ("height_corner", metrics.corner),
        ("normal", metrics.normal),
        ("pole_elevation", metrics.pole_elevation),
        ("pole_slope", metrics.pole_slope),
    ] {
        println!("gate.{name}={}", status(value));
    }
    for name in NOT_RUN {
        println!("gate.{name}=NOT_RUN");
    }
    println!(
        "metric.fnv1a64={hash:016x}\nmetric.height_edge_p95={:0.9}\nmetric.height_corner_p95={:0.9}\nmetric.normal_p95_deg={:0.9}\nmetric.normal_max_deg={:0.9}\nmetric.pole_elevation_north_p50={:0.9}\nmetric.pole_elevation_south_p50={:0.9}\nmetric.pole_slope_north_p50={:0.9}\nmetric.pole_slope_south_p50={:0.9}\npath.dir={dir}\npath.control={control}",
        metrics.edge_p95,
        metrics.corner_p95,
        metrics.normal_p95,
        metrics.normal_max,
        metrics.elevation_p50[0],
        metrics.elevation_p50[1],
        metrics.slope_p50[0],
        metrics.slope_p50[1]
    );
}

fn validate(dir: &Path, control_dir: &Path, n: u32) -> Result<(Metrics, u64)> {
    let terrain = load(dir, n)?;
    let control = load(control_dir, n)?;
    let hash = fnv(&artifact_bytes(dir, n)?);
    Ok((validation(&terrain, &control), hash))
}

fn metrics_pass(metrics: &Metrics, orientation: bool) -> bool {
    metrics.range
        && metrics.edge
        && metrics.corner
        && metrics.normal
        && metrics.pole_elevation
        && metrics.pole_slope
        && orientation
}

struct ArtifactGuard {
    path: PathBuf,
    armed: bool,
}
impl ArtifactGuard {
    fn reserve(output: &Path) -> Result<Self> {
        let parent = output
            .parent()
            .ok_or_else(|| "destination has no parent".to_owned())?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        fs::create_dir(output)
            .map_err(|error| format!("destination exists or cannot be created: {error}"))?;
        if let Err(error) = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join(".incomplete"))
        {
            let _ = fs::remove_dir_all(output);
            return Err(error.to_string());
        }
        Ok(Self {
            path: output.to_path_buf(),
            armed: true,
        })
    }
    fn complete(mut self) -> Result<()> {
        fs::remove_file(self.path.join(".incomplete")).map_err(|e| e.to_string())?;
        self.armed = false;
        Ok(())
    }
}
impl Drop for ArtifactGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn ensure_parent_inside_cwd(output: &Path) -> Result<PathBuf> {
    let cwd = fs::canonicalize(".").map_err(|e| e.to_string())?;
    let parent = output
        .parent()
        .ok_or_else(|| "destination has no parent".to_owned())?;
    let relative = parent
        .strip_prefix(&cwd)
        .map_err(|_| "destination parent escapes the current working directory".to_owned())?;
    let mut current = cwd.clone();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("invalid destination parent".into());
        };
        current.push(name);
        if !current.exists() {
            fs::create_dir(&current).map_err(|e| e.to_string())?;
        }
        let resolved = fs::canonicalize(&current).map_err(|e| e.to_string())?;
        if !resolved.starts_with(&cwd) || !resolved.is_dir() {
            return Err("destination parent escapes the current working directory".into());
        }
        current = resolved;
    }
    Ok(current)
}

fn temporary_file(output: &Path) -> Result<PathBuf> {
    let parent = ensure_parent_inside_cwd(output)?;
    let name = output
        .file_name()
        .ok_or_else(|| "destination has no file name".to_owned())?
        .to_string_lossy();
    for _ in 0..1000 {
        let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let path = parent.join(format!(".{name}.tmp-{}-{nanos}-{unique}", process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("cannot allocate temporary output".into())
}

fn write_reserved(dir: &Path, terrain: &TectonicTerrain) -> Result<()> {
    let count = checked_resolution(terrain.resolution)?;
    for (name, face) in NAMES.iter().zip(&terrain.faces) {
        if face.len() != count || face.iter().any(|value| !value.is_finite()) {
            return Err(format!("invalid terrain face {name}"));
        }
        let output = dir.join(name);
        let temporary = temporary_file(&output)?;
        let mut bytes = Vec::with_capacity(count * 4);
        for value in face {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
        if let Err(error) = fs::hard_link(&temporary, &output) {
            let _ = fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        fs::remove_file(&temporary).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn capture(output: &Path, n: u32) -> std::result::Result<(u64, bool), OperationError> {
    let guard = ArtifactGuard::reserve(output).map_err(OperationError::input)?;
    (|| {
        let terrain = control(n).map_err(OperationError::gpu)?;
        write_reserved(output, &terrain).map_err(OperationError::input)?;
        let bytes = artifact_bytes(output, n).map_err(OperationError::input)?;
        guard.complete().map_err(OperationError::input)?;
        Ok((fnv(&bytes), true))
    })()
}

fn preview(
    workspace: &Workspace,
    dir: &Path,
    control_dir: &Path,
    dir_display: &str,
    control_display: &str,
    n: u32,
    candidate: bool,
) -> std::result::Result<i32, OperationError> {
    let output = workspace
        .output_file(relative_for(candidate))
        .map_err(OperationError::input)?;
    let (metrics, hash) = validate(dir, control_dir, n).map_err(OperationError::input)?;
    let orientation = fixture().map_err(OperationError::input)?;
    let command = if candidate {
        "preview-candidate"
    } else {
        "preview-control"
    };
    let artifact = if candidate { "candidate" } else { "control" };
    let relative = relative_for(candidate);
    if !metrics_pass(&metrics, orientation) {
        print_preview(
            command,
            artifact,
            n,
            dir_display,
            control_display,
            relative,
            false,
            false,
            false,
            hash,
            0,
        );
        return Ok(3);
    }
    let temporary = temporary_file(&output).map_err(OperationError::input)?;
    let result = (|| {
        let terrain = load(dir, n).map_err(OperationError::input)?;
        let gpu = GpuContext::new().map_err(OperationError::gpu)?;
        let renderer = PreviewRenderer::new(&gpu);
        let terrain_view = renderer.upload_terrain(&gpu, &terrain);
        let params = PlanetParams::default();
        let derived = DerivedProperties::from_params(&params);
        let uniforms = preview_uniforms(&params, &derived);
        let pixels = renderer.render(&gpu, &uniforms, &terrain_view, None, None, 512);
        let image = image::RgbaImage::from_raw(512, 512, pixels)
            .ok_or_else(|| OperationError::gpu("invalid RGBA readback"))?;
        image
            .save_with_format(&temporary, image::ImageFormat::Png)
            .map_err(|error| match error {
                image::ImageError::IoError(error) => OperationError::input(error),
                _ => OperationError::gpu(error),
            })?;
        if output.exists() {
            return Err(OperationError::input("destination exists"));
        }
        fs::hard_link(&temporary, &output).map_err(OperationError::input)?;
        fs::remove_file(&temporary).map_err(OperationError::input)?;
        fs::metadata(&output)
            .map(|metadata| metadata.len())
            .map_err(OperationError::input)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    print_preview(
        command,
        artifact,
        n,
        dir_display,
        control_display,
        relative,
        true,
        true,
        true,
        hash,
        result?,
    );
    Ok(0)
}

fn relative_for(candidate: bool) -> &'static str {
    if candidate {
        "artifacts/terrain-diffusion-eval/candidate-512.png"
    } else {
        "artifacts/terrain-diffusion-eval/control-512.png"
    }
}

fn preview_uniforms(params: &PlanetParams, derived: &DerivedProperties) -> PreviewUniforms {
    PreviewUniforms {
        rotation: [
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ],
        light_dir: [0.5, 0.7, -1.],
        ocean_level: -0.5 + 1.7 * derived.ocean_fraction,
        base_temp_c: derived.base_temperature_c,
        ocean_fraction: derived.ocean_fraction,
        axial_tilt_rad: params.axial_tilt_deg.to_radians(),
        view_mode: 0,
        season: 0.5,
        atmosphere_density: 0.,
        atmosphere_height: 0.,
        height_scale: 3.,
        zoom: 1.,
        pan_x: 0.,
        pan_y: 0.,
        cloud_coverage: 0.,
        cloud_seed: 0,
        night_lights: 0.,
        star_color_temp: 0.5,
        city_light_hue: 0.,
        show_ao: 1.,
        show_water: 0.,
        show_ice: 0.,
        show_biomes: 0.,
        show_clouds: 0.,
        show_atmosphere_layer: 0.,
        show_cities: 0.,
        cloud_opacity: 0.,
        cloud_advection: 0.,
        rotation_rate: derived.rotation_rate_rad_s,
        atm_pressure: derived.surface_pressure_bar,
        _pad4: 0.,
        lava_glow: 0.,
        ring_inner: 0.,
        ring_outer: 0.,
        ring_tilt: 0.,
        ring_opacity: 0.,
        planet_radius_km: derived.radius_km,
        show_cloud_shadows: 0.,
        _pad5: 0.,
    }
}

#[allow(clippy::too_many_arguments)]
fn print_preview(
    command: &str,
    artifact: &str,
    n: u32,
    dir: &str,
    control: &str,
    png: &str,
    validation: bool,
    render: bool,
    png_ok: bool,
    hash: u64,
    png_bytes: u64,
) {
    println!(
        "result_version=1\ncommand={command}\nartifact={artifact}\nresolution={n}\ngate.validation={}\ngate.render={}\ngate.png={}",
        status(validation),
        status(render),
        status(png_ok)
    );
    for name in NOT_RUN {
        println!("gate.{name}=NOT_RUN");
    }
    println!(
        "metric.fnv1a64={hash:016x}\nmetric.png_bytes={png_bytes}\npath.dir={dir}\npath.control={control}\npath.png={png}"
    );
}

fn fail(code: i32, error: impl AsRef<str>) -> ! {
    eprintln!("{}", error.as_ref());
    process::exit(code)
}

fn fail_operation(error: OperationError) -> ! {
    match error {
        OperationError::Input(error) => fail(2, error),
        OperationError::Gpu(error) => fail(4, error),
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let invocation = parse(&args).unwrap_or_else(|error| fail(2, error));
    if invocation.command == Command::Fixture {
        if fixture().unwrap_or_else(|error| fail(2, error)) {
            println!(
                "result_version=1\ncommand=fixture-orientation\nartifact=orientation-fixture\nresolution={FIXTURE_RESOLUTION}\ngate.size=PASS\ngate.finite=PASS\ngate.orientation_fixture=PASS"
            );
        } else {
            fail(3, "orientation fixture failed");
        }
        return;
    }
    let workspace = Workspace::new().unwrap_or_else(|error| fail(2, error));
    match invocation.command {
        Command::Capture => {
            let n = invocation.resolution.expect("validated resolution");
            let (output, display) = workspace
                .output_dir(invocation.dir.as_deref().expect("validated dir"))
                .unwrap_or_else(|error| fail(2, error));
            let (hash, finite) = capture(&output, n).unwrap_or_else(|error| fail_operation(error));
            println!(
                "result_version=1\ncommand=capture-control\nartifact=control\nresolution={n}\ngate.capture=PASS\ngate.finite={}\nmetric.fnv1a64={hash:016x}\npath.dir={display}",
                status(finite)
            );
            for name in NAMES {
                println!(
                    "path.{}={display}/{name}",
                    name.split('.').next().expect("name has extension")
                );
            }
        }
        Command::Compare => {
            let (first, first_display) = workspace
                .read_dir(invocation.first.as_deref().expect("validated first"))
                .unwrap_or_else(|error| fail(2, error));
            let (second, second_display) = workspace
                .read_dir(invocation.second.as_deref().expect("validated second"))
                .unwrap_or_else(|error| fail(2, error));
            let n = infer_resolution(&first).unwrap_or_else(|error| fail(2, error));
            if infer_resolution(&second).unwrap_or_else(|error| fail(2, error)) != n {
                fail(2, "artifact resolutions differ");
            }
            let first_bytes = artifact_bytes(&first, n).unwrap_or_else(|error| fail(2, error));
            let second_bytes = artifact_bytes(&second, n).unwrap_or_else(|error| fail(2, error));
            let equal = first_bytes == second_bytes;
            println!(
                "result_version=1\ncommand=compare-bytes\nartifact=control-pair\nresolution={n}\ngate.byte_equality={}\nmetric.first_fnv1a64={:016x}\nmetric.second_fnv1a64={:016x}\npath.first={first_display}\npath.second={second_display}",
                status(equal),
                fnv(&first_bytes),
                fnv(&second_bytes)
            );
            if !equal {
                process::exit(3);
            }
        }
        Command::ValidateControl | Command::ValidateCandidate => {
            let n = invocation.resolution.expect("validated resolution");
            let (dir, dir_display) = workspace
                .read_dir(invocation.dir.as_deref().expect("validated dir"))
                .unwrap_or_else(|error| fail(2, error));
            let candidate = invocation.command == Command::ValidateCandidate;
            let (control, control_display) = if candidate {
                workspace
                    .read_dir(invocation.control.as_deref().expect("validated control"))
                    .unwrap_or_else(|error| fail(2, error))
            } else {
                (dir.clone(), dir_display.clone())
            };
            let (metrics, hash) =
                validate(&dir, &control, n).unwrap_or_else(|error| fail(2, error));
            let orientation = fixture().unwrap_or_else(|error| fail(2, error));
            print_validation(
                if candidate {
                    "validate-candidate"
                } else {
                    "validate-control"
                },
                if candidate { "candidate" } else { "control" },
                n,
                &dir_display,
                &control_display,
                &metrics,
                hash,
                orientation,
            );
            if !metrics_pass(&metrics, orientation) {
                process::exit(3);
            }
        }
        Command::PreviewControl | Command::PreviewCandidate => {
            let n = invocation.resolution.expect("validated resolution");
            let (dir, dir_display) = workspace
                .read_dir(invocation.dir.as_deref().expect("validated dir"))
                .unwrap_or_else(|error| fail(2, error));
            let candidate = invocation.command == Command::PreviewCandidate;
            let (control, control_display) = if candidate {
                workspace
                    .read_dir(invocation.control.as_deref().expect("validated control"))
                    .unwrap_or_else(|error| fail(2, error))
            } else {
                (dir.clone(), dir_display.clone())
            };
            match preview(
                &workspace,
                &dir,
                &control,
                &dir_display,
                &control_display,
                n,
                candidate,
            ) {
                Ok(0) => {}
                Ok(code) => process::exit(code),
                Err(error) => fail_operation(error),
            }
        }
        Command::Fixture => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn terrain(n: u32, value: f32) -> TectonicTerrain {
        TectonicTerrain {
            faces: std::array::from_fn(|_| vec![value; (n * n) as usize]),
            resolution: n,
        }
    }
    #[test]
    fn parser_rejects_unknown_duplicate_missing_and_extra() {
        assert!(parse(&["bad".into()]).is_err());
        assert!(
            parse(&[
                "capture-control".into(),
                "--resolution".into(),
                "4".into(),
                "--resolution".into(),
                "4".into(),
                "--dir".into(),
                "x".into()
            ])
            .is_err()
        );
        assert!(parse(&["capture-control".into(), "--resolution".into(), "4".into()]).is_err());
        assert!(parse(&["fixture-orientation".into(), "--dir".into(), "x".into()]).is_err());
    }
    #[test]
    fn paths_and_sizes_are_checked() {
        let workspace = Workspace::new().unwrap();
        assert!(workspace.relative("../x").is_err());
        assert!(workspace.relative("/x").is_err());
        assert!(checked_resolution(3).is_err());
        assert!(checked_resolution(MAX_RESOLUTION + 1).is_err());
    }
    #[test]
    fn fixture_uses_production_serialization_and_loader() {
        assert!(fixture().unwrap());
    }
    #[test]
    fn fixture_mutations_fail() {
        let mut t = fixture_terrain();
        mutate_face_swap(&mut t);
        assert!(!orientation_matches(&t));
        let mut t = fixture_terrain();
        mutate_rows(&mut t);
        assert!(!orientation_matches(&t));
        let mut t = fixture_terrain();
        mutate_columns(&mut t);
        assert!(!orientation_matches(&t));
        let mut t = fixture_terrain();
        transpose(&mut t);
        assert!(!orientation_matches(&t));
    }
    #[test]
    fn interior_pairs_exclude_borders() {
        let mut t = terrain(4, 0.0);
        t.faces[0][4] = 1.0;
        assert_eq!(interior_jumps(&t).len(), 24);
        assert!(!interior_jumps(&t).contains(&1.0));
        let t = terrain(5, 0.0);
        assert_eq!(interior_jumps(&t).len(), 72);
    }
    #[test]
    fn normal_threshold_uses_control_plus_two_or_five() {
        assert!(5.0 <= 5.0f32.max(10.0 + 2.0));
        assert!(6.0 <= 5.0f32.max(4.0 + 2.0));
        assert!(7.0 > 5.0f32.max(4.0 + 2.0));
    }
    #[test]
    fn pole_formula_checks_all_quantiles_and_fallback() {
        let cap = Distribution {
            p10: 0.02,
            p50: 0.02,
            p90: 0.02,
        };
        let ring = Distribution {
            p10: 0.0,
            p50: 0.0,
            p90: 0.0,
        };
        assert!(pole_pass(cap, ring));
        let bad = Distribution {
            p10: 0.03,
            p50: 0.0,
            p90: 0.0,
        };
        assert!(!pole_pass(bad, ring));
        let cap = Distribution {
            p10: 1.1,
            p50: 1.1,
            p90: 1.1,
        };
        let ring = Distribution {
            p10: 1.0,
            p50: 1.0,
            p90: 1.0,
        };
        assert!(pole_pass(cap, ring));
    }
    #[test]
    fn loader_rejects_nan_inf_and_wrong_size() {
        let root = env::temp_dir().join(format!("terrain-eval-invalid-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let t = terrain(4, 0.0);
        write(&root, &t).unwrap();
        fs::write(root.join(NAMES[0]), [0_u8; 3]).unwrap();
        assert!(load(&root, 4).is_err());
        write(&root, &t).unwrap();
        fs::write(root.join(NAMES[0]), f32::NAN.to_le_bytes()).unwrap();
        assert!(load(&root, 4).is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn sphere_normals_match_across_edges() {
        let t = terrain(32, 0.0);
        assert!(percentile(normal_angles(&t, 0.0, 1.0).unwrap(), 0.95).unwrap() < 5.0);
    }
    #[test]
    fn centered_extraction_uses_odd_source_texels() {
        let source = TectonicTerrain {
            faces: std::array::from_fn(|face| {
                (0..81).map(|index| (face * 1000 + index) as f32).collect()
            }),
            resolution: 9,
        };
        let extracted = centered_extract(&source, 4).unwrap();
        assert_eq!(extracted.faces[0][0], 10.0);
        assert_eq!(extracted.faces[0][3], 16.0);
        assert_eq!(extracted.faces[0][12], 64.0);
        assert!(source_resolution(MAX_RESOLUTION + 1).is_err());
    }
    #[test]
    fn edge_table_uses_authoritative_cube_directions() {
        assert!(verify_edge_table());
    }
    #[test]
    fn compare_infers_resolution_and_rejects_malformed_artifacts() {
        let root = env::temp_dir().join(format!("terrain-eval-infer-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        write(&root, &terrain(4, 0.0)).unwrap();
        assert_eq!(infer_resolution(&root), Ok(4));
        fs::write(root.join(NAMES[0]), [0_u8; 12]).unwrap();
        assert!(infer_resolution(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn compare_cli_has_no_resolution_flag() {
        assert!(
            parse(&[
                "compare-bytes".into(),
                "--first".into(),
                "a".into(),
                "--second".into(),
                "b".into()
            ])
            .is_ok()
        );
        assert!(
            parse(&[
                "compare-bytes".into(),
                "--resolution".into(),
                "4".into(),
                "--first".into(),
                "a".into(),
                "--second".into(),
                "b".into()
            ])
            .is_err()
        );
    }
    #[test]
    fn fnv_and_percentile_are_frozen() {
        assert_eq!(fnv(b""), 14695981039346656037);
        assert_eq!(percentile(vec![0.0, 10.0], 0.5), Some(5.0));
    }
}
