use std::process::Command;

#[test]
fn fixture_protocol_stdout_is_byte_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_terrain_diffusion_eval"))
        .arg("fixture-orientation")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"result_version=1\ncommand=fixture-orientation\nartifact=orientation-fixture\nresolution=17\ngate.size=PASS\ngate.finite=PASS\ngate.orientation_fixture=PASS\n");
}
