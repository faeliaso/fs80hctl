use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use fs80h_core::{
    CaptureManifest, DPI, DerivedManifest, FRAME_LEN, FingerprintScanner, HEIGHT, ImageFormat,
    Manifest, RawFrame, WIDTH, compare_frames, evaluate_quality,
};
use fs80h_driver::{FutronicScanner, list_devices};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "fs80hctl",
    version,
    about = "Technical control tool for Futronic FS80/FS80H fingerprint scanners",
    long_about = "Technical control CLI for Futronic FS80/FS80H optical fingerprint scanners, supporting frame acquisition, SHA-256 chain of custody, forensic transcoding (PNG, TIFF 500 DPI, NIST PGM, RAW), image inspection, and 1:1 biometric verification."
)]
struct Cli {
    /// Output results in structured JSON format
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected Futronic fingerprint scanners via USB
    Devices,
    /// Device inspection and low-level diagnostic operations
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Capture an optical biometric frame from the scanner
    Capture(CaptureArgs),
    /// Transcode biometric images across forensic formats (PNG, TIFF, PGM, RAW)
    Convert(ConvertArgs),
    /// Inspect image dimensions, DPI, SHA-256 integrity, quality metrics, and custody manifest
    Inspect(InspectArgs),
    /// Compare two biometric images (1:1) and compute technical similarity score
    Verify(VerifyArgs),
    /// Run environment diagnostics (USB subsystem, device permissions, filesystem write)
    Doctor,
    /// Display technical manual and quick reference guide
    Manual,
}

#[derive(Subcommand)]
enum DeviceCommand {
    /// Inspect USB communication with the selected scanner
    Inspect(Selection),
}

#[derive(Args, Clone)]
struct Selection {
    /// USB bus number (e.g., 1)
    #[arg(long)]
    bus: Option<u8>,
    /// USB device address (e.g., 2)
    #[arg(long)]
    address: Option<u8>,
}

#[derive(Args)]
struct CaptureArgs {
    #[command(flatten)]
    selection: Selection,
    /// Output file path (default: capture.png)
    #[arg(short, long, default_value = "capture.png")]
    output: PathBuf,
    /// Image format (png, tiff, pgm, raw). Inferred from output extension if omitted.
    #[arg(long)]
    format: Option<ImageFormat>,
    /// Save raw optical sensor frame without ridge inversion (light ridges on dark background)
    #[arg(long)]
    raw: bool,
    /// Use direct variable-dose acquisition path instead of default frame mode
    #[arg(long)]
    variable_dose: bool,
    /// Exposure dose used with --variable-dose (default: 166)
    #[arg(long, default_value_t = 166)]
    dose: u8,
}

#[derive(Args)]
struct ConvertArgs {
    /// Input image file (PNG, TIFF, PGM, RAW)
    input: PathBuf,
    /// Target format (png, tiff, pgm, raw). Inferred from output extension if omitted.
    #[arg(short = 't', long)]
    to: Option<ImageFormat>,
    /// Output file path
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Invert pixel polarity (255 - pixel)
    #[arg(long)]
    invert: bool,
}

#[derive(Args)]
struct InspectArgs {
    /// Biometric image file to inspect (PNG, TIFF, PGM, RAW)
    image: PathBuf,
}

#[derive(Args)]
struct VerifyArgs {
    /// Probe / sample image file (PNG, TIFF, PGM, RAW)
    probe: PathBuf,
    /// Reference / template image file (PNG, TIFF, PGM, RAW)
    reference: PathBuf,
    /// Minimum similarity threshold for verification (0 to 100)
    #[arg(long, default_value_t = 40.0)]
    threshold: f32,
    /// Exit with code 1 if threshold is not met
    #[arg(long)]
    exit_code: bool,
}

#[derive(Serialize)]
struct VerifyOutput {
    deterministic: fs80h_core::MatchReport,
    threshold: f32,
    threshold_met: bool,
    biometric_score: Option<f32>,
}

