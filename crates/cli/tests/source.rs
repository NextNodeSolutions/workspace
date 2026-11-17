use std::process::Command;

#[test]
fn committed_source_contract() {
    let output = Command::new("python3")
        .args([
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            "tests",
            "-p",
            "test_source_*.py",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("WORKSPACE_TEST_BINARY", env!("CARGO_BIN_EXE_workspace"))
        .output()
        .expect("Python must be installed for the committed-source fixture suite");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
