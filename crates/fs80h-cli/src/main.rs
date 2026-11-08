use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use fs80h_core::{
    CaptureManifest, DerivedManifest, FingerprintScanner, RawFrame, compare_frames,
    evaluate_quality, sha256_bytes,
};
use fs80h_driver::{FutronicScanner, abi_version, export_derived, list_devices, native_match};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "fs80hctl",
    about = "Controle técnico do leitor Futronic FS80/FS80H"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Devices,
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    Capture(CaptureArgs),
    Verify(VerifyArgs),
}
#[derive(Subcommand)]
enum DeviceCommand {
    Inspect(Selection),
}
#[derive(Args, Clone)]
struct Selection {
    #[arg(long)]
    bus: Option<u8>,
    #[arg(long)]
    address: Option<u8>,
}
#[derive(Args)]
struct CaptureArgs {
    #[command(flatten)]
    selection: Selection,
    #[arg(short, long, default_value = "capture.png")]
    output: PathBuf,
    #[arg(long)]
    auto_capture: bool,
    #[arg(long, default_value_t = 10_000)]
    timeout_ms: u64,
    /// Comma-separated non-destructive derivatives: inverted, normalized, enhanced.
    #[arg(long, value_delimiter = ',')]
    derive: Vec<String>,
    #[arg(long, default_value = "png", value_parser = ["png", "tiff", "pgm", "wsq"])]
    format: String,
}
#[derive(Args)]
struct VerifyArgs {
    probe: PathBuf,
    reference: PathBuf,
    #[arg(long, default_value_t = 40.0)]
    threshold: f32,
}
#[derive(Serialize)]
struct VerifyOutput {
    deterministic: fs80h_core::MatchReport,
    threshold: f32,
    threshold_met: bool,
    native_score: Option<f32>,
    native_warning: Option<String>,
}

fn selected(sel: &Selection) -> Result<FutronicScanner> {
    match (sel.bus, sel.address) {
        (Some(bus), Some(address)) => {
            let info = list_devices()?
                .into_iter()
                .find(|d| d.bus_number == bus && d.address == address)
                .context("leitor solicitado não encontrado")?;
            Ok(FutronicScanner::open(info)?)
        }
        (None, None) => Ok(FutronicScanner::open_first()?),
        _ => anyhow::bail!("--bus e --address devem ser usados juntos"),
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
                println!("Nenhum leitor FS80/FS80H encontrado.")
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
            let value =
                serde_json::json!({"abi_version":abi_version(),"device":scanner.device_info()});
            if cli.json {
                print(true, &value)?
            } else {
                println!("ABI {} — dispositivo aberto com sucesso", abi_version());
            }
        }
        Command::Capture(args) => {
            let mut scanner = selected(&args.selection)?;
            let frame = if args.auto_capture {
                scanner.auto_capture(args.timeout_ms)?
            } else {
                scanner.capture()?
            };
            let quality = evaluate_quality(&frame);
            let manifest = CaptureManifest::for_frame(
                &frame,
                quality.clone(),
                env!("CARGO_PKG_VERSION"),
                Some(abi_version()),
                scanner.device_info(),
            );
            // The custody hash is materialized from the raw sensor frame before
            // any image container is written.
            frame.write_png(&args.output)?;
            manifest.write_adjacent(&args.output)?;
            let extension = match args.format.as_str() {
                "png" => "png",
                "tiff" => "tiff",
                "pgm" => "pgm",
                "wsq" => "wsq",
                _ => unreachable!(),
            };
            let format_code = match args.format.as_str() {
                "png" => 1,
                "tiff" => 2,
                "pgm" => 3,
                "wsq" => 0,
                _ => unreachable!(),
            };
            for derived in &args.derive {
                if !matches!(derived.as_str(), "inverted" | "normalized" | "enhanced") {
                    anyhow::bail!("--derive aceita inverted, normalized ou enhanced");
                }
                let bytes = if args.format == "wsq" {
                    // WSQ is an export of the captured frame; derived WSQ is intentionally not emitted.
                    anyhow::bail!("WSQ não é um formato de derivada; use png, tiff ou pgm")
                } else {
                    export_derived(&frame, derived, format_code)?
                };
                let path = args.output.with_file_name(format!(
                    "{}.{}.{}",
                    args.output
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy(),
                    derived,
                    extension
                ));
                std::fs::write(&path, &bytes)?;
                DerivedManifest {
                    schema_version: 1,
                    sha256_derived_artifact: sha256_bytes(&bytes),
                    sha256_raw_frame: frame.sha256(),
                    format: args.format.clone(),
                    transformations: vec![derived.clone()],
                }
                .write_adjacent(&path)?;
            }
            let value = serde_json::json!({"png":args.output,"manifest":format!("{}.meta.json",args.output.display()),"sha256_raw_frame":frame.sha256(),"quality":quality});
            if cli.json {
                print(true, &value)?
            } else {
                println!("Captura gravada em {}", args.output.display());
            }
        }
        Command::Verify(args) => {
            if !args.threshold.is_finite() || !(0.0..=100.0).contains(&args.threshold) {
                anyhow::bail!("--threshold deve estar entre 0 e 100");
            }
            let probe = RawFrame::from_png(&args.probe).context("probe PNG inválido")?;
            let reference =
                RawFrame::from_png(&args.reference).context("reference PNG inválido")?;
            let deterministic = compare_frames(&probe, &reference);
            // The CLI's deterministic score and threshold are percentages. The
            // native ABI uses a normalized matcher threshold/score in [0.0, 1.0].
            let native = match native_match(&probe, &reference, args.threshold / 100.0) {
                Ok(score) => (Some(score * 100.0), None),
                Err(e) => (None, Some(format!("matcher nativo indisponível: {e}"))),
            };
            let out = VerifyOutput {
                threshold: args.threshold,
                threshold_met: deterministic.score >= args.threshold,
                deterministic,
                native_score: native.0,
                native_warning: native.1,
            };
            if cli.json {
                print(true, &out)?
            } else {
                println!(
                    "Score determinístico: {:.1}/100 ({})",
                    out.deterministic.score,
                    if out.threshold_met {
                        "atinge limiar"
                    } else {
                        "abaixo do limiar"
                    }
                );
                if let Some(n) = out.native_score {
                    println!("Score nativo adicional: {n:.1}")
                }
                if let Some(warning) = &out.native_warning {
                    println!("Aviso: {warning}")
                }
                for w in &out.deterministic.warnings {
                    println!("Aviso: {w}")
                }
            }
        }
    };
    Ok(())
}
