use std::{
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static UNIQUE: AtomicU64 = AtomicU64::new(0);
const FACES: [&str; 6] = [
    "posx.f32le",
    "negx.f32le",
    "posy.f32le",
    "negy.f32le",
    "posz.f32le",
    "negz.f32le",
];

struct TempRoot(PathBuf);
impl Deref for TempRoot {
    type Target = Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root() -> TempRoot {
    let unique = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "terrain-diffusion-cli-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    TempRoot(root)
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_terrain_diffusion_eval"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn cube_to_sphere(face: usize, u: f32, v: f32) -> [f32; 3] {
    let s = 2.0 * u - 1.0;
    let t = 2.0 * v - 1.0;
    let point = match face {
        0 => [1.0, -t, -s],
        1 => [-1.0, -t, s],
        2 => [s, 1.0, t],
        3 => [s, -1.0, -t],
        4 => [s, -t, 1.0],
        _ => [-s, -t, -1.0],
    };
    let length = (point[0] * point[0] + point[1] * point[1] + point[2] * point[2]).sqrt();
    [point[0] / length, point[1] / length, point[2] / length]
}

fn write_analytic_artifact(root: &Path, name: &str, n: usize) {
    let dir = root.join(name);
    fs::create_dir(&dir).unwrap();
    for (face, file) in FACES.into_iter().enumerate() {
        let mut bytes = Vec::with_capacity(n * n * 4);
        for y in 0..n {
            for x in 0..n {
                let direction = cube_to_sphere(
                    face,
                    (x as f32 + 0.5) / n as f32,
                    (y as f32 + 0.5) / n as f32,
                );
                let value = (20.0 * (direction[0] + direction[1] + direction[2])).sin();
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        fs::write(dir.join(file), bytes).unwrap();
    }
}

fn write_artifact(root: &Path, name: &str, bytes: &[u8]) {
    let dir = root.join(name);
    fs::create_dir(&dir).unwrap();
    for face in FACES {
        fs::write(dir.join(face), bytes).unwrap();
    }
}

#[test]
fn fixture_protocol_is_exact_and_unknown_or_duplicate_flags_exit_two() {
    let root = root();
    let output = run(&root, &["fixture-orientation"]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "result_version=1\ncommand=fixture-orientation\nartifact=orientation-fixture\nresolution=17\ngate.size=PASS\ngate.finite=PASS\ngate.orientation_fixture=PASS\n"
    );
    assert_eq!(run(&root, &["unknown"]).status.code(), Some(2));
    assert_eq!(
        run(
            &root,
            &[
                "capture-control",
                "--resolution",
                "4",
                "--resolution",
                "4",
                "--dir",
                "x"
            ]
        )
        .status
        .code(),
        Some(2)
    );
}

#[test]
fn compare_rejects_identical_malformed_artifacts() {
    let root = root();
    write_artifact(&root, "first", &[0; 3]);
    write_artifact(&root, "second", &[0; 3]);
    assert_eq!(
        run(
            &root,
            &["compare-bytes", "--first", "first", "--second", "second"]
        )
        .status
        .code(),
        Some(2)
    );
}

#[test]
fn capture_collision_and_path_io_exit_two() {
    let root = root();
    fs::create_dir(root.join("taken")).unwrap();
    assert_eq!(
        run(
            &root,
            &["capture-control", "--resolution", "4", "--dir", "taken"]
        )
        .status
        .code(),
        Some(2)
    );
    fs::write(root.join("file"), []).unwrap();
    assert_eq!(
        run(
            &root,
            &[
                "capture-control",
                "--resolution",
                "4",
                "--dir",
                "file/child"
            ]
        )
        .status
        .code(),
        Some(2)
    );
}

#[test]
fn preview_collision_exits_two_before_validation_or_gpu() {
    let root = root();
    fs::create_dir(root.join("input")).unwrap();
    fs::create_dir_all(root.join("artifacts/terrain-diffusion-eval")).unwrap();
    fs::write(
        root.join("artifacts/terrain-diffusion-eval/control-512.png"),
        [],
    )
    .unwrap();
    let output = run(
        &root,
        &["preview-control", "--resolution", "4", "--dir", "input"],
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn incomplete_artifacts_are_rejected() {
    let root = root();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    write_artifact(&root, "incomplete", &bytes.repeat(16));
    fs::write(root.join("incomplete/.incomplete"), []).unwrap();
    assert_eq!(
        run(
            &root,
            &[
                "compare-bytes",
                "--first",
                "incomplete",
                "--second",
                "incomplete"
            ]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(
        run(
            &root,
            &[
                "validate-control",
                "--resolution",
                "4",
                "--dir",
                "incomplete"
            ]
        )
        .status
        .code(),
        Some(2)
    );
}

#[test]
fn concurrent_capture_reserves_one_complete_destination() {
    let root = root();
    let binary = env!("CARGO_BIN_EXE_terrain_diffusion_eval");
    let first = Command::new(binary)
        .current_dir(&*root)
        .args(["capture-control", "--resolution", "4", "--dir", "shared"])
        .spawn()
        .unwrap();
    let second = Command::new(binary)
        .current_dir(&*root)
        .args(["capture-control", "--resolution", "4", "--dir", "shared"])
        .spawn()
        .unwrap();
    let codes = [
        first.wait_with_output().unwrap().status.code(),
        second.wait_with_output().unwrap().status.code(),
    ];
    assert_eq!(codes.iter().filter(|code| **code == Some(0)).count(), 1);
    assert_eq!(codes.iter().filter(|code| **code == Some(2)).count(), 1);
    assert!(!root.join("shared/.incomplete").exists());
    assert_eq!(
        run(
            &root,
            &["compare-bytes", "--first", "shared", "--second", "shared"]
        )
        .status
        .code(),
        Some(0)
    );
}

#[test]
fn first_preview_creates_parent_and_publishes_png() {
    let root = root();
    write_analytic_artifact(&root, "analytic", 512);
    let validation = run(
        &root,
        &[
            "validate-control",
            "--resolution",
            "512",
            "--dir",
            "analytic",
        ],
    );
    assert!(
        validation.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&validation.stdout),
        String::from_utf8_lossy(&validation.stderr)
    );
    let preview = run(
        &root,
        &[
            "preview-control",
            "--resolution",
            "512",
            "--dir",
            "analytic",
        ],
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let png = root.join("artifacts/terrain-diffusion-eval/control-512.png");
    assert_eq!(image::image_dimensions(png).unwrap(), (512, 512));
}

#[test]
fn validation_failure_exits_three() {
    let root = root();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    write_artifact(&root, "flat", &bytes.repeat(16));
    assert_eq!(
        run(
            &root,
            &["validate-control", "--resolution", "4", "--dir", "flat"]
        )
        .status
        .code(),
        Some(3)
    );
}
