mod cli;
mod doctor;

use clap::Parser;
use cli::{
    CaptureArgs, Cli, CliError, CliExitCode, Command, ConvertArgs, DeviceCommand, EnrollArgs,
    InspectArgs, MANUAL_TEXT, OutputFormat, Selection, VerifyArgs, VerifyOutput,
};
use doctor::run_doctor;
use fs80h_core::{
    CaptureManifest, DPI, DerivedManifest, FRAME_LEN, FingerprintScanner, HEIGHT, ImageFormat,
    Manifest, RawFrame, WIDTH, compare_frames, compare_probe_to_gallery, evaluate_quality,
};
use fs80h_driver::{DriverError, FutronicScanner, list_devices};
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Serialize)]
struct JsonErrorResponse<'a> {
    status: &'static str,
    code: u8,
    message: &'a str,
}

fn init_tracing(verbose_count: u8) {
    let default_level = match verbose_count {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .try_init();
}

fn print_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let s = serde_json::to_string_pretty(value)
        .map_err(|e| CliError::Operational(format!("JSON serialization error: {e}")))?;
    println!("{s}");
    Ok(())
}

fn selected(sel: &Selection) -> Result<FutronicScanner, CliError> {
    match (sel.bus, sel.address) {
        (Some(bus), Some(address)) => {
            let devs = list_devices().map_err(|e| CliError::Hardware(e.to_string()))?;
            let info = devs
                .into_iter()
                .find(|d| d.bus_number == bus && d.address == address)
                .ok_or_else(|| {
                    CliError::Hardware(format!(
                        "requested scanner at bus {bus} address {address} not found"
                    ))
                })?;
            FutronicScanner::open(info).map_err(map_driver_error)
        }
        (None, None) => FutronicScanner::open_first().map_err(map_driver_error),
        _ => Err(CliError::Operational(
            "--bus and --address must be used together".into(),
        )),
    }
}

fn map_driver_error(err: DriverError) -> CliError {
    match err {
        DriverError::Timeout(d) => CliError::Timeout(format!("{d:?}")),
        DriverError::Native(native_err) => CliError::Hardware(native_err.to_string()),
        DriverError::Core(core_err) => CliError::Operational(core_err.to_string()),
    }
}

fn execute(cli: &Cli, format: OutputFormat) -> Result<(), CliError> {
    match &cli.command {
        Command::Devices => {
            let devices = list_devices().map_err(|e| CliError::Hardware(e.to_string()))?;
            match format {
                OutputFormat::Json => print_json(&devices)?,
                OutputFormat::Text | OutputFormat::Raw => {
                    if devices.is_empty() {
                        println!("No Futronic FS80/FS80H scanners found.");
                    } else {
                        for d in devices {
                            println!(
                                "{:04x}:{:04x} bus {} address {} model {}",
                                d.vendor_id, d.product_id, d.bus_number, d.address, d.model
                            );
                        }
                    }
                }
            }
        }

        Command::Device {
            command: DeviceCommand::Inspect(selection),
        } => {
            let scanner = selected(selection)?;
            let value = serde_json::json!({"device": scanner.device_info()});
            match format {
                OutputFormat::Json => print_json(&value)?,
                OutputFormat::Text | OutputFormat::Raw => {
                    println!("Device opened successfully");
                }
            }
        }

        Command::Capture(args) => {
            handle_capture(args, format)?;
        }

        Command::Enroll(args) => {
            handle_enroll(args, format)?;
        }

        Command::Convert(args) => {
            handle_convert(args, format)?;
        }

        Command::Inspect(args) => {
            handle_inspect(args, format)?;
        }

        Command::Verify(args) => {
            handle_verify(args, format)?;
        }

        Command::Doctor => {
            let report = run_doctor();
            match format {
                OutputFormat::Json => print_json(&report)?,
                OutputFormat::Text | OutputFormat::Raw => {
                    println!("fs80hctl Diagnostics");
                    println!("====================");
                    for check in &report.checks {
                        let icon = if check.passed { "✓" } else { "!" };
                        println!("[{icon}] {}: {}", check.name, check.message);
                        if let Some(details) = &check.details {
                            println!("    Details: {details}");
                        }
                        if let Some(suggestion) = &check.suggestion {
                            println!(
                                "    Suggestion:\n      {}",
                                suggestion.replace('\n', "\n      ")
                            );
                        }
                    }
                    println!();
                    if report.healthy {
                        if report.devices_found > 0 {
                            println!("Overall status: READY (scanner connected and operational)");
                        } else {
                            println!(
                                "Overall status: OPERATIONAL (waiting for USB scanner connection)"
                            );
                        }
                    } else {
                        println!("Overall status: WARNING (operational issues detected)");
                    }
                }
            }
        }

        Command::Manual => match format {
            OutputFormat::Json => {
                print_json(&serde_json::json!({ "manual": MANUAL_TEXT.trim() }))?;
            }
            OutputFormat::Text | OutputFormat::Raw => {
                println!("{MANUAL_TEXT}");
            }
        },
    }
    Ok(())
}

