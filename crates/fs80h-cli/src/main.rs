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
    Convert(ConvertArgs),
    Inspect(InspectArgs),
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
    /// Formato de imagem (png, tiff, pgm, raw). Se omitido, deduz da extensão de --output.
    #[arg(long)]
    format: Option<ImageFormat>,
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
struct ConvertArgs {
    /// Arquivo de imagem de entrada (PNG, TIFF, PGM, RAW)
    input: PathBuf,
    /// Formato de destino (png, tiff, pgm, raw). Se omitido, deduz da extensão de --output.
    #[arg(short = 't', long)]
    to: Option<ImageFormat>,
    /// Caminho do arquivo de saída
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Inverte a polaridade dos pixels (255 - pixel)
    #[arg(long)]
    invert: bool,
}

#[derive(Args)]
struct InspectArgs {
    /// Arquivo de imagem a ser inspecionado (PNG, TIFF, PGM, RAW)
    image: PathBuf,
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

            let format = args
                .format
                .or_else(|| ImageFormat::from_path_extension(&args.output))
                .unwrap_or(ImageFormat::Png);

            // Padrão AFIS da indústria biométrica (cristas escuras sobre fundo claro).
            // Se --raw for fornecido, exporta o sinal óptico bruto do sensor sem inversão.
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

            // Custódia: sha256_raw_frame no manifesto registra o hash do frame bruto do sensor.
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
                println!("Captura gravada com sucesso em {}", args.output.display());
                println!("Formato de arquivo: {}", format);
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
        Command::Convert(args) => {
            let input_frame =
                RawFrame::from_file(&args.input).context("falha ao carregar imagem de entrada")?;

            let target_format = match (args.to, &args.output) {
                (Some(fmt), _) => fmt,
                (None, Some(out)) => ImageFormat::from_path_extension(out).ok_or_else(|| {
                    anyhow::anyhow!(
                        "não foi possível deduzir formato a partir de {}",
                        out.display()
                    )
                })?,
                (None, None) => {
                    anyhow::bail!("especifique --to <formato> ou --output <arquivo.ext>")
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

            // Preservação do manifesto de custódia
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
                    "Conversão concluída com sucesso: {} -> {}",
                    args.input.display(),
                    output_path.display()
                );
                println!("Formato de saída: {}", target_format);
                println!("SHA-256 da imagem gerada: {}", converted_frame.sha256());
                println!(
                    "SHA-256 de custódia (frame original): {}",
                    sha256_original_raw
                );
                println!(
                    "Manifesto derivado gerado: {}.meta.json",
                    output_path.display()
                );
            }
        }
        Command::Inspect(args) => {
            let format = ImageFormat::detect(&args.image)
                .context("falha ao identificar formato do arquivo")?;
            let frame = RawFrame::from_file(&args.image).context("falha ao ler dados da imagem")?;
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
                println!("Inspeção Técnica de Imagem Forense");
                println!("===================================");
                println!("Arquivo: {}", args.image.display());
                println!("Formato detectado: {}", format);
                println!(
                    "Resolução: {}x{} pixels @ {} DPI ({} bytes)",
                    WIDTH, HEIGHT, DPI, FRAME_LEN
                );
                println!("SHA-256 do buffer: {}", sha256);
                println!(
                    "Qualidade do frame: {}/100 ({})",
                    quality.score,
                    if quality.accepted {
                        "aceita"
                    } else {
                        "baixa qualidade"
                    }
                );
                println!("  - Variância local: {:.1}", quality.mean_local_variance);
                println!("  - Saturação: {:.1}%", quality.saturated_ratio * 100.0);
                if let Some(m) = &manifest {
                    println!("Manifesto de custódia: Presente");
                    println!("  - SHA-256 original (custódia): {}", m.sha256_raw_frame());
                    match m {
                        Manifest::Capture(c) => {
                            println!("  - Tipo: Captura direta (hardware)");
                            println!("  - Data UTC: {}", c.captured_at_utc);
                        }
                        Manifest::Derived(d) => {
                            println!("  - Tipo: Derivado / Transcodificado");
                            if !d.transformations.is_empty() {
                                println!("  - Transformações: {}", d.transformations.join(", "));
                            }
                        }
                    }
                } else {
                    println!("Manifesto de custódia: Ausente (.meta.json não encontrado)");
                }
            }
        }
        Command::Verify(args) => {
            if !args.threshold.is_finite() || !(0.0..=100.0).contains(&args.threshold) {
                anyhow::bail!("--threshold deve estar entre 0 e 100");
            }
            let probe = RawFrame::from_file(&args.probe).context("falha ao carregar probe")?;
            let reference =
                RawFrame::from_file(&args.reference).context("falha ao carregar reference")?;
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
