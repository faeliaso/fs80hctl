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
    let output = bin()
        .args(["--mock", "match", "doctor"])
        .output()
        .expect("failed to execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fs80hctl Diagnostics"));
    assert!(stdout.contains("usb_subsystem"));
    assert!(stdout.contains("filesystem_write"));
}

#[test]
fn test_cli_doctor_json() {
    let output = bin()
        .args(["--mock", "match", "--json", "doctor"])
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
    // With --exit-code, exits with status code 2 (Biometric Mismatch) when threshold is not met
    assert_eq!(output.status.code(), Some(2));
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

#[test]
fn test_cli_extended_format_conversion_and_verification() {
    let tmp_dir = tempfile::tempdir().expect("temporary directory");
    let formats = [
        ("wsq", "wsq"),
        ("jp2", "jp2"),
        ("jpeg", "jpeg"),
        ("bmp", "bmp"),
    ];

    for (format, extension) in formats {
        let output = tmp_dir.path().join(format!("probe.{extension}"));
        let converted = bin()
            .args([
                "convert",
                fixture("probe.png").to_str().unwrap(),
                "--to",
                format,
                "--output",
                output.to_str().unwrap(),
            ])
            .output()
            .expect("convert extended format");
        assert!(
            converted.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&converted.stderr)
        );

        let inspected = bin()
            .args(["--json", "inspect", output.to_str().unwrap()])
            .output()
            .expect("inspect extended format");
        assert!(inspected.status.success());
        let value: serde_json::Value = serde_json::from_slice(&inspected.stdout).unwrap();
        assert_eq!(
            value["format"],
            if format == "jpeg" { "jpeg" } else { format }
        );

        let verified = bin()
            .args([
                "verify",
                output.to_str().unwrap(),
                fixture("reference_match.png").to_str().unwrap(),
                "--threshold",
                "20",
            ])
            .output()
            .expect("verify extended format");
        assert!(
            verified.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&verified.stderr)
        );
    }

    let tool_output = tmp_dir.path().join("tool-converted.bmp");
    let tool = bin()
        .args([
            "tool",
            "convert",
            "--input",
            fixture("probe.png").to_str().unwrap(),
            "--output",
            tool_output.to_str().unwrap(),
        ])
        .output()
        .expect("tool convert to BMP");
    assert!(
        tool.status.success(),
        "{}",
        String::from_utf8_lossy(&tool.stderr)
    );
    assert!(tool_output.exists());
}

#[test]
fn test_cli_capture_and_enroll_help() {
    let out = bin()
        .args(["capture", "--help"])
        .output()
        .expect("capture help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--wait"));
    assert!(stdout.contains("--timeout"));
    assert!(stdout.contains("--min-quality"));

    let out = bin()
        .args(["enroll", "--help"])
        .output()
        .expect("enroll help");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("--samples"));
    assert!(stdout.contains("--timeout"));
    assert!(stdout.contains("--min-quality"));
}

#[test]
fn test_cli_verify_argument_validation() {
    // Neither reference nor gallery
    let out = bin()
        .args(["verify", fixture("probe.png").to_str().unwrap()])
        .output()
        .expect("verify command");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("specify a reference file for 1:1 verification or --gallery <DIR> for 1:N")
    );

    // Both reference and gallery
    let out = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--gallery",
            fixture("").parent().unwrap().to_str().unwrap(),
        ])
        .output()
        .expect("verify command");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("specify either <REFERENCE> for 1:1 or --gallery <DIR> for 1:N, not both")
    );
}

#[test]
fn test_cli_verify_gallery_text_and_json() {
    let fixtures_dir = fixture("probe.png").parent().unwrap().to_path_buf();

    // 1. Text output
    let out = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            "--gallery",
            fixtures_dir.to_str().unwrap(),
            "--threshold",
            "40",
        ])
        .output()
        .expect("verify gallery text");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1:N Gallery Verification Report"));
    assert!(stdout.contains("candidates"));
    assert!(stdout.contains("reference_match.png"));
    assert!(stdout.contains("MATCH"));

    // 2. JSON output
    let out = bin()
        .args([
            "--json",
            "verify",
            fixture("probe.png").to_str().unwrap(),
            "--gallery",
            fixtures_dir.to_str().unwrap(),
            "--threshold",
            "40",
        ])
        .output()
        .expect("verify gallery json");
    assert!(out.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json output");
    assert!(parsed.get("matches").is_some());
    assert!(parsed["matches"].as_array().unwrap().len() >= 3);
    assert_eq!(parsed["threshold_met"], true);
    assert_eq!(parsed["best_match"]["score"], 100.0);
    let matches = parsed["matches"].as_array().unwrap();
    assert!(matches.iter().any(|m| {
        m["reference_path"]
            .as_str()
            .unwrap()
            .contains("reference_match.png")
            && m["matched"] == true
    }));
    assert!(matches.iter().any(|m| {
        m["reference_path"]
            .as_str()
            .unwrap()
            .contains("reference_mismatch.png")
            && m["matched"] == false
    }));
}

#[test]
fn test_cli_format_json_and_clean_stdout_doctor() {
    let output = bin()
        .args(["--mock", "match", "--format", "json", "doctor"])
        .output()
        .expect("doctor format json");
    assert!(output.status.success());

    // stdout must be strictly valid JSON without any stray logging
    let stdout_str = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout_str.trim().is_empty());
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout_str).expect("stdout must be 100% valid JSON");
    assert!(parsed.get("checks").is_some());
    assert!(parsed.get("healthy").is_some());
    assert!(parsed.get("devices_found").is_some());

    // Verify checks contain udev, user_groups, nfiq2_tool
    let checks = parsed["checks"].as_array().expect("checks array");
    let check_names: Vec<&str> = checks.iter().filter_map(|c| c["name"].as_str()).collect();

    assert!(check_names.contains(&"usb_subsystem"));
    assert!(check_names.contains(&"udev_rules"));
    assert!(check_names.contains(&"user_groups"));
    assert!(check_names.contains(&"nfiq2_tool"));
    assert!(check_names.contains(&"filesystem_write"));
}