#[derive(Serialize)]
struct DoctorCheck {
    name: String,
    passed: bool,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[derive(Serialize)]
struct DoctorReport {
    healthy: bool,
    checks: Vec<DoctorCheck>,
    devices_found: usize,
}

fn run_doctor() -> DoctorReport {
    let mut checks = Vec::new();
    let mut healthy = true;
    let mut devices_found = 0;

    // 1. USB subsystem / libusb check
    match list_devices() {
        Ok(devices) => {
            devices_found = devices.len();
            checks.push(DoctorCheck {
                name: "usb_subsystem".into(),
                passed: true,
                message: "USB subsystem operational (libusb loaded)".into(),
                details: None,
            });

            // 2. Compatible hardware detection
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
                });

                // 3. Scanner access permissions
                match FutronicScanner::open_first() {
                    Ok(_) => {
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: true,
                            message: "USB communication permission confirmed".into(),
                            details: None,
                        });
                    }
                    Err(err) => {
                        healthy = false;
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: false,
                            message: "Permission error opening USB scanner".into(),
                            details: Some(format!(
                                "{err}. On Linux, check udev rules. On macOS, verify USB permissions."
                            )),
                        });
                    }
                }
            } else {
                checks.push(DoctorCheck {
                    name: "device_detection".into(),
                    passed: false,
                    message: "No Futronic FS80/FS81 scanner detected on USB bus".into(),
                    details: Some("Connect the scanner or check the USB cable.".into()),
                });
                checks.push(DoctorCheck {
                    name: "device_access".into(),
                    passed: true,
                    message: "Skipped (no scanner available for communication test)".into(),
                    details: None,
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
            });
        }
    }

    // 4. Write permission in current working directory
    let test_file = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!(".fs80h_test_{}", std::process::id()));
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            checks.push(DoctorCheck {
                name: "filesystem_write".into(),
                passed: true,
                message: "Write permission confirmed in current working directory".into(),
                details: None,
            });
        }
        Err(err) => {
            healthy = false;
            checks.push(DoctorCheck {
                name: "filesystem_write".into(),
                passed: false,
                message: "No write permission in current working directory".into(),
                details: Some(err.to_string()),
            });
        }
    }

    DoctorReport {
        healthy,
        checks,
        devices_found,
    }
}

fn selected(sel: &Selection) -> Result<FutronicScanner> {
    match (sel.bus, sel.address) {
        (Some(bus), Some(address)) => {
            let info = list_devices()?
                .into_iter()
                .find(|d| d.bus_number == bus && d.address == address)
                .context("requested scanner not found")?;
            Ok(FutronicScanner::open(info)?)
        }
        (None, None) => Ok(FutronicScanner::open_first()?),
        _ => anyhow::bail!("--bus and --address must be used together"),
    }
}

