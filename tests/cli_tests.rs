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
    assert!(stdout.contains("manual"));
    assert!(stdout.contains("List connected"));
    assert!(stdout.contains("Capture an optical"));
}

#[test]
fn test_cli_version() {
    let output_long = bin().arg("--version").output().expect("failed to execute");
    assert!(output_long.status.success());
    let stdout_long = String::from_utf8_lossy(&output_long.stdout);
    assert!(stdout_long.contains("fs80hctl 0.1.0"));

    let output_short = bin().arg("-V").output().expect("failed to execute");
    assert!(output_short.status.success());
    let stdout_short = String::from_utf8_lossy(&output_short.stdout);
    assert!(stdout_short.contains("fs80hctl 0.1.0"));
}

#[test]
fn test_cli_manual() {
    let output = bin().arg("manual").output().expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fs80hctl(1) - Futronic"));
    assert!(stdout.contains("SYNOPSIS"));
    assert!(stdout.contains("SUBCOMMANDS"));
    assert!(stdout.contains("SUPPORTED FORMATS"));

    let json_output = bin()
        .args(["--json", "manual"])
        .output()
        .expect("failed to execute");
    assert!(json_output.status.success());
    let parsed: serde_json::Value =
        serde_json::from_slice(&json_output.stdout).expect("valid json output");
    assert!(parsed.get("manual").is_some());
    assert!(parsed["manual"].as_str().unwrap().contains("SYNOPSIS"));
}

#[test]
fn test_cli_doctor_text() {
    let output = bin().arg("doctor").output().expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fs80hctl Diagnostics"));
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
    assert!(stdout.contains("meets threshold"));
    assert!(stdout.contains("Warning: result is a technical similarity indicator"));
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
    // Defaults to returning 0 when threshold is not met (without --exit-code)
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("below threshold"));
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
    // With --exit-code, exits with status code 1 when threshold is not met
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("below threshold"));
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
    assert!(stderr.contains("--threshold must be between 0 and 100"));
}

#[test]
fn test_cli_device_inspect_mismatched_bus_address() {
    let output = bin()
        .args(["device", "inspect", "--bus", "1"])
        .output()
        .expect("failed to execute");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--bus and --address must be used together"));
}

#[test]
fn test_cli_convert_and_inspect() {
    let tmp_dir = std::env::temp_dir().join(format!("fs80h-cli-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);

    let tiff_out = tmp_dir.join("probe.tiff");
    let pgm_out = tmp_dir.join("probe.pgm");
    let raw_out = tmp_dir.join("probe.raw");

    // 1. Convert PNG -> TIFF
    let out = bin()
        .args([
            "convert",
            fixture("probe.png").to_str().unwrap(),
            "--to",
            "tiff",
            "--output",
            tiff_out.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute convert to tiff");
    assert!(out.status.success());
    assert!(tiff_out.exists());
    assert!(tmp_dir.join("probe.tiff.meta.json").exists());

    // 2. Convert TIFF -> PGM
    let out = bin()
        .args([
            "convert",
            tiff_out.to_str().unwrap(),
            "--to",
            "pgm",
            "--output",
            pgm_out.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute convert to pgm");
    assert!(out.status.success());
    assert!(pgm_out.exists());

    // 3. Convert PGM -> RAW
    let out = bin()
        .args([
            "convert",
            pgm_out.to_str().unwrap(),
            "--to",
            "raw",
            "--output",
            raw_out.to_str().unwrap(),
        ])
        .output()
        .expect("failed to execute convert to raw");
    assert!(out.status.success());
    assert!(raw_out.exists());
    assert_eq!(std::fs::metadata(&raw_out).unwrap().len(), 153600);

    // 4. Inspect TIFF text
    let out = bin()
        .args(["inspect", tiff_out.to_str().unwrap()])
        .output()
        .expect("failed to inspect tiff");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Detected format: tiff"));
    assert!(stdout.contains("320x480"));

    // 5. Inspect PGM JSON
    let out = bin()
        .args(["--json", "inspect", pgm_out.to_str().unwrap()])
        .output()
        .expect("failed to inspect pgm json");
    assert!(out.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert_eq!(parsed["format"], "pgm");
    assert_eq!(parsed["dimensions"]["width"], 320);
    assert_eq!(parsed["dimensions"]["height"], 480);
    assert_eq!(parsed["dimensions"]["bytes"], 153600);

    // 6. Cross-format verify: TIFF probe vs PNG reference
    let out = bin()
        .args([
            "verify",
            tiff_out.to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "40",
        ])
        .output()
        .expect("failed to cross verify");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("meets threshold"));

    let _ = std::fs::remove_dir_all(&tmp_dir);
}
