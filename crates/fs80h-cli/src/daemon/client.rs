use super::protocol::{ClientCommand, ServerError, ServerEvent};
use crate::cli::{CliError, OutputFormat};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use fs80h_core::{CaptureManifest, ImageFormat, RawFrame};
use std::io::Write;
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn run_client_capture(
    socket_path: &Path,
    output_path: &Path,
    image_format: Option<ImageFormat>,
    raw_opt: bool,
    timeout_secs: u64,
    min_quality: u8,
    format: OutputFormat,
) -> Result<(), CliError> {
    let mut stream = UnixStream::connect(socket_path).await.map_err(|e| {
        CliError::Hardware(format!(
            "failed to connect to fs80h daemon at {:?}: {e}",
            socket_path
        ))
    })?;

    let req = ClientCommand::Capture {
        timeout_ms: timeout_secs * 1000,
        min_quality,
    };

    let mut payload = serde_json::to_string(&req)
        .map_err(|e| CliError::Operational(format!("serialize request: {e}")))?;
    payload.push('\n');

    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|e| CliError::Operational(format!("write to socket: {e}")))?;

    let (reader, _) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| CliError::Operational(format!("read from socket: {e}")))?
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check if error / busy response
        if let Ok(err_resp) = serde_json::from_str::<ServerError>(trimmed) {
            return Err(CliError::Operational(format!(
                "{}: {}",
                err_resp.status, err_resp.message
            )));
        }

        // Parse event
        if let Ok(event) = serde_json::from_str::<ServerEvent>(trimmed) {
            match event {
                ServerEvent::WaitingFinger => {
                    eprintln!(
                        "Waiting for finger on scanner (timeout: {}s, min quality: {} via daemon)...",
                        timeout_secs, min_quality
                    );
                }
                ServerEvent::FingerDetected {
                    area_coverage,
                    score,
                } => {
                    let s = score.unwrap_or(0);
                    eprint!("\r[contact: {area_coverage}%, score: {s}/100] Stabilizing...");
                    let _ = std::io::stderr().flush();
                }
                ServerEvent::Completed {
                    quality,
                    width: _,
                    height: _,
                    image_base64,
                    sha256_raw_frame,
                    ..
                } => {
                    eprintln!();
                    let raw_bytes = BASE64_STANDARD
                        .decode(image_base64.as_bytes())
                        .map_err(|e| CliError::Operational(format!("base64 decode error: {e}")))?;

                    let raw_frame = RawFrame::new(raw_bytes)
                        .map_err(|e| CliError::Operational(format!("invalid frame: {e}")))?;

                    let img_format = image_format
                        .or_else(|| ImageFormat::from_path_extension(output_path))
                        .unwrap_or(ImageFormat::Png);

                    let standard_afis = !raw_opt;
                    let export_frame = if standard_afis {
                        raw_frame.inverted()
                    } else {
                        raw_frame.clone()
                    };

                    if format == OutputFormat::Raw {
                        let mut stdout = std::io::stdout().lock();
                        stdout.write_all(export_frame.pixels()).map_err(|e| {
                            CliError::Operational(format!("stream raw bytes to stdout: {e}"))
                        })?;
                        stdout
                            .flush()
                            .map_err(|e| CliError::Operational(format!("flush stdout: {e}")))?;
                        return Ok(());
                    }

                    let eval_quality = fs80h_core::evaluate_quality(&raw_frame);
                    let manifest = CaptureManifest::for_frame_with_afis_and_format(
                        &raw_frame,
                        eval_quality.clone(),
                        env!("CARGO_PKG_VERSION"),
                        None,
                        None,
                        standard_afis,
                        img_format,
                    );

                    export_frame
                        .write_format(output_path, img_format)
                        .map_err(|e| CliError::Operational(format!("write image: {e}")))?;
                    manifest
                        .write_adjacent(output_path)
                        .map_err(|e| CliError::Operational(format!("write manifest: {e}")))?;

                    let value = serde_json::json!({
                        "file": output_path,
                        "format": img_format.to_string(),
                        "manifest": format!("{}.meta.json", output_path.display()),
                        "sha256_raw_frame": sha256_raw_frame,
                        "standard_afis": standard_afis,
                        "quality": eval_quality,
                        "daemon": true
                    });

                    match format {
                        OutputFormat::Json => {
                            let s = serde_json::to_string_pretty(&value).map_err(|e| {
                                CliError::Operational(format!("JSON serialization: {e}"))
                            })?;
                            println!("{s}");
                        }
                        OutputFormat::Text => {
                            println!("Capture saved successfully to {}", output_path.display());
                            println!("File format: {}", img_format);
                            println!("Custody manifest: {}.meta.json", output_path.display());
                            println!("SHA-256 (raw frame): {}", sha256_raw_frame);
                            println!("Frame quality: {}/100 (via daemon)", quality);
                        }
                        OutputFormat::Raw => unreachable!(),
                    }

                    return Ok(());
                }
            }
        }
    }

    Err(CliError::Operational(
        "daemon closed connection before completing capture".into(),
    ))
}
