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
    Doctor,
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
    /// Salva o frame óptico bruto do sensor sem inversão (cristas claras em fundo escuro)
    #[arg(long)]
    raw: bool,
    /// Use the direct variable-dose acquisition path instead of frame mode.
    #[arg(long)]
    variable_dose: bool,
    /// Exposure dose used with --variable-dose.
    #[arg(long, default_value_t = 166)]
    dose: u8,
}
#[derive(Args)]
struct VerifyArgs {
    probe: PathBuf,
    reference: PathBuf,
    #[arg(long, default_value_t = 40.0)]
    threshold: f32,
    /// Retorna código de saída 1 caso o score não atinja o limiar
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

    // 1. Checagem do subsistema USB / libusb
    match list_devices() {
        Ok(devices) => {
            devices_found = devices.len();
            checks.push(DoctorCheck {
                name: "usb_subsystem".into(),
                passed: true,
                message: "Subsistema USB operacional (libusb carregado)".into(),
                details: None,
            });

            // 2. Detecção de hardware compatível
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
                    message: format!("{devices_found} leitor(es) Futronic detectado(s)"),
                    details: Some(dev_desc),
                });

                // 3. Permissão de acesso ao leitor
                match FutronicScanner::open_first() {
                    Ok(_) => {
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: true,
                            message: "Permissão de comunicação USB com o leitor confirmada".into(),
                            details: None,
                        });
                    }
                    Err(err) => {
                        healthy = false;
                        checks.push(DoctorCheck {
                            name: "device_access".into(),
                            passed: false,
                            message: "Falha de permissão ao abrir o leitor USB".into(),
                            details: Some(format!(
                                "{err}. No Linux, verifique as regras udev. No macOS, confirme permissões USB."
                            )),
                        });
                    }
                }
            } else {
                checks.push(DoctorCheck {
                    name: "device_detection".into(),
                    passed: false,
                    message: "Nenhum leitor Futronic FS80/FS81 detectado na porta USB".into(),
                    details: Some("Conecte o leitor ou verifique o cabo USB.".into()),
                });
                checks.push(DoctorCheck {
                    name: "device_access".into(),
                    passed: true,
                    message: "Ignorado (nenhum leitor disponível para teste de abertura)".into(),
                    details: None,
                });
            }
        }
        Err(err) => {
            healthy = false;
            checks.push(DoctorCheck {
                name: "usb_subsystem".into(),
                passed: false,
                message: "Falha ao inicializar subsistema USB".into(),
                details: Some(err.to_string()),
            });
        }
    }

    // 4. Permissão de escrita no diretório atual
    let test_file = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(format!(".fs80h_test_{}", std::process::id()));
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            checks.push(DoctorCheck {
                name: "filesystem_write".into(),
                passed: true,
                message: "Permissão de escrita no diretório de trabalho confirmada".into(),
                details: None,
            });
        }
        Err(err) => {
            healthy = false;
            checks.push(DoctorCheck {
                name: "filesystem_write".into(),
                passed: false,
                message: "Sem permissão de gravação no diretório de trabalho".into(),
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
            let value = serde_json::json!({"device":scanner.device_info()});
            if cli.json {
                print(true, &value)?
            } else {
                println!("Dispositivo aberto com sucesso");
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

            // Padrão AFIS da indústria biométrica (cristas escuras sobre fundo claro).
            // Se --raw for fornecido, exporta o sinal óptico bruto do sensor sem inversão.
            let standard_afis = !args.raw;
            let export_frame = if standard_afis {
                raw_frame.inverted()
            } else {
                raw_frame.clone()
            };

            let manifest = CaptureManifest::for_frame_with_afis(
                &raw_frame,
                quality.clone(),
                env!("CARGO_PKG_VERSION"),
                None,
                scanner.device_info(),
                standard_afis,
            );
            // Custódia: sha256_raw_frame no manifesto registra o hash do frame bruto do sensor.
            export_frame.write_png(&args.output)?;
            manifest.write_adjacent(&args.output)?;
            let value = serde_json::json!({
                "png": args.output,
                "manifest": format!("{}.meta.json", args.output.display()),
                "sha256_raw_frame": sha256_raw,
                "standard_afis": standard_afis,
                "quality": quality
            });
            if cli.json {
                print(true, &value)?
            } else {
                println!("Captura gravada com sucesso em {}", args.output.display());
                println!(
                    "Padrão visual: {}",
                    if standard_afis {
                        "AFIS padrão (cristas escuras em fundo claro)"
                    } else {
                        "Óptico bruto do sensor (cristas claras em fundo escuro)"
                    }
                );
                println!("Manifesto de custódia: {}.meta.json", args.output.display());
                println!("SHA-256 (frame bruto): {}", sha256_raw);
                println!(
                    "Qualidade do frame: {}/100 ({})",
                    quality.score,
                    if quality.accepted {
                        "aceita"
                    } else {
                        "rejeitada/baixa qualidade"
                    }
                );
                println!(
                    "  - Variância local média: {:.1} (mínimo: 80.0)",
                    quality.mean_local_variance
                );
                println!(
                    "  - Saturação: {:.1}% (máximo tolerado: 20.0%)",
                    quality.saturated_ratio * 100.0
                );
                for w in &quality.warnings {
                    println!("  Aviso de qualidade: {w}");
                }
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
            if args.exit_code && !out.threshold_met {
                std::process::exit(1);
            }
        }
        Command::Doctor => {
            let report = run_doctor();
            if cli.json {
                print(true, &report)?;
            } else {
                println!("Diagnóstico do fs80hctl");
                println!("======================");
                for check in &report.checks {
                    let icon = if check.passed { "✓" } else { "!" };
                    println!("[{icon}] {}: {}", check.name, check.message);
                    if let Some(details) = &check.details {
                        println!("    Detalhes: {details}");
                    }
                }
                println!();
                if report.healthy {
                    if report.devices_found > 0 {
                        println!("Status geral: PRONTO (scanner conectado e operacional)");
                    } else {
                        println!("Status geral: OPERACIONAL (aguardando conexão de leitor USB)");
                    }
                } else {
                    println!("Status geral: ATENÇÃO (foram detectadas pendências operacionais)");
                }
            }
        }
    };
    Ok(())
}
