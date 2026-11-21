use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

fn bin(home: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fs80hctl"));
    cmd.env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("FS80H_SOCKET_PATH")
        .env_remove("FS80H_MOCK_FIXTURE");
    cmd
}
fn json_output(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn init_path_show_validate_and_private_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("private/config.toml");
    let run = |args: &[&str]| {
        bin(dir.path())
            .arg("--config")
            .arg(&path)
            .args(args)
            .output()
            .unwrap()
    };
    assert_eq!(json_output(run(&["config", "path"]))["exists"], false);
    json_output(run(&["config", "init"]));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        json_output(run(&["--json", "config", "validate"]))["valid"],
        true
    );
    let shown = json_output(run(&["config", "show"]));
    assert_eq!(
        shown["settings"]["driver.poll_interval_ms"],
        json!({"value":150,"source":"file"})
    );
    let original = std::fs::read(&path).unwrap();
    assert!(!run(&["config", "init"]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn missing_invalid_and_disabled_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let run = |args: &[&str]| {
        bin(dir.path())
            .arg("--config")
            .arg(&path)
            .args(args)
            .output()
            .unwrap()
    };
    assert!(
        bin(dir.path())
            .args(["config", "show"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !bin(dir.path())
            .args(["config", "validate"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!run(&["config", "show"]).status.success());
    std::fs::write(&path, "schema_version = 1\nunknown = true").unwrap();
    assert!(!run(&["config", "validate"]).status.success());
    for args in [vec!["config", "path"], vec!["--help"], vec!["--version"]] {
        assert!(run(&args).status.success());
    }
    assert!(!run(&["--no-config", "config", "show"]).status.success());
    for action in ["init", "validate"] {
        assert!(
            !bin(dir.path())
                .args(["--no-config", "config", action])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let shown = json_output(
        bin(dir.path())
            .args(["--no-config", "config", "show"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        shown["settings"]["capture.timeout_secs"],
        json!({"value":10,"source":"default"})
    );
}

#[test]
fn socket_environment_overrides_file_even_with_no_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "schema_version=1\n[daemon]\nsocket_path='/tmp/file.sock'",
    )
    .unwrap();
    let file = json_output(
        bin(dir.path())
            .arg("--config")
            .arg(&path)
            .args(["config", "show"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        file["settings"]["daemon.socket_path"],
        json!({"value":"/tmp/file.sock","source":"file"})
    );
    for options in [
        vec!["--config", path.to_str().unwrap()],
        vec!["--no-config"],
    ] {
        let shown = json_output(
            bin(dir.path())
                .env("FS80H_SOCKET_PATH", "/tmp/env.sock")
                .args(options)
                .args(["config", "show"])
                .output()
                .unwrap(),
        );
        assert_eq!(
            shown["settings"]["daemon.socket_path"],
            json!({"value":"/tmp/env.sock","source":"environment"})
        );
    }
}