fn print<T: Serialize>(json: bool, value: &T) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Devices => {
            let devices = list_devices()?;
            if cli.json {
                print(true, &devices)?
            } else if devices.is_empty() {
                println!("No Futronic FS80/FS80H scanners found.")
            } else {
                for d in devices {
                    println!(
                        "{:04x}:{:04x} bus {} address {} model {}",
                        d.vendor_id, d.product_id, d.bus_number, d.address, d.model
                    )
                }
            }
        }
        Command::Device {
            command: DeviceCommand::Inspect(selection),
        } => {
            let scanner = selected(&selection)?;
            let value = serde_json::json!({"device": scanner.device_info()});
            if cli.json {
                print(true, &value)?
            } else {
                println!("Device opened successfully");
            }
        }
        Command::Capture(args) => {
            let mut scanner = selected(&args.selection)?;
            let raw_frame = if args.variable_dose {
                scanner.capture_with_dose(args.dose)?
            } else {
                scanner.capture()?
            };
            let quality = evaluate_quality(&raw_frame);
            let sha256_raw = raw_frame.sha256();

            let format = args
                .format
                .or_else(|| ImageFormat::from_path_extension(&args.output))
                .unwrap_or(ImageFormat::Png);

            // Industry-standard AFIS presentation (dark ridges on light background).
            // If --raw is provided, export the raw optical sensor signal without inversion.
            let standard_afis = !args.raw;
            let export_frame = if standard_afis {
                raw_frame.inverted()
            } else {
                raw_frame.clone()
            };

            let manifest = CaptureManifest::for_frame_with_afis_and_format(
                &raw_frame,
                quality.clone(),
                env!("CARGO_PKG_VERSION"),
                None,
                scanner.device_info(),
                standard_afis,
                format,
            );

            // Custody: sha256_raw_frame in the manifest records the hash of the raw sensor frame.
            export_frame.write_format(&args.output, format)?;
            manifest.write_adjacent(&args.output)?;

            let value = serde_json::json!({
                "file": args.output,
                "format": format.to_string(),
                "manifest": format!("{}.meta.json", args.output.display()),
                "sha256_raw_frame": sha256_raw,
                "standard_afis": standard_afis,
                "quality": quality
            });

            if cli.json {
                print(true, &value)?
            } else {
                println!("Capture saved successfully to {}", args.output.display());
                println!("File format: {}", format);
                println!(
                    "Visual presentation: {}",
                    if standard_afis {
                        "Standard AFIS (dark ridges on light background)"
                    } else {
                        "Raw optical sensor (light ridges on dark background)"
                    }
                );
                println!("Custody manifest: {}.meta.json", args.output.display());
                println!("SHA-256 (raw frame): {}", sha256_raw);
                println!(
                    "Frame quality: {}/100 ({})",
                    quality.score,
                    if quality.accepted {
                        "accepted"
                    } else {
                        "rejected: low quality"
                    }
                );
                println!(
                    "  - Mean local variance: {:.1} (minimum: 80.0)",
                    quality.mean_local_variance
                );
                println!(
                    "  - Saturation: {:.1}% (maximum tolerated: 20.0%)",
                    quality.saturated_ratio * 100.0
                );
                for w in &quality.warnings {
                    println!("  Quality warning: {w}");
                }
            }
        }
        Command::Convert(args) => {
            let input_frame =
                RawFrame::from_file(&args.input).context("failed to load input image")?;

            let target_format = match (args.to, &args.output) {
                (Some(fmt), _) => fmt,
                (None, Some(out)) => ImageFormat::from_path_extension(out).ok_or_else(|| {
                    anyhow::anyhow!("could not infer format from {}", out.display())
                })?,
                (None, None) => {
                    anyhow::bail!("specify --to <format> or --output <file.ext>")
                }
            };

            let output_path = match args.output {
                Some(out) => out,
                None => {
                    let mut out = args.input.clone();
                    out.set_extension(target_format.to_string());
                    out
                }
            };

            let converted_frame = if args.invert {
                input_frame.inverted()
            } else {
                input_frame.clone()
            };

            converted_frame.write_format(&output_path, target_format)?;

            // Custody manifest preservation
            let sha256_original_raw =
                if let Ok(parent_manifest) = Manifest::read_adjacent(&args.input) {
                    parent_manifest.sha256_raw_frame().to_string()
                } else {
                    input_frame.sha256()
                };

            let mut transformations = Vec::new();
            if args.invert {
                transformations.push("invert_polarity".to_string());
            }

            let derived = DerivedManifest {
                schema_version: 1,
                sha256_derived_artifact: converted_frame.sha256(),
                sha256_raw_frame: sha256_original_raw.clone(),
                format: target_format.to_string(),
                transformations,
            };
            derived.write_adjacent(&output_path)?;

            let value = serde_json::json!({
                "input": args.input,
                "output": output_path,
                "format": target_format.to_string(),
                "sha256_raw_frame": sha256_original_raw,
                "sha256_converted": converted_frame.sha256(),
                "manifest": format!("{}.meta.json", output_path.display())
            });

            if cli.json {
                print(true, &value)?;
            } else {
                println!(
                    "Conversion completed successfully: {} -> {}",
                    args.input.display(),
                    output_path.display()
                );
                println!("Output format: {}", target_format);
                println!("SHA-256 of generated image: {}", converted_frame.sha256());
                println!(
                    "Custody SHA-256 (original raw frame): {}",
                    sha256_original_raw
                );
                println!(
                    "Derived manifest generated: {}.meta.json",
                    output_path.display()
                );
            }
        }
        Command::Inspect(args) => {
            let format = ImageFormat::detect(&args.image)
                .context("failed to detect file format")?;
            let frame = RawFrame::from_file(&args.image).context("failed to read image data")?;
            let quality = evaluate_quality(&frame);
            let sha256 = frame.sha256();

            let manifest = Manifest::read_adjacent(&args.image).ok();

            let value = serde_json::json!({
                "file": args.image,
                "format": format.to_string(),
                "dimensions": {
                    "width": WIDTH,
                    "height": HEIGHT,
                    "dpi": DPI,
                    "bytes": FRAME_LEN
                },
                "sha256": sha256,
                "quality": quality,
                "manifest_present": manifest.is_some(),
                "manifest": manifest
            });

            if cli.json {
                print(true, &value)?;
            } else {
                println!("Technical Biometric Image Inspection");
                println!("===================================");
                println!("File: {}", args.image.display());
                println!("Detected format: {}", format);
                println!(
                    "Resolution: {}x{} pixels @ {} DPI ({} bytes)",
                    WIDTH, HEIGHT, DPI, FRAME_LEN
                );
                println!("Buffer SHA-256: {}", sha256);
                println!(
                    "Frame quality: {}/100 ({})",
                    quality.score,
                    if quality.accepted {
                        "accepted"
                    } else {
                        "low quality"
                    }
                );
                println!("  - Local variance: {:.1}", quality.mean_local_variance);
                println!("  - Saturation: {:.1}%", quality.saturated_ratio * 100.0);
                if let Some(m) = &manifest {
                    println!("Custody manifest: Present");
                    println!("  - Original SHA-256 (custody): {}", m.sha256_raw_frame());
                    match m {
                        Manifest::Capture(c) => {
                            println!("  - Type: Direct capture (hardware)");
                            println!("  - UTC timestamp: {}", c.captured_at_utc);
                        }
                        Manifest::Derived(d) => {
                            println!("  - Type: Derived / Transcoded");
                            if !d.transformations.is_empty() {
                                println!("  - Transformations: {}", d.transformations.join(", "));
                            }
                        }
                    }
                } else {
                    println!("Custody manifest: Missing (.meta.json not found)");
                }
            }
        }
        Command::Verify(args) => {
            if !args.threshold.is_finite() || !(0.0..=100.0).contains(&args.threshold) {
                anyhow::bail!("--threshold must be between 0 and 100");
            }
            let probe = RawFrame::from_file(&args.probe).context("failed to load probe")?;
            let reference =
                RawFrame::from_file(&args.reference).context("failed to load reference")?;
            let deterministic = compare_frames(&probe, &reference);
            let out = VerifyOutput {
                threshold: args.threshold,
                threshold_met: deterministic.score >= args.threshold,
                deterministic,
                biometric_score: None,
            };
            if cli.json {
                print(true, &out)?
            } else {
                println!(
                    "Deterministic score: {:.1}/100 ({})",
                    out.deterministic.score,
                    if out.threshold_met {
                        "meets threshold"
                    } else {
                        "below threshold"
                    }
                );
                for w in &out.deterministic.warnings {
                    println!("Warning: {w}")
                }
            }
            if args.exit_code && !out.threshold_met {
                std::process::exit(1);
            }
        }
        Command::Doctor => {
            let report = run_doctor();
            if cli.json {
                print(true, &report)?;
            } else {
                println!("fs80hctl Diagnostics");
                println!("====================");
                for check in &report.checks {
                    let icon = if check.passed { "✓" } else { "!" };
                    println!("[{icon}] {}: {}", check.name, check.message);
                    if let Some(details) = &check.details {
                        println!("    Details: {details}");
                    }
                }
                println!();
                if report.healthy {
                    if report.devices_found > 0 {
                        println!("Overall status: READY (scanner connected and operational)");
                    } else {
                        println!("Overall status: OPERATIONAL (waiting for USB scanner connection)");
                    }
                } else {
                    println!("Overall status: WARNING (operational issues detected)");
                }
            }
        }
        Command::Manual => {
            if cli.json {
                print(true, &serde_json::json!({ "manual": MANUAL_TEXT.trim() }))?;
            } else {
                println!("{MANUAL_TEXT}");
            }
        }
    };
    Ok(())
}

