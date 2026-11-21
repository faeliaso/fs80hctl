//! User configuration loading and explicit CLI precedence.
use crate::cli::{Cli, CliError, Command, ConfigCommand, ForensicCommand, ToolCommand};
use clap::{ArgMatches, parser::ValueSource};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

const EXAMPLE: &str = "# fs80hctl user defaults; CLI arguments take precedence.\nschema_version = 1\n[capture]\ntimeout_secs = 10\nmin_quality = 50\n[enroll]\nsamples = 3\ntimeout_secs = 10\nmin_quality = 50\n[verify]\nthreshold = 40.0\n[forensic]\ncomparison_threshold = 40.0\n[conversion]\nwsq_bitrate = 0.75\ndpi = 500\n[driver]\npoll_interval_ms = 150\n# [daemon]\n# socket_path = \"/tmp/fs80h.sock\"\n";

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    schema_version: Option<u32>,
    #[serde(default)]
    capture: Capture,
    #[serde(default)]
    enroll: Enroll,
    #[serde(default)]
    verify: Verify,
    #[serde(default)]
    forensic: Forensic,
    #[serde(default)]
    conversion: Conversion,
    #[serde(default)]
    driver: Driver,
    #[serde(default)]
    daemon: Daemon,
}
macro_rules! section {
    ($name:ident { $($field:ident: $ty:ty),* }) => {
        #[derive(Default, Deserialize, Serialize)]
        #[serde(deny_unknown_fields)]
        struct $name { $( $field: Option<$ty> ),* }
    };
}
section!(Capture {
    timeout_secs: u64,
    min_quality: u8
});
section!(Enroll {
    samples: usize,
    timeout_secs: u64,
    min_quality: u8
});
section!(Verify { threshold: f32 });
section!(Forensic {
    comparison_threshold: f32
});
section!(Conversion {
    wsq_bitrate: f32,
    dpi: u16
});
section!(Driver {
    poll_interval_ms: u64
});
section!(Daemon {
    socket_path: PathBuf
});
fn error(e: impl std::fmt::Display) -> CliError {
    CliError::Operational(format!("configuration: {e}"))
}
fn default_path(
    os: &str,
    home: Option<PathBuf>,
    xdg: Option<PathBuf>,
) -> Result<PathBuf, CliError> {
    if os == "linux"
        && let Some(p) = xdg.filter(|p| p.is_absolute())
    {
        return Ok(p.join("fs80hctl/config.toml"));
    }
    let home = home.ok_or_else(|| error("HOME is unavailable; use --config or --no-config"))?;
    Ok(home.join(if os == "macos" {
        "Library/Application Support/fs80hctl/config.toml"
    } else {
        ".config/fs80hctl/config.toml"
    }))
}
fn validate(s: &Settings) -> Result<(), CliError> {
    if s.schema_version != Some(1) {
        return Err(error("schema_version must be 1"));
    }
    for (key, value) in [
        ("capture.timeout_secs", s.capture.timeout_secs),
        ("enroll.timeout_secs", s.enroll.timeout_secs),
    ] {
        if value.is_some_and(|v| v == 0 || v.checked_mul(1000).is_none()) {
            return Err(error(format!(
                "{key} must be positive and fit milliseconds"
            )));
        }
    }
    for (key, v) in [
        ("capture.min_quality", s.capture.min_quality),
        ("enroll.min_quality", s.enroll.min_quality),
    ] {
        if v.is_some_and(|v| v > 100) {
            return Err(error(format!("{key} must be 0..100")));
        }
    }
    for (key, v) in [
        ("verify.threshold", s.verify.threshold),
        (
            "forensic.comparison_threshold",
            s.forensic.comparison_threshold,
        ),
    ] {
        if v.is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v)) {
            return Err(error(format!("{key} must be finite and 0..100")));
        }
    }
    if s.enroll.samples == Some(0) {
        return Err(error("enroll.samples must be positive"));
    }
    if s.driver.poll_interval_ms == Some(0) {
        return Err(error("driver.poll_interval_ms must be positive"));
    }
    if s.conversion.dpi == Some(0) {
        return Err(error("conversion.dpi must be positive"));
    }
    if s.conversion
        .wsq_bitrate
        .is_some_and(|v| !v.is_finite() || v <= 0.0)
    {
        return Err(error("conversion.wsq_bitrate must be finite and positive"));
    }
    if s.daemon
        .socket_path
        .as_ref()
        .is_some_and(|p| !p.is_absolute())
    {
        return Err(error("daemon.socket_path must be absolute"));
    }
    Ok(())
}
fn explicit(m: &ArgMatches, key: &str) -> bool {
    matches!(
        m.value_source(key),
        Some(ValueSource::CommandLine | ValueSource::EnvVariable)
    )
}

