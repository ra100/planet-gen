use planet_gen::{
    terrain_artifact::{
        CANONICAL_FACE_NAMES, CliMode, KNOWN_NO_GO_CONTROL_FNV, KNOWN_NO_GO_VALIDATOR_COMMIT,
        TerrainArtifactError, TerrainSource, export_refusal, fnv1a64, load_approved_terrain,
        parse_approval, parse_cli_args,
    },
    terrain_compute::TectonicTerrain,
};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

static UNIQUE: AtomicU64 = AtomicU64::new(0);

struct Root(PathBuf);

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn root() -> Root {
    let id = UNIQUE.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        ".terrain-artifact-test-{}-{}-{id}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    );
    fs::create_dir(&name).unwrap();
    Root(PathBuf::from(name))
}

fn approval(artifact: &Path, evidence: &Path, control: u64, validator: &str) -> Vec<u8> {
    let face_hashes: Vec<_> = CANONICAL_FACE_NAMES
        .iter()
        .map(|name| fnv1a64(&fs::read(artifact.join(name)).unwrap()))
        .collect();
    let candidate = CANONICAL_FACE_NAMES
        .iter()
        .fold(14695981039346656037, |hash, name| {
            fs::read(artifact.join(name))
                .unwrap()
                .iter()
                .fold(hash, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
                })
        });
    let artifact_path = artifact.to_str().unwrap().replace('\\', "/");
    let evidence_path = evidence.to_str().unwrap().replace('\\', "/");
    format!(
        "approval_version=1\nartifact_schema_version=1\nartifact_path={artifact_path}\nresolution=512\nface.posx.fnv1a64={:016x}\nface.negx.fnv1a64={:016x}\nface.posy.fnv1a64={:016x}\nface.negy.fnv1a64={:016x}\nface.posz.fnv1a64={:016x}\nface.negz.fnv1a64={:016x}\ncandidate_fnv1a64={candidate:016x}\ncontrol_fnv1a64={control:016x}\nvalidator_commit={validator}\nevidence.path={evidence_path}\nevidence.fnv1a64={:016x}\ngate.artifact=PASS\ngate.control=PASS\ngate.byte_equality=PASS\ngate.orientation=PASS\ngate.seams=PASS\ngate.normals=PASS\ngate.poles=PASS\ngate.provenance=PASS\ngate.rights=PASS\ngate.resource_capture=PASS\ngate.human_review=PASS\nunits.height=normalized_control_range\nunits.horizontal=unit_sphere\nsea_level=control_ocean_level\ncontrol_ocean_level=0\nexport_policy=REFUSE_IMPORTED\nstatus=APPROVED\n",
        face_hashes[0], face_hashes[1], face_hashes[2], face_hashes[3], face_hashes[4], face_hashes[5], fnv1a64(&fs::read(evidence).unwrap()),
    ).into_bytes()
}

fn fixture() -> (Root, PathBuf, PathBuf) {
    let root = root();
    let artifact = root.0.join("artifact");
    let evidence = root.0.join("evidence.txt");
    fs::create_dir(&artifact).unwrap();
    let face = vec![0_u8; 512 * 512 * 4];
    for name in CANONICAL_FACE_NAMES {
        fs::write(artifact.join(name), &face).unwrap();
    }
    fs::write(&evidence, "evidence\n").unwrap();
    (root, artifact, evidence)
}

fn replace_value(bytes: &[u8], key: &str, replacement: &str) -> Vec<u8> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            line.strip_prefix(&format!("{key}="))
                .map_or_else(|| line.to_owned(), |_| format!("{key}={replacement}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
        .into_iter()
        .chain(std::iter::once(b'\n'))
        .collect()
}

#[test]
fn approval_and_loader_accept_exact_synthetic_artifact() {
    let (root, artifact, evidence) = fixture();
    let approval_path = root.0.join("approval.txt");
    fs::write(
        &approval_path,
        approval(
            &artifact,
            &evidence,
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    )
    .unwrap();
    let loaded = load_approved_terrain(&artifact, &approval_path).unwrap();
    assert_eq!(loaded.terrain.resolution, 512);
    assert_eq!(loaded.terrain.faces[0].len(), 512 * 512);
    assert_eq!(loaded.approval.control_ocean_level, 0.0);
}

#[test]
fn parser_and_loader_reject_schema_layout_and_no_go_values() {
    let (root, artifact, evidence) = fixture();
    let valid = approval(
        &artifact,
        &evidence,
        1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    assert!(parse_approval(&valid).is_ok());
    assert_eq!(
        parse_approval(b"approval_version=1\n")
            .unwrap_err()
            .to_string(),
        "invalid terrain approval schema"
    );
    fs::write(artifact.join("extra"), []).unwrap();
    let approval_path = root.0.join("approval.txt");
    fs::write(&approval_path, &valid).unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Schema)
    ));
    fs::remove_file(artifact.join("extra")).unwrap();
    fs::write(
        &approval_path,
        approval(
            &artifact,
            &evidence,
            KNOWN_NO_GO_CONTROL_FNV,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    )
    .unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Identity)
    ));
    fs::write(
        &approval_path,
        approval(&artifact, &evidence, 1, KNOWN_NO_GO_VALIDATOR_COMMIT),
    )
    .unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Identity)
    ));
}

