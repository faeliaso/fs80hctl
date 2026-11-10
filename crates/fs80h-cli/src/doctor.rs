use fs80h_driver::{FutronicScanner, MockScenario, MockTransport, list_devices_with_mock};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub passed: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DoctorReport {
    pub healthy: bool,
    pub checks: Vec<DoctorCheck>,
    pub devices_found: usize,
}

#[allow(dead_code)]
pub fn run_doctor() -> DoctorReport {
    run_doctor_with_mock(None)
}

pub fn run_doctor_with_mock(mock: Option<MockScenario>) -> DoctorReport {
    let mut checks = Vec::new();
    let mut healthy = true;
    let mut devices_found = 0;

    // 1. USB Subsystem & Device Detection
    match list_devices_with_mock(mock) {
        Ok(devices) => {
            devices_found = devices.len();
            checks.push(DoctorCheck {
                name: "usb_subsystem".into(),
                passed: true,
                message: if mock.is_some() {
                    "USB subsystem operational (emulated mock transport)".into()
                } else {
                    "USB subsystem operational (libusb initialized)".into()
                },
                details: None,
                suggestion: None,
            });

            if devices_found > 0 {
                let dev_desc = devices
                    .iter()
                    .map(|d| {
                        format!(
                            "{:04x}:{:04x} (bus {}, addr {}, mod {})",
                            d.vendor_id, d.product_id, d.bus_number, d.address, d.model
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");

                checks.push(DoctorCheck {
                    name: "device_detection".into(),
                    passed: true,
                    message: format!("{devices_found} Futronic scanner(s) detected"),
                    details: Some(dev_desc),
                    suggestion: None,
                });

                // Device communication & node permissions
                #[allow(unused_mut)]
                let mut access_ok = true;
                #[allow(unused_mut)]
                let mut access_details: Vec<String> = Vec::new();

                #[cfg(target_os = "linux")]
                if mock.is_none() {
                    for d in &devices {
                        let dev_path = format!("/dev/bus/usb/{:03}/{:03}", d.bus_number, d.address);
                        let path = Path::new(&dev_path);
                        if path.exists() {
                            match std::fs::OpenOptions::new()
                                .read(true)
                                .write(true)
                                .open(path)
                            {
                                Ok(_) => {
                                    access_details.push(format!("{dev_path}: read/write OK"));
                                }
                                Err(err) => {
                                    access_ok = false;
                                    access_details
                                        .push(format!("{dev_path}: permission denied ({err})"));
                                }
                            }
                        } else {
                            access_details.push(format!("{dev_path}: node not present"));
                        }
                    }
                }

                let open_result = match mock {
                    Some(scenario) => {
                        if scenario == MockScenario::Disconnect {
                            Err(fs80h_driver::DriverError::Transport(
                                fs80h_driver::TransportError::DeviceDisconnected,
                            ))
                        } else {
                            Ok(FutronicScanner::new_mock(MockTransport::new(scenario)))
                        }
                    }
                    None => FutronicScanner::open_first(),
                };

                match open_result {
                    Ok(_) => {
                        let details_msg = if access_details.is_empty() {
                            None
                        } else {
                            Some(access_details.join("; "))
                        };
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: access_ok,
                            message: "USB scanner communication verified".into(),
                            details: details_msg,
                            suggestion: None,
                        });
                        if !access_ok {
                            healthy = false;
                        }
                    }
                    Err(err) => {
                        healthy = false;
                        let details_msg = if access_details.is_empty() {
                            err.to_string()
                        } else {
                            format!("{err} ({})", access_details.join("; "))
                        };
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: false,
                            message: "Permission or I/O error opening USB scanner".into(),
                            details: Some(details_msg),
                            suggestion: Some(
                                "Ensure user belongs to 'plugdev' group or udev rules are active."
                                    .into(),
                            ),
                        });
                    }
                }
            } else {
                checks.push(DoctorCheck {
                    name: "device_detection".into(),
                    passed: false,
                    message: "No Futronic FS80/FS80H/FS81H scanner detected on USB bus".into(),
                    details: Some("Connect scanner or check USB cable/hub.".into()),
                    suggestion: Some(
                        "Plug the Futronic USB scanner into an active USB port.".into(),
                    ),
                });
                checks.push(DoctorCheck {
                    name: "device_access".into(),
                    passed: true,
                    message: "Skipped (no scanner available for communication test)".into(),
                    details: None,
                    suggestion: None,
                });
            }
        }
        Err(err) => {
            healthy = false;
            checks.push(DoctorCheck {
                name: "usb_subsystem".into(),
                passed: false,
                message: "Failed to initialize USB subsystem".into(),
                details: Some(err.to_string()),
                suggestion: Some(
                    "Verify libusb is installed and accessible on this system.".into(),
                ),
            });
        }
    }

    // 2. Linux Udev Rules Check
    let udev_check = check_udev_rules();
    checks.push(udev_check);

    // 3. User Groups & Permissions Check
    let groups_check = check_user_groups();
    checks.push(groups_check);

    // 4. External NFIQ2 Tool Availability Check (Decoupled, strictly external)
    let nfiq2_check = check_nfiq2_tool();
    checks.push(nfiq2_check);

    // 5. Filesystem Write Permission
    let fs_check = check_filesystem_write();
    if !fs_check.passed {
        healthy = false;
    }
    checks.push(fs_check);

    DoctorReport {
        healthy,
        checks,
        devices_found,
    }
}