pub fn prepare(cli: &mut Cli, matches: &ArgMatches) -> Result<bool, CliError> {
    let action = if let Command::Config { command } = &cli.command {
        Some(command)
    } else {
        None
    };
    if cli.no_config && matches!(action, Some(ConfigCommand::Init | ConfigCommand::Validate)) {
        return Err(error(
            "this command requires configuration; remove --no-config",
        ));
    }
    let path = if cli.no_config {
        None
    } else {
        Some(cli.config.clone().map(Ok).unwrap_or_else(|| {
            default_path(
                std::env::consts::OS,
                std::env::var_os("HOME").map(PathBuf::from),
                std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            )
        })?)
    };
    if matches!(action, Some(ConfigCommand::Path)) {
        crate::print_json(
            &serde_json::json!({"path": path, "exists": path.as_ref().is_some_and(|p|p.exists())}),
        )?;
        return Ok(true);
    }
    if matches!(action, Some(ConfigCommand::Init)) {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let path = path.as_ref().expect("config enabled");
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(error)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(error)?;
        file.write_all(EXAMPLE.as_bytes()).map_err(error)?;
        crate::print_json(&serde_json::json!({"created": path}))?;
        return Ok(true);
    }
    let mut settings = Settings::default();
    if let Some(path) = &path {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                settings = toml::from_str(&text).map_err(error)?;
                validate(&settings)?;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && cli.config.is_none()
                    && !matches!(action, Some(ConfigCommand::Validate)) => {}
            Err(e) => return Err(error(format!("{}: {e}", path.display()))),
        }
    }
    if matches!(action, Some(ConfigCommand::Validate)) {
        crate::print_json(&serde_json::json!({"valid":true,"path":path}))?;
        return Ok(true);
    }
    if matches!(action, Some(ConfigCommand::Show)) {
        let defaults: Settings = toml::from_str(EXAMPLE).map_err(error)?;
        let values = serde_json::to_value(&settings).map_err(error)?;
        let defaults = serde_json::to_value(defaults).map_err(error)?;
        let mut result = BTreeMap::new();
        for (section, fields) in defaults.as_object().unwrap() {
            if let Some(fields) = fields.as_object() {
                for (key, default) in fields {
                    let configured = &values[section][key];
                    let (mut value, mut source) = if configured.is_null() {
                        (default.clone(), "default")
                    } else {
                        (configured.clone(), "file")
                    };
                    if section == "daemon" && key == "socket_path" {
                        if std::env::var_os("FS80H_SOCKET_PATH").is_some() {
                            value = serde_json::json!(crate::daemon::default_socket_path());
                            source = "environment";
                        } else if value.is_null() {
                            value = serde_json::json!(crate::daemon::default_socket_path());
                        }
                    }
                    result.insert(
                        format!("{section}.{key}"),
                        serde_json::json!({"value":value,"source":source}),
                    );
                }
            }
        }
        crate::print_json(&serde_json::json!({"path":path,"schema_version":1,"settings":result}))?;
        return Ok(true);
    }
    cli.poll_interval_ms = settings.driver.poll_interval_ms.unwrap_or(150);
    let socket = std::env::var_os("FS80H_SOCKET_PATH")
        .map(PathBuf::from)
        .or(settings.daemon.socket_path);
    let m = matches.subcommand().expect("command").1;
    macro_rules! apply {
        ($args:ident, $field:ident, $id:literal, $value:expr, $m:expr) => {
            if !explicit($m, $id)
                && let Some(value) = $value
            {
                $args.$field = value;
            }
        };
    }
    match &mut cli.command {
        Command::Capture(a) => {
            apply!(a, timeout, "timeout", settings.capture.timeout_secs, m);
            apply!(
                a,
                min_quality,
                "min_quality",
                settings.capture.min_quality,
                m
            );
            a.poll_interval_ms = cli.poll_interval_ms;
            if a.socket.is_none() {
                a.socket = socket;
            }
        }
        Command::Enroll(a) => {
            apply!(a, timeout, "timeout", settings.enroll.timeout_secs, m);
            apply!(
                a,
                min_quality,
                "min_quality",
                settings.enroll.min_quality,
                m
            );
            apply!(a, samples, "samples", settings.enroll.samples, m);
            a.poll_interval_ms = cli.poll_interval_ms;
            if a.socket.is_none() {
                a.socket = socket;
            }
        }
        Command::Verify(a) => {
            apply!(a, threshold, "threshold", settings.verify.threshold, m);
        }
        Command::Forensic {
            command: ForensicCommand::Compare(a),
        } => {
            apply!(
                a,
                threshold,
                "threshold",
                settings.forensic.comparison_threshold,
                m.subcommand().unwrap().1
            );
        }
        Command::Tool {
            command: ToolCommand::Convert(a),
        } => {
            let m = m.subcommand().unwrap().1;
            apply!(a, bitrate, "bitrate", settings.conversion.wsq_bitrate, m);
            apply!(a, dpi, "dpi", settings.conversion.dpi, m);
        }
        Command::Serve(a) if a.socket_path.is_none() => {
            a.socket_path = socket;
        }
        _ => {}
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_paths() {
        assert_eq!(
            default_path("linux", Some("/home/a".into()), Some("/cfg".into())).unwrap(),
            PathBuf::from("/cfg/fs80hctl/config.toml")
        );
        assert_eq!(
            default_path("macos", Some("/Users/a".into()), None).unwrap(),
            PathBuf::from("/Users/a/Library/Application Support/fs80hctl/config.toml")
        );
        assert_eq!(
            default_path("linux", Some("/home/a".into()), Some("relative".into())).unwrap(),
            PathBuf::from("/home/a/.config/fs80hctl/config.toml")
        );
    }
    #[test]
    fn schema_validation() {
        validate(&toml::from_str(EXAMPLE).unwrap()).unwrap();
        assert!(toml::from_str::<Settings>("schema_version=1\nunknown=2").is_err());
        for text in [
            "schema_version=2",
            "schema_version=1\n[capture]\ntimeout_secs=0",
            "schema_version=1\n[verify]\nthreshold=nan",
        ] {
            assert!(validate(&toml::from_str(text).unwrap()).is_err());
        }
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};
    #[test]
    fn file_defaults_and_explicit_cli_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "schema_version=1\n[capture]\ntimeout_secs=23\nmin_quality=61\n[driver]\npoll_interval_ms=7").unwrap();
        for extra in [vec![], vec!["--timeout", "10"]] {
            let mut args = vec!["fs80hctl", "--config", path.to_str().unwrap(), "capture"];
            args.extend(extra.iter().copied());
            let m = Cli::command().try_get_matches_from(args).unwrap();
            let mut cli = Cli::from_arg_matches(&m).unwrap();
            assert!(!prepare(&mut cli, &m).unwrap());
            let Command::Capture(a) = cli.command else {
                panic!()
            };
            assert_eq!(a.timeout, if extra.is_empty() { 23 } else { 10 });
            assert_eq!(a.min_quality, 61);
            assert_eq!(a.poll_interval_ms, 7);
        }
    }
    #[test]
    fn missing_explicit_file_fails_and_no_config_preserves_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing");
        let m = Cli::command()
            .try_get_matches_from(["fs80hctl", "--config", path.to_str().unwrap(), "capture"])
            .unwrap();
        assert!(prepare(&mut Cli::from_arg_matches(&m).unwrap(), &m).is_err());
        let m = Cli::command()
            .try_get_matches_from(["fs80hctl", "--no-config", "enroll"])
            .unwrap();
        let mut cli = Cli::from_arg_matches(&m).unwrap();
        prepare(&mut cli, &m).unwrap();
        let Command::Enroll(a) = cli.command else {
            panic!()
        };
        assert_eq!(
            (a.timeout, a.samples, a.min_quality, a.poll_interval_ms),
            (10, 3, 50, 150)
        );
    }
    #[test]
    fn remaining_command_settings_are_applied() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "schema_version=1\n[enroll]\nsamples=7\ntimeout_secs=21\nmin_quality=66\n[verify]\nthreshold=77.0\n[forensic]\ncomparison_threshold=78.0\n[conversion]\nwsq_bitrate=1.2\ndpi=600\n[driver]\npoll_interval_ms=8").unwrap();
        for command in [
            vec!["enroll"],
            vec!["verify", "probe.png", "ref.png"],
            vec![
                "forensic",
                "compare",
                "--probe",
                "p.png",
                "--reference",
                "r.png",
                "--output",
                "o.png",
            ],
            vec!["tool", "convert", "--input", "i.png", "--output", "o.wsq"],
            vec!["serve"],
        ] {
            let mut args = vec!["fs80hctl", "--config", path.to_str().unwrap()];
            args.extend(command);
            let m = Cli::command().try_get_matches_from(args).unwrap();
            let mut cli = Cli::from_arg_matches(&m).unwrap();
            prepare(&mut cli, &m).unwrap();
            match cli.command {
                Command::Enroll(a) => assert_eq!(
                    (a.samples, a.timeout, a.min_quality, a.poll_interval_ms),
                    (7, 21, 66, 8)
                ),
                Command::Verify(a) => assert_eq!(a.threshold, 77.0),
                Command::Forensic {
                    command: ForensicCommand::Compare(a),
                } => assert_eq!(a.threshold, 78.0),
                Command::Tool {
                    command: ToolCommand::Convert(a),
                } => assert_eq!((a.bitrate, a.dpi), (1.2, 600)),
                Command::Serve(_) => assert_eq!(cli.poll_interval_ms, 8),
                _ => panic!(),
            }
        }
    }
    #[test]
    fn init_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "keep").unwrap();
        let m = Cli::command()
            .try_get_matches_from([
                "fs80hctl",
                "--config",
                path.to_str().unwrap(),
                "config",
                "init",
            ])
            .unwrap();
        assert!(prepare(&mut Cli::from_arg_matches(&m).unwrap(), &m).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "keep");
    }
}

