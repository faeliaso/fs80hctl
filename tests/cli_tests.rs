use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fs80hctl"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn test_cli_help() {
    let output = bin().arg("--help").output().expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fs80hctl"));
    assert!(stdout.contains("devices"));
    assert!(stdout.contains("capture"));
    assert!(stdout.contains("verify"));
    assert!(stdout.contains("doctor"));
}

#[test]
fn test_cli_doctor_text() {
    let output = bin().arg("doctor").output().expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Diagnóstico do fs80hctl"));
    assert!(stdout.contains("usb_subsystem"));
    assert!(stdout.contains("filesystem_write"));
}

#[test]
fn test_cli_doctor_json() {
    let output = bin()
        .args(["--json", "doctor"])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid json output");
    assert!(parsed.get("healthy").is_some());
    assert!(parsed.get("checks").unwrap().is_array());
    assert!(parsed.get("devices_found").is_some());
}

#[test]
fn test_cli_verify_match() {
    let output = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "40",
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("atinge limiar"));
    assert!(stdout.contains("Aviso: resultado é uma similaridade técnica"));
}

#[test]
fn test_cli_verify_mismatch_without_exit_code() {
    let output = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_mismatch.png").to_str().unwrap(),
            "--threshold",
            "40",
        ])
        .output()
        .expect("failed to execute");
    // Por padrão (sem --exit-code), retorna 0 mesmo não atingindo o limiar
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("abaixo do limiar"));
}

#[test]
fn test_cli_verify_mismatch_with_exit_code() {
    let output = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_mismatch.png").to_str().unwrap(),
            "--threshold",
            "40",
            "--exit-code",
        ])
        .output()
        .expect("failed to execute");
    // Com --exit-code, falha com status code 1 quando não atinge o limiar
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("abaixo do limiar"));
}

#[test]
fn test_cli_verify_json_output() {
    let output = bin()
        .args([
            "--json",
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid json output");
    assert_eq!(parsed["threshold_met"], true);
    assert!(parsed["deterministic"]["score"].as_f64().unwrap() >= 40.0);
}

#[test]
fn test_cli_verify_invalid_threshold() {
    let output = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "150",
        ])
        .output()
        .expect("failed to execute");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--threshold deve estar entre 0 e 100"));
}

#[test]
fn test_cli_device_inspect_mismatched_bus_address() {
    let output = bin()
        .args(["device", "inspect", "--bus", "1"])
        .output()
        .expect("failed to execute");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--bus e --address devem ser usados juntos"));
}