fn handle_capture(args: &CaptureArgs, format: OutputFormat) -> Result<(), CliError> {
    let mut scanner = selected(&args.selection)?;
    let raw_frame = if args.wait {
        let dose_opt = if args.variable_dose {
            Some(args.dose)
        } else {
            None
        };

        eprintln!(
            "Waiting for finger on scanner (timeout: {}s, min quality: {})...",
            args.timeout, args.min_quality
        );

        let frame = scanner
            .wait_for_finger(
                std::time::Duration::from_secs(args.timeout),
                args.min_quality,
                dose_opt,
                |presence, elapsed| {
                    if presence.is_present {
                        eprint!(
                            "\r[contact: {:.0}%, score: {}/100, elapsed: {:.1}s] Stabilizing...",
                            presence.contact_ratio * 100.0,
                            presence.quality.score,
                            elapsed.as_secs_f32()
                        );
                        let _ = std::io::stderr().flush();
                    }
                },
            )
            .map_err(map_driver_error)?;

        eprintln!();
        frame
    } else if args.variable_dose {
        scanner
            .capture_with_dose(args.dose)
            .map_err(map_driver_error)?
    } else {
        scanner.capture().map_err(map_driver_error)?
    };

    let quality = evaluate_quality(&raw_frame);
    let sha256_raw = raw_frame.sha256();

    let img_format = args
        .image_format
        .or_else(|| ImageFormat::from_path_extension(&args.output))
        .unwrap_or(ImageFormat::Png);

    let standard_afis = !args.raw;
    let export_frame = if standard_afis {
        raw_frame.inverted()
    } else {
        raw_frame.clone()
    };

    // If global output format is RAW, stream the raw 153,600 bytes directly to stdout for Unix pipes
    if format == OutputFormat::Raw {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(export_frame.pixels()).map_err(|e| {
            CliError::Operational(format!("failed to stream raw bytes to stdout: {e}"))
        })?;
        stdout
            .flush()
            .map_err(|e| CliError::Operational(format!("failed to flush stdout: {e}")))?;
        return Ok(());
    }

    let manifest = CaptureManifest::for_frame_with_afis_and_format(
        &raw_frame,
        quality.clone(),
        env!("CARGO_PKG_VERSION"),
        None,
        scanner.device_info(),
        standard_afis,
        img_format,
    );

    export_frame
        .write_format(&args.output, img_format)
        .map_err(|e| CliError::Operational(format!("failed to write image file: {e}")))?;
    manifest
        .write_adjacent(&args.output)
        .map_err(|e| CliError::Operational(format!("failed to write manifest: {e}")))?;

    let value = serde_json::json!({
        "file": args.output,
        "format": img_format.to_string(),
        "manifest": format!("{}.meta.json", args.output.display()),
        "sha256_raw_frame": sha256_raw,
        "standard_afis": standard_afis,
        "quality": quality
    });

    match format {
        OutputFormat::Json => print_json(&value)?,
        OutputFormat::Text => {
            println!("Capture saved successfully to {}", args.output.display());
            println!("File format: {}", img_format);
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
        OutputFormat::Raw => unreachable!(),
    }

    Ok(())
}

fn handle_enroll(args: &EnrollArgs, format: OutputFormat) -> Result<(), CliError> {
    if args.samples == 0 {
        return Err(CliError::Operational(
            "--samples must be greater than 0".into(),
        ));
    }
    let mut scanner = selected(&args.selection)?;
    let dose_opt = if args.variable_dose {
        Some(args.dose)
    } else {
        None
    };
    let mut samples_data = Vec::with_capacity(args.samples);

    eprintln!(
        "Starting Enrollment: Acquiring {} biometric samples",
        args.samples
    );
    eprintln!("==================================================");

    for idx in 1..=args.samples {
        eprintln!(
            "Sample {}/{}: Place finger firmly on scanner...",
            idx, args.samples
        );
        let frame = scanner
            .wait_for_finger(
                std::time::Duration::from_secs(args.timeout),
                args.min_quality,
                dose_opt,
                |presence, elapsed| {
                    if presence.is_present {
                        eprint!(
                            "\r  [contact: {:.0}%, score: {}/100, elapsed: {:.1}s] Acquiring...",
                            presence.contact_ratio * 100.0,
                            presence.quality.score,
                            elapsed.as_secs_f32()
                        );
                        let _ = std::io::stderr().flush();
                    }
                },
            )
            .map_err(map_driver_error)?;

        eprintln!();
        let quality = evaluate_quality(&frame);
        eprintln!(
            "  -> Sample {} captured: quality {}/100 (variance: {:.1}, saturation: {:.1}%)",
            idx,
            quality.score,
            quality.mean_local_variance,
            quality.saturated_ratio * 100.0
        );
        samples_data.push((idx, frame, quality));

        if idx < args.samples {
            eprintln!("  Please lift your finger before the next take...");
            std::thread::sleep(std::time::Duration::from_millis(1200));
        }
    }

    let (best_idx, best_frame, best_quality) = samples_data
        .into_iter()
        .max_by(|(_, _, q1), (_, _, q2)| {
            q1.score.cmp(&q2.score).then_with(|| {
                q1.mean_local_variance
                    .partial_cmp(&q2.mean_local_variance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
        .ok_or_else(|| CliError::Operational("no samples acquired".into()))?;

    let img_format = args
        .image_format
        .or_else(|| ImageFormat::from_path_extension(&args.output))
        .unwrap_or(ImageFormat::Png);

    let standard_afis = !args.raw;
    let export_frame = if standard_afis {
        best_frame.inverted()
    } else {
        best_frame.clone()
    };

    if format == OutputFormat::Raw {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(export_frame.pixels())
            .map_err(|e| CliError::Operational(format!("failed to stream raw bytes: {e}")))?;
        stdout
            .flush()
            .map_err(|e| CliError::Operational(format!("failed to flush stdout: {e}")))?;
        return Ok(());
    }

    let manifest = CaptureManifest::for_frame_with_afis_and_format(
        &best_frame,
        best_quality.clone(),
        env!("CARGO_PKG_VERSION"),
        None,
        scanner.device_info(),
        standard_afis,
        img_format,
    );

    export_frame
        .write_format(&args.output, img_format)
        .map_err(|e| CliError::Operational(format!("failed to write image file: {e}")))?;
    manifest
        .write_adjacent(&args.output)
        .map_err(|e| CliError::Operational(format!("failed to write manifest: {e}")))?;

    let sha256_raw = best_frame.sha256();
    let value = serde_json::json!({
        "enrollment": {
            "total_samples": args.samples,
            "selected_sample": best_idx,
            "quality": best_quality,
            "sha256_raw_frame": sha256_raw,
            "file": args.output,
            "format": img_format.to_string(),
            "manifest": format!("{}.meta.json", args.output.display()),
            "standard_afis": standard_afis
        }
    });

    match format {
        OutputFormat::Json => print_json(&value)?,
        OutputFormat::Text => {
            println!();
            println!("Enrollment completed successfully!");
            println!(
                "Selected best sample: #{} with quality {}/100",
                best_idx, best_quality.score
            );
            println!("Saved to: {}", args.output.display());
            println!("Format: {}", img_format);
            println!("Custody manifest: {}.meta.json", args.output.display());
            println!("SHA-256 (raw frame): {}", sha256_raw);
        }
        OutputFormat::Raw => unreachable!(),
    }

    Ok(())
}

fn handle_convert(args: &ConvertArgs, format: OutputFormat) -> Result<(), CliError> {
    let input_frame = RawFrame::from_file(&args.input)
        .map_err(|e| CliError::Operational(format!("failed to load input image: {e}")))?;

    let target_format = match (args.to, &args.output) {
        (Some(fmt), _) => fmt,
        (None, Some(out)) => ImageFormat::from_path_extension(out).ok_or_else(|| {
            CliError::Operational(format!("could not infer format from {}", out.display()))
        })?,
        (None, None) => {
            return Err(CliError::Operational(
                "specify --to <format> or --output <file.ext>".into(),
            ));
        }
    };

    let output_path = match &args.output {
        Some(out) => out.clone(),
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

    if format == OutputFormat::Raw {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(converted_frame.pixels())
            .map_err(|e| CliError::Operational(format!("failed to stream raw bytes: {e}")))?;
        stdout
            .flush()
            .map_err(|e| CliError::Operational(format!("failed to flush stdout: {e}")))?;
        return Ok(());
    }

    converted_frame
        .write_format(&output_path, target_format)
        .map_err(|e| CliError::Operational(format!("failed to write converted image: {e}")))?;

    let sha256_original_raw = if let Ok(parent_manifest) = Manifest::read_adjacent(&args.input) {
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
    derived
        .write_adjacent(&output_path)
        .map_err(|e| CliError::Operational(format!("failed to write derived manifest: {e}")))?;

    let value = serde_json::json!({
        "input": args.input,
        "output": output_path,
        "format": target_format.to_string(),
        "sha256_raw_frame": sha256_original_raw,
        "sha256_converted": converted_frame.sha256(),
        "manifest": format!("{}.meta.json", output_path.display())
    });

    match format {
        OutputFormat::Json => print_json(&value)?,
        OutputFormat::Text => {
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
        OutputFormat::Raw => unreachable!(),
    }

    Ok(())
}

fn handle_inspect(args: &InspectArgs, format: OutputFormat) -> Result<(), CliError> {
    let img_format = ImageFormat::detect(&args.image)
        .map_err(|e| CliError::Operational(format!("failed to detect file format: {e}")))?;
    let frame = RawFrame::from_file(&args.image)
        .map_err(|e| CliError::Operational(format!("failed to read image data: {e}")))?;
    let quality = evaluate_quality(&frame);
    let sha256 = frame.sha256();

    let manifest = Manifest::read_adjacent(&args.image).ok();

    let value = serde_json::json!({
        "file": args.image,
        "format": img_format.to_string(),
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

    match format {
        OutputFormat::Json => print_json(&value)?,
        OutputFormat::Text | OutputFormat::Raw => {
            println!("Technical Biometric Image Inspection");
            println!("===================================");
            println!("File: {}", args.image.display());
            println!("Detected format: {}", img_format);
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

    Ok(())
}

fn handle_verify(args: &VerifyArgs, format: OutputFormat) -> Result<(), CliError> {
    if !args.threshold.is_finite() || !(0.0..=100.0).contains(&args.threshold) {
        return Err(CliError::Operational(
            "--threshold must be between 0 and 100".into(),
        ));
    }
    let probe = RawFrame::from_file(&args.probe)
        .map_err(|e| CliError::Operational(format!("failed to load probe: {e}")))?;

    match (&args.reference, &args.gallery) {
        (Some(ref_path), None) => {
            let reference = RawFrame::from_file(ref_path)
                .map_err(|e| CliError::Operational(format!("failed to load reference: {e}")))?;
            let deterministic = compare_frames(&probe, &reference);
            let threshold_met = deterministic.score >= args.threshold;
            let out = VerifyOutput {
                threshold: args.threshold,
                threshold_met,
                deterministic,
                biometric_score: None,
            };

            match format {
                OutputFormat::Json => print_json(&out)?,
                OutputFormat::Text | OutputFormat::Raw => {
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
                        println!("Warning: {w}");
                    }
                }
            }

            if args.exit_code && !threshold_met {
                return Err(CliError::Mismatch {
                    score: out.deterministic.score,
                    threshold: args.threshold,
                });
            }
        }

        (None, Some(gallery_dir)) => {
            if !gallery_dir.is_dir() {
                return Err(CliError::Operational(format!(
                    "gallery path is not a directory: {}",
                    gallery_dir.display()
                )));
            }
            let mut candidates = Vec::new();
            let mut entries = std::fs::read_dir(gallery_dir)
                .map_err(|e| {
                    CliError::Operational(format!("failed to read gallery directory: {e}"))
                })?
                .filter_map(|e| e.ok())
                .collect::<Vec<_>>();
            entries.sort_by_key(|e| e.path());

            for entry in entries {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if file_name.ends_with(".meta.json") {
                        continue;
                    }
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        match ext.to_ascii_lowercase().as_str() {
                            "png" | "tiff" | "tif" | "pgm" | "raw" | "dat" => {
                                if let Ok(frame) = RawFrame::from_file(&path) {
                                    candidates.push((path.display().to_string(), frame));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            if candidates.is_empty() {
                return Err(CliError::Operational(format!(
                    "no valid biometric image files found in gallery directory: {}",
                    gallery_dir.display()
                )));
            }

            let report = compare_probe_to_gallery(&probe, &candidates, args.threshold);

            match format {
                OutputFormat::Json => print_json(&report)?,
                OutputFormat::Text | OutputFormat::Raw => {
                    println!("1:N Gallery Verification Report");
                    println!("===============================");
                    println!("Probe image: {}", args.probe.display());
                    println!(
                        "Gallery directory: {} ({} candidates)",
                        gallery_dir.display(),
                        report.total_candidates
                    );
                    println!("Threshold: {:.1}%", args.threshold);
                    println!();
                    println!(
                        "{:<4} | {:<32} | {:<7} | Status",
                        "Rank", "Candidate File", "Score"
                    );
                    println!(
                        "{:-<4}-+-{:-<32}-+-{:-<7}-+--------------------",
                        "", "", ""
                    );

                    for (idx, c) in report.matches.iter().enumerate() {
                        let status = if c.matched { "MATCH" } else { "NO MATCH" };
                        let display_name = PathBuf::from(&c.reference_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&c.reference_path)
                            .to_string();
                        let truncated = if display_name.len() > 32 {
                            format!("{}...", &display_name[..29])
                        } else {
                            display_name
                        };
                        println!(
                            "{:<4} | {:<32} | {:>5.1} | {}",
                            idx + 1,
                            truncated,
                            c.score,
                            status
                        );
                    }

                    println!();
                    if let Some(best) = &report.best_match {
                        println!(
                            "Best candidate: {} (score: {:.1}/100) -> {}",
                            best.reference_path,
                            best.score,
                            if best.matched {
                                "VERIFIED (meets threshold)"
                            } else {
                                "BELOW THRESHOLD"
                            }
                        );
                    }
                }
            }

            if args.exit_code && !report.threshold_met {
                let best_score = report.best_match.as_ref().map(|b| b.score).unwrap_or(0.0);
                return Err(CliError::Mismatch {
                    score: best_score,
                    threshold: args.threshold,
                });
            }
        }

        (Some(_), Some(_)) => {
            return Err(CliError::Operational(
                "specify either <REFERENCE> for 1:1 or --gallery <DIR> for 1:N, not both".into(),
            ));
        }
        (None, None) => {
            return Err(CliError::Operational(
                "specify a reference file for 1:1 verification or --gallery <DIR> for 1:N".into(),
            ));
        }
    }

    Ok(())
}

fn main() -> ExitCode {
    // Try parsing arguments with custom error output
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(err) => {
            if err.use_stderr() {
                eprintln!("{err}");
                return CliExitCode::OperationalError.into();
            } else {
                print!("{err}");
                return CliExitCode::Success.into();
            }
        }
    };

    init_tracing(cli.verbose);
    let format = cli.resolved_format();

    match execute(&cli, format) {
        Ok(()) => CliExitCode::Success.into(),
        Err(err) => {
            let code = err.exit_code();
            let err_msg = err.to_string();

            if format == OutputFormat::Json {
                let json_err = JsonErrorResponse {
                    status: "error",
                    code: code.as_u8(),
                    message: &err_msg,
                };
                if let Ok(serialized) = serde_json::to_string_pretty(&json_err) {
                    eprintln!("{serialized}");
                } else {
                    eprintln!("error: {err_msg}");
                }
            } else {
                eprintln!("error: {err_msg}");
            }

            code.into()
        }
    }
}