#[cfg(test)]
mod additional_tests {
    use super::*;
    use clap::{CommandFactory, FromArgMatches};

    #[test]
    fn rejects_invalid_values_in_all_sections() {
        for section in [
            "[capture]\ntimeout_secs=9223372036854775807",
            "[capture]\nmin_quality=101",
            "[enroll]\nsamples=0",
            "[enroll]\ntimeout_secs=0",
            "[enroll]\nmin_quality=101",
            "[verify]\nthreshold=-1.0",
            "[forensic]\ncomparison_threshold=inf",
            "[conversion]\nwsq_bitrate=nan",
            "[conversion]\nwsq_bitrate=0.0",
            "[conversion]\ndpi=0",
            "[driver]\npoll_interval_ms=0",
            "[daemon]\nsocket_path='relative'",
        ] {
            let settings: Settings =
                toml::from_str(&format!("schema_version=1\n{section}")).unwrap();
            assert!(validate(&settings).is_err(), "{section}");
        }
        assert!(toml::from_str::<Settings>("schema_version=1\n[capture]\nunknown=1").is_err());
        assert!(validate(&Settings::default()).is_err());
        assert!(default_path("linux", None, None).is_err());
        assert_eq!(
            default_path("linux", None, Some("/cfg".into())).unwrap(),
            PathBuf::from("/cfg/fs80hctl/config.toml")
        );
    }

