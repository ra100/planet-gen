use std::process::Command;

#[test]
fn fixture_protocol_stdout_is_byte_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_procedural_terrain_eval"))
        .arg("fixture-orientation")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"result_version=1\ncommand=fixture-orientation\nartifact=orientation-fixture\nresolution=17\ngate.size=PASS\ngate.finite=PASS\ngate.orientation_fixture=PASS\n");
}

#[test]
fn malformed_cubemap_rejects_with_named_gate() {
    let root = std::env::temp_dir().join(format!(
        "procedural-terrain-malformed-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir(&root).unwrap();
    let artifact = root.join("malformed");
    std::fs::create_dir(&artifact).unwrap();
    for face in [
        "posx.f32le",
        "negx.f32le",
        "posy.f32le",
        "negy.f32le",
        "posz.f32le",
        "negz.f32le",
    ] {
        std::fs::write(artifact.join(face), [0_u8; 3]).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_procedural_terrain_eval"))
        .current_dir(&root)
        .args([
            "validate-control",
            "--resolution",
            "4",
            "--dir",
            "malformed",
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(root);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("gate.cubemap=FAIL")
    );
}