const MANUAL_TEXT: &str = r#"fs80hctl(1) - Futronic FS80/FS80H Technical Control Manual

NAME
    fs80hctl - Technical acquisition, custody, and processing utility for Futronic FS80/FS80H scanners

SYNOPSIS
    fs80hctl [GLOBAL_OPTIONS] <COMMAND> [ARGUMENTS...]

DESCRIPTION
    fs80hctl is a pure-Rust command-line tool for technical and forensic operations on
    Futronic optical fingerprint scanners (FS80, FS80H, and FS81H). It provides direct optical
    sensor acquisition, biometric standard compliance (500 DPI), image quality metrics
    (local variance and saturation), format transcoding with immutable chain of custody
    (SHA-256 manifests), and deterministic 1:1 biometric comparison.

GLOBAL OPTIONS
    --json
        Output results in structured JSON format (suitable for automation, scripts, and pipelines).

    -h, --help
        Print quick help message or subcommand syntax.

    -V, --version
        Print installed fs80hctl version.

SUBCOMMANDS
    doctor
        Verify runtime environment: USB subsystem (libusb), scanner access permissions,
        and current working directory write availability.
        Examples:
            fs80hctl doctor
            fs80hctl --json doctor

    devices
        List all connected Futronic FS80/FS80H/FS81H scanners on the USB bus.
        Examples:
            fs80hctl devices
            fs80hctl --json devices

    device inspect [--bus <N>] [--address <N>]
        Open specified device (or first available) and inspect USB communication.
        Examples:
            fs80hctl device inspect
            fs80hctl device inspect --bus 1 --address 4

    capture [OPTIONS]
        Acquire a biometric frame from optical sensor (320x480 pixels @ 500 DPI, L8 grayscale).
        Writes image file and adjacent chain-of-custody manifest (.meta.json).
        Capture options:
            -o, --output <FILE>       Output path (default: capture.png)
            --format <FORMAT>         Image format: png, tiff, pgm, raw
            --raw                     Save raw optical sensor frame without ridge inversion
            --variable-dose           Direct variable-dose acquisition path
            --dose <VALUE>            Exposure dose (default: 166)
            --bus <N> / --address <N> Select specific USB device
        Examples:
            fs80hctl capture --output digital.png
            fs80hctl capture --format tiff --output digital.tiff
            fs80hctl capture --format pgm --output nist.pgm
            fs80hctl capture --raw --output raw_sensor.png

    convert <INPUT> [OPTIONS]
        Transcode biometric images between formats, preserving root custody SHA-256 in the
        adjacent .meta.json manifest.
        Conversion options:
            -t, --to <FORMAT>         Target format: png, tiff, pgm, raw
            -o, --output <FILE>       Output file path
            --invert                  Invert pixel polarity (255 - pixel)
        Examples:
            fs80hctl convert capture.png --to tiff --output forensic.tiff
            fs80hctl convert forensic.tiff --to pgm --output print.pgm
            fs80hctl convert forensic.tiff --to raw --output frame.raw

    inspect <IMAGE>
        Inspect biometric image metadata: dimensions, DPI, raw buffer SHA-256 hash,
        quality metrics (local variance and saturation), and custody manifest status.
        Examples:
            fs80hctl inspect capture.png
            fs80hctl --json inspect forensic.tiff

    verify <PROBE> <REFERENCE> [OPTIONS]
        Compare two biometric images (1:1) using deterministic spatial feature extraction
        and alignment across any format combination.
        Options:
            --threshold <0-100>       Minimum verification score threshold (default: 40.0)
            --exit-code               Exit with code 1 if threshold is not met
        Examples:
            fs80hctl verify capture.png reference.tiff
            fs80hctl verify capture.png reference.tiff --threshold 45 --exit-code
            fs80hctl --json verify probe.pgm ref.png

    manual
        Display this technical reference manual on standard output.

SUPPORTED FORMATS
    PNG     Standard AFIS presentation (dark ridges, light background), 500 DPI pHYs chunk.
    TIFF    Baseline 6.0 L8 grayscale, full forensic EXIF tags (Make, Model, Software, 500 DPI).
    PGM     Netpbm binary P5, native format for NIST NBIS tools (mindtct, bozorth3).
    RAW/DAT Uncompressed 153,600-byte L8 buffer for NumPy, PyTorch, and ML pipelines.

CHAIN OF CUSTODY
    Every saved or converted image preserves an adjacent <IMAGE>.meta.json manifest.
    The manifest records the SHA-256 of the raw optical frame at acquisition, UTC timestamp,
    scanner model, and transformation lineage.

EXIT CODES
    0   Success.
    1   Threshold not met (when using --exit-code) or operational/diagnostic failure.
    2   Invalid command-line syntax or arguments (clap).
"#;