    #[test]
    fn explicit_defaults_override_file_including_nested_commands() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "schema_version=1\n[enroll]\nsamples=9\ntimeout_secs=20\nmin_quality=60\n[verify]\nthreshold=80.0\n[forensic]\ncomparison_threshold=80.0\n[conversion]\nwsq_bitrate=1.5\ndpi=600\n[daemon]\nsocket_path='/tmp/file.sock'").unwrap();
        for args in [
            vec![
                "enroll",
                "--samples",
                "3",
                "--timeout",
                "10",
                "--min-quality",
                "50",
                "--socket",
                "/tmp/explicit.sock",
            ],
            vec!["verify", "p.png", "r.png", "--threshold", "40"],
            vec![
                "forensic",
                "compare",
                "--probe",
                "p.png",
                "--reference",
                "r.png",
                "--output",
                "o.png",
                "--threshold",
                "40",
            ],
            vec![
                "tool",
                "convert",
                "--input",
                "i.png",
                "--output",
                "o.wsq",
                "--bitrate",
                "0.75",
                "--dpi",
                "500",
            ],
            vec![
                "serve",
                "--socket-path",
                "/tmp/explicit.sock",
                "--systemd-activation",
            ],
            vec!["capture", "--socket", "/tmp/explicit.sock"],
        ] {
            let m = Cli::command()
                .try_get_matches_from(
                    ["fs80hctl", "--config", path.to_str().unwrap()]
                        .into_iter()
                        .chain(args),
                )
                .unwrap();
            let mut cli = Cli::from_arg_matches(&m).unwrap();
            prepare(&mut cli, &m).unwrap();
            match cli.command {
                Command::Enroll(a) => {
                    assert_eq!((a.samples, a.timeout, a.min_quality), (3, 10, 50));
                    assert_eq!(a.socket.unwrap(), PathBuf::from("/tmp/explicit.sock"));
                }
                Command::Verify(a) => assert_eq!(a.threshold, 40.0),
                Command::Forensic {
                    command: ForensicCommand::Compare(a),
                } => assert_eq!(a.threshold, 40.0),
                Command::Tool {
                    command: ToolCommand::Convert(a),
                } => assert_eq!((a.bitrate, a.dpi), (0.75, 500)),
                Command::Serve(a) => {
                    assert!(a.systemd_activation);
                    assert_eq!(a.socket_path.unwrap(), PathBuf::from("/tmp/explicit.sock"));
                }
                Command::Capture(a) => {
                    assert_eq!(a.socket.unwrap(), PathBuf::from("/tmp/explicit.sock"))
                }
                _ => panic!("unexpected command"),
            }
        }
    }
}
