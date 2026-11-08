use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use fs80h_core::{CaptureManifest, FingerprintScanner, RawFrame, compare_frames, evaluate_quality};
use fs80h_driver::{FutronicScanner, list_devices};
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
    biometric_score: Option<f32>,
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
                serde_json::json!({"device":scanner.device_info()});
            if cli.json {
                print(true, &value)?
            } else {
                println!("Dispositivo aberto com sucesso");
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
                None,
                scanner.device_info(),
            );
            // The custody hash is materialized from the raw sensor frame before
            // any image container is written.
            frame.write_png(&args.output)?;
            manifest.write_adjacent(&args.output)?;
            if args.format != "png" {
                anyhow::bail!("formatos adicionais serão fornecidos por fs80h-formats");
            }
            if !args.derive.is_empty() {
                anyhow::bail!("derivadas serão fornecidas por fs80h-formats");
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
                    "Score determinístico: {:.1}/100 ({})",
                    out.deterministic.score,
                    if out.threshold_met {
                        "atinge limiar"
                    } else {
                        "abaixo do limiar"
                    }
                );
                for w in &out.deterministic.warnings {
                    println!("Aviso: {w}")
                }
            }
        }
    };
    Ok(())
}