#[test]
fn parser_rejects_all_frozen_schema_failures() {
    let (_root, artifact, evidence) = fixture();
    let valid = approval(
        &artifact,
        &evidence,
        1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    for key in [
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
    ] {
        for value in ["FAIL", "NOT_RUN"] {
            assert!(matches!(
                parse_approval(&replace_value(&valid, key, value)),
                Err(TerrainArtifactError::Approval)
            ));
        }
    }
    for (key, value) in [
        ("status", "REJECTED"),
        ("status", "NOT_RUN"),
        (
            "validator_commit",
            "gaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        (
            "validator_commit",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        ("candidate_fnv1a64", "gggggggggggggggg"),
        ("candidate_fnv1a64", "aaaaaaaaaaaaaaa"),
        ("candidate_fnv1a64", "243e1887675e77a8"),
        ("control_ocean_level", "NaN"),
        ("control_ocean_level", "1.3"),
        ("control_ocean_level", "0.0"),
    ] {
        assert!(parse_approval(&replace_value(&valid, key, value)).is_err());
    }
    let duplicated = format!(
        "{}approval_version=1\n",
        String::from_utf8(valid.clone()).unwrap()
    )
    .into_bytes();
    assert!(parse_approval(&duplicated).is_err());
    let unknown = String::from_utf8(valid)
        .unwrap()
        .replacen("approval_version=1", "unknown=x", 1)
        .into_bytes();
    assert!(parse_approval(&unknown).is_err());
    let missing = String::from_utf8(unknown)
        .unwrap()
        .replacen("export_policy=REFUSE_IMPORTED\n", "", 1)
        .into_bytes();
    assert!(parse_approval(&missing).is_err());
}

#[test]
fn control_ocean_level_uses_canonical_finite_boundaries() {
    let (_root, artifact, evidence) = fixture();
    let valid = approval(
        &artifact,
        &evidence,
        1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    for value in ["-0", "-0.5", "1.2"] {
        assert!(parse_approval(&replace_value(&valid, "control_ocean_level", value)).is_ok());
    }
    for value in ["Inf", "-Inf", "NaN", "0.0", "0e0", "1.20", "-0.0"] {
        assert!(parse_approval(&replace_value(&valid, "control_ocean_level", value)).is_err());
    }
}

#[test]
fn loader_rejects_fail_closed_artifact_and_evidence_changes() {
    let (root, artifact, evidence) = fixture();
    let approval_path = root.0.join("approval.txt");
    let valid = approval(
        &artifact,
        &evidence,
        1,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    fs::write(&approval_path, &valid).unwrap();
    fs::write(artifact.join(".incomplete"), []).unwrap();
    assert!(load_approved_terrain(&artifact, &approval_path).is_err());
    fs::remove_file(artifact.join(".incomplete")).unwrap();
    let mut finite = fs::read(artifact.join(CANONICAL_FACE_NAMES[0])).unwrap();
    finite[..4].copy_from_slice(&1.0f32.to_le_bytes());
    fs::write(artifact.join(CANONICAL_FACE_NAMES[0]), &finite).unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Identity)
    ));
    fs::write(
        artifact.join(CANONICAL_FACE_NAMES[0]),
        vec![0; 512 * 512 * 4],
    )
    .unwrap();
    let mut non_finite = fs::read(artifact.join(CANONICAL_FACE_NAMES[0])).unwrap();
    for value in [f32::NAN, f32::INFINITY] {
        non_finite[..4].copy_from_slice(&value.to_le_bytes());
        fs::write(artifact.join(CANONICAL_FACE_NAMES[0]), &non_finite).unwrap();
        assert!(matches!(
            load_approved_terrain(&artifact, &approval_path),
            Err(TerrainArtifactError::NonFinite)
        ));
    }
    fs::write(artifact.join(CANONICAL_FACE_NAMES[0]), vec![0; 4]).unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Size)
    ));
    fs::write(
        artifact.join(CANONICAL_FACE_NAMES[0]),
        vec![0; 512 * 512 * 4],
    )
    .unwrap();
    fs::write(
        &approval_path,
        approval(
            &artifact,
            &evidence,
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    )
    .unwrap();
    fs::write(&evidence, "changed\n").unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Evidence)
    ));
}