#[test]
fn test_cli_doctor_output_redirection() {
    let tmp_dir = std::env::temp_dir().join(format!("fs80h-redir-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let report_file = tmp_dir.join("report.json");
    let debug_file = tmp_dir.join("debug.log");

    let stdout_target = std::fs::File::create(&report_file).expect("create report file");
    let stderr_target = std::fs::File::create(&debug_file).expect("create debug file");

    let status = bin()
        .args(["--mock", "match", "-vv", "--format", "json", "doctor"])
        .stdout(stdout_target)
        .stderr(stderr_target)
        .status()
        .expect("run command with redirected streams");

    assert!(status.success());

    // report.json must contain strictly valid JSON
    let report_content = std::fs::read_to_string(&report_file).expect("read report");
    let parsed: serde_json::Value =
        serde_json::from_str(&report_content).expect("report.json must be valid JSON");
    assert!(parsed.get("checks").is_some());

    // debug.log must exist and be accessible
    assert!(debug_file.exists());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_cli_semantic_exit_codes() {
    // Exit code 1: operational syntax error (invalid flag)
    let out = bin()
        .arg("--unknown-flag")
        .output()
        .expect("run invalid flag");
    assert_eq!(out.status.code(), Some(1));

    // Exit code 1: parameter validation failure
    let out = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "120",
        ])
        .output()
        .expect("run invalid threshold");
    assert_eq!(out.status.code(), Some(1));

    // Exit code 2: biometric mismatch with --exit-code
    let out = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_mismatch.png").to_str().unwrap(),
            "--threshold",
            "50",
            "--exit-code",
        ])
        .output()
        .expect("run verify mismatch");
    assert_eq!(out.status.code(), Some(2));

    // Exit code 0: biometric match
    let out = bin()
        .args([
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "40",
            "--exit-code",
        ])
        .output()
        .expect("run verify match");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn test_cli_format_json_error_structure() {
    let out = bin()
        .args([
            "--format",
            "json",
            "verify",
            fixture("probe.png").to_str().unwrap(),
            fixture("reference_match.png").to_str().unwrap(),
            "--threshold",
            "200",
        ])
        .output()
        .expect("run verify with invalid threshold in json format");

    assert_eq!(out.status.code(), Some(1));
    let stderr_str = String::from_utf8_lossy(&out.stderr);
    let parsed: serde_json::Value =
        serde_json::from_str(&stderr_str).expect("stderr should contain structured JSON error");
    assert_eq!(parsed["status"], "error");
    assert_eq!(parsed["code"], 1);
    assert!(
        parsed["message"]
            .as_str()
            .unwrap()
            .contains("--threshold must be between 0 and 100")
    );
}

#[test]
fn test_cli_mock_capture_success_json() {
    let tmp_dir = std::env::temp_dir().join(format!("fs80h-mock-cap-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp_dir);
    let output_file = tmp_dir.join("mock_capture.png");

    let out = bin()
        .args([
            "--mock",
            "match",
            "capture",
            "--output",
            output_file.to_str().unwrap(),
            "--format",
            "json",
        ])
        .output()
        .expect("mock match capture");

    assert_eq!(out.status.code(), Some(0));
    assert!(output_file.exists());
    assert!(tmp_dir.join("mock_capture.png.meta.json").exists());

    let parsed: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json output");
    assert_eq!(parsed["format"], "png");
    assert_eq!(parsed["quality"]["accepted"], true);
    assert_eq!(parsed["standard_afis"], true);

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_cli_mock_verify_match() {
    let out = bin()
        .args([
            "--mock",
            "match",
            "verify",
            fixture("probe.png").to_str().unwrap(),
            "--exit-code",
        ])
        .output()
        .expect("mock match verify");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("meets threshold"));
}

#[test]
fn test_cli_mock_verify_mismatch() {
    let out = bin()
        .args([
            "--mock",
            "mismatch",
            "verify",
            fixture("probe.png").to_str().unwrap(),
            "--exit-code",
        ])
        .output()
        .expect("mock mismatch verify");

    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("below threshold"));
}

#[test]
fn test_cli_mock_capture_degraded_quality() {
    let tmp_file = std::env::temp_dir().join(format!("mock_deg_{}.png", std::process::id()));
    let out = bin()
        .args([
            "--mock",
            "degraded",
            "capture",
            "--output",
            tmp_file.to_str().unwrap(),
        ])
        .output()
        .expect("mock degraded capture");

    assert_eq!(out.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("insufficient biometric quality"));
    let _ = std::fs::remove_file(&tmp_file);
}

#[test]
fn test_cli_mock_capture_disconnect_hardware() {
    let tmp_file = std::env::temp_dir().join(format!("mock_dc_{}.png", std::process::id()));
    let out = bin()
        .args([
            "--mock",
            "disconnect",
            "capture",
            "--output",
            tmp_file.to_str().unwrap(),
        ])
        .output()
        .expect("mock disconnect capture");

    assert_eq!(out.status.code(), Some(5));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("hardware error"));
    let _ = std::fs::remove_file(&tmp_file);
}

#[test]
fn test_cli_mock_env_variable() {
    let out = bin()
        .env("FS80H_MOCK_FIXTURE", "match")
        .args(["devices"])
        .output()
        .expect("mock env devices");

    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1491:0020 bus 1 address 2 model 80"));
}