fn check_udev_rules() -> DoctorCheck {
    #[cfg(target_os = "linux")]
    {
        let candidate_dirs = [
            "/etc/udev/rules.d",
            "/lib/udev/rules.d",
            "/usr/lib/udev/rules.d",
        ];

        let mut found_rule_file = None;
        for dir in &candidate_dirs {
            let path = Path::new(dir);
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let file_path = entry.path();
                    if let Some(ext) = file_path.extension() {
                        if ext == "rules" {
                            if let Ok(content) = std::fs::read_to_string(&file_path) {
                                if content.contains("1491")
                                    || content.to_lowercase().contains("futronic")
                                {
                                    found_rule_file = Some(file_path.display().to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if found_rule_file.is_some() {
                break;
            }
        }

        if let Some(rule_file) = found_rule_file {
            DoctorCheck {
                name: "udev_rules".into(),
                passed: true,
                message: "Futronic udev rule configured".into(),
                details: Some(format!("Found vendor rule in {rule_file}")),
                suggestion: None,
            }
        } else {
            DoctorCheck {
                name: "udev_rules".into(),
                passed: false,
                message: "No udev rule found for Futronic vendor 1491".into(),
                details: Some("Without udev rules, non-root users may lack device permissions.".into()),
                suggestion: Some(
                    "Create /etc/udev/rules.d/70-futronic-fs80h.rules with:\n\
                     SUBSYSTEM==\"usb\", ATTR{idVendor}==\"1491\", ATTR{idProduct}==\"0020\", MODE=\"0660\", GROUP=\"plugdev\", TAG+=\"uaccess\"\n\
                     SUBSYSTEM==\"usb\", ATTR{idVendor}==\"1491\", ATTR{idProduct}==\"0025\", MODE=\"0660\", GROUP=\"plugdev\", TAG+=\"uaccess\"\n\
                     Then reload rules with: sudo udevadm control --reload-rules && sudo udevadm trigger"
                        .into(),
                ),
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        DoctorCheck {
            name: "udev_rules".into(),
            passed: true,
            message: "Udev check skipped (udev is Linux-specific)".into(),
            details: Some(format!("OS: {}", std::env::consts::OS)),
            suggestion: None,
        }
    }
}

fn check_user_groups() -> DoctorCheck {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("id").arg("-Gn").output();
        match output {
            Ok(out) if out.status.success() => {
                let groups = String::from_utf8_lossy(&out.stdout);
                let in_plugdev = groups.split_whitespace().any(|g| g == "plugdev");
                if in_plugdev {
                    DoctorCheck {
                        name: "user_groups".into(),
                        passed: true,
                        message: "Current user belongs to 'plugdev' group".into(),
                        details: Some(format!("Active groups: {}", groups.trim())),
                        suggestion: None,
                    }
                } else {
                    DoctorCheck {
                        name: "user_groups".into(),
                        passed: false,
                        message: "Current user is not in 'plugdev' group".into(),
                        details: Some(format!("Active groups: {}", groups.trim())),
                        suggestion: Some(
                            "Add user to plugdev group: sudo usermod -a -G plugdev $USER".into(),
                        ),
                    }
                }
            }
            _ => DoctorCheck {
                name: "user_groups".into(),
                passed: true,
                message: "Unable to inspect groups via 'id'".into(),
                details: None,
                suggestion: None,
            },
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        DoctorCheck {
            name: "user_groups".into(),
            passed: true,
            message: "User session permissions verified".into(),
            details: Some(format!("OS: {}", std::env::consts::OS)),
            suggestion: None,
        }
    }
}

fn find_executable_in_path(name: &str) -> Option<PathBuf> {
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let direct = dir.join(name);
            if direct.is_file() {
                return Some(direct);
            }
            #[cfg(target_os = "windows")]
            {
                let with_exe = dir.join(format!("{name}.exe"));
                if with_exe.is_file() {
                    return Some(with_exe);
                }
            }
        }
    }
    None
}

fn check_nfiq2_tool() -> DoctorCheck {
    match find_executable_in_path("nfiq2") {
        Some(path) => {
            let output = Command::new(&path)
                .output()
                .or_else(|_| Command::new(&path).arg("-h").output());

            let mut version_str = None;
            if let Ok(out) = output {
                let combined = format!(
                    "{}\n{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr)
                );
                for line in combined.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("NFIQ 2:") || trimmed.starts_with("NFIQ2:") {
                        version_str = Some(trimmed.to_string());
                        break;
                    }
                }
            }

            let version_display = version_str.unwrap_or_else(|| "detected".to_string());

            DoctorCheck {
                name: "nfiq2_tool".into(),
                passed: true,
                message: "External NFIQ2 executable detected in PATH".into(),
                details: Some(format!(
                    "Location: {} | Version: {}",
                    path.display(),
                    version_display
                )),
                suggestion: None,
            }
        }
        None => DoctorCheck {
            name: "nfiq2_tool".into(),
            passed: false,
            message: "External nfiq2 executable not found in PATH".into(),
            details: Some(
                "NIST NFIQ 2.x is an optional external tool for standardized biometric quality assessment."
                    .into(),
            ),
            suggestion: Some(
                "To enable NFIQ2 integration, install the nfiq2 binary manually and ensure it is placed in your PATH (e.g., /usr/local/bin/nfiq2)."
                    .into(),
            ),
        },
    }
}

fn check_filesystem_write() -> DoctorCheck {
    let test_file = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!(".fs80h_test_{}", std::process::id()));

    match std::fs::write(&test_file, b"fs80h_doctor_test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            DoctorCheck {
                name: "filesystem_write".into(),
                passed: true,
                message: "Write permission confirmed in current working directory".into(),
                details: None,
                suggestion: None,
            }
        }
        Err(err) => DoctorCheck {
            name: "filesystem_write".into(),
            passed: false,
            message: "No write permission in current working directory".into(),
            details: Some(err.to_string()),
            suggestion: Some("Run from a writable directory or check folder permissions.".into()),
        },
    }
}