#[cfg(unix)]
#[test]
fn loader_rejects_symlinked_inputs_and_unsafe_paths() {
    use std::os::unix::fs::symlink;

    let (root, artifact, evidence) = fixture();
    let approval_path = root.0.join("approval.txt");
    fs::write(
        &approval_path,
        approval(
            &artifact,
            &evidence,
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    )
    .unwrap();
    let artifact_link = root.0.join("artifact-link");
    symlink("artifact", &artifact_link).unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact_link, &approval_path),
        Err(TerrainArtifactError::Path)
    ));
    let approval_link = root.0.join("approval-link.txt");
    symlink("approval.txt", &approval_link).unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_link),
        Err(TerrainArtifactError::Path)
    ));
    assert!(matches!(
        load_approved_terrain(Path::new("/tmp"), &approval_path),
        Err(TerrainArtifactError::Path)
    ));
    assert!(matches!(
        load_approved_terrain(Path::new("../artifact"), &approval_path),
        Err(TerrainArtifactError::Path)
    ));
    let evidence_real = root.0.join("evidence-real.txt");
    fs::rename(&evidence, &evidence_real).unwrap();
    symlink("evidence-real.txt", &evidence).unwrap();
    fs::write(
        &approval_path,
        approval(
            &artifact,
            &evidence,
            1,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
    )
    .unwrap();
    assert!(matches!(
        load_approved_terrain(&artifact, &approval_path),
        Err(TerrainArtifactError::Path)
    ));
}

#[test]
fn cli_and_source_policies_are_pure() {
    assert!(matches!(
        parse_cli_args(Vec::<OsString>::new()),
        Ok(CliMode::Procedural)
    ));
    assert!(matches!(
        parse_cli_args([
            "--terrain-approval".into(),
            "a".into(),
            "--terrain-artifact".into(),
            "d".into()
        ]),
        Ok(CliMode::Import { .. })
    ));
    for args in [
        vec!["--unknown", "x"],
        vec!["--terrain-artifact", "d"],
        vec!["--terrain-artifact", "--terrain-approval", "a"],
        vec!["--terrain-approval", "--terrain-artifact", "d"],
        vec![
            "--terrain-artifact",
            "--review-example",
            "--terrain-approval",
            "a",
        ],
        vec!["d", "x"],
        vec![
            "--terrain-artifact",
            "d",
            "--terrain-artifact",
            "e",
            "--terrain-approval",
            "a",
        ],
    ] {
        assert!(parse_cli_args(args.into_iter().map(OsString::from)).is_err());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        assert!(matches!(
            parse_cli_args([
                OsString::from("--terrain-artifact"),
                OsString::from_vec(vec![0xff]),
                OsString::from("--terrain-approval"),
                OsString::from("a")
            ]),
            Ok(CliMode::Import { .. })
        ));
    }

    let terrain = Arc::new(TectonicTerrain {
        faces: std::array::from_fn(|_| vec![0.0]),
        resolution: 1,
    });
    let imported = TerrainSource::Imported {
        terrain: Arc::clone(&terrain),
        control_ocean_level: 0.0,
    };
    assert!(Arc::ptr_eq(
        &terrain,
        match &imported {
            TerrainSource::Imported { terrain: value, .. } => value,
            TerrainSource::Procedural => unreachable!(),
        }
    ));
    assert_eq!(
        export_refusal(&imported),
        Some("Export is unavailable for imported terrain artifacts.")
    );
    assert_eq!(export_refusal(&TerrainSource::Procedural), None);
}
