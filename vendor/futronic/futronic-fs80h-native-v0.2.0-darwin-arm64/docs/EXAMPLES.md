# Exemplos, testes e comandos de validação

Este guia é a referência operacional em português. Execute os comandos na raiz
do repositório. Captura requer leitor suportado, permissões USB e `libusb-1.0`.
Imagens, templates e streams LFD são dados biométricos sensíveis.

## Preparação

```sh
# macOS
brew install libusb pkg-config

# Debian/Ubuntu
sudo apt-get install libusb-1.0-0-dev pkg-config
```

No Linux, aplique uma regra udev para VID `1491`, recarregue-a e reconecte o
scanner. Para inspeção somente leitura: `cargo run --example inspect_usb`.

## Qualidade, testes e benchmark

| Objetivo | Comando |
| --- | --- |
| Formatação | `cargo fmt --check` |
| Compilação padrão | `cargo check` |
| Linter | `cargo clippy -- -D warnings` |
| Todas as suites padrão | `cargo test` |
| Doctests | `cargo test --doc` |
| Frames | `cargo test --test frame` |
| WSQ | `cargo test --test wsq` |
| Presença/auto-capture | `cargo test --test presence` |
| Descoberta/serde | `cargo test --test discovery` |
| Núcleo biométrico | `cargo test --test biometric_core` |
| MCC | `cargo test --test mcc` |
| ABI, LFD e testes internos | `cargo test --features native,lfd-experimental-decision --lib` |
| Benchmark MCC | `cargo bench --bench mcc_benchmark --features mcc` |
| Documentação API | `cargo doc --no-deps --open` |

Não use `--all-features` sem NFIQ2 2.3, OpenCV e modelo instalados: `nfiq2` é
externo por design.

## Exemplos de captura e processamento

Todos usam hardware, salvo quando indicado. Quando o programa pede Enter,
posicione o dedo antes de confirmar.

| Exemplo | Comando | Resultado/efeito |
| --- | --- | --- |
| Inspecionar USB | `cargo run --example inspect_usb` | Lê descritores; não captura. |
| Listar/abrir por ID | `cargo run --example list_and_open_by_id -- 1:4` | Lista e captura do ID opcional `barramento:endereço`. |
| Captura PNG | `cargo run --example capture_to_png` | Gera quatro PNGs: calibrado, invertido, normalizado e realçado. |
| Exportação sem perdas | API `RawFrame::encode_image` | Produz PNG, TIFF/Deflate ou PGM com os pixels atuais do frame. |
| Auto-capture | `cargo run --example auto_capture_interactive` | Aguarda contato e grava `auto_captured_fingerprint.png`. |
| Base64 | `cargo run --example capture_to_base64` | Grava PNG/Base64 e Base64 WSQ. |
| LED verde | `cargo run --example capture_with_green_diode` | Captura dose `0xA0` e gera PNG. |
| WSQ | `cargo run --example capture_to_wsq` | Gera PNG, `fingerprint.wsq` e PNG reconstruído. |
| ISO | `cargo run --example capture_to_iso_template` | Gera templates ISO 2011 e 2005. |
| ANSI | `cargo run --example capture_to_ansi_template` | Gera ANSI 378-2004, 2009 e 2009 AM1. |
| Bozorth3 | `cargo run --example capture_and_match` | Captura, extrai minúcias, grava visualizações e auto-match. |
| MCC/Bozorth3 | `cargo run --example capture_and_mcc_match` | Faz 1:1 e 1:N; usa `capture_original_black.png` sem leitor. |
| Matcher 1:1 | `cargo run --release --example verify_live_match` | Captura o mesmo dedo duas vezes. |
| Calibração matcher | `cargo run --release --example calibrate_live_match -- calibration.csv` | Coleta scores e grava CSV opcional. |
| Dois dedos/MCC | `cargo run --release --example verify_two_fingers_mcc` | Compare mesmo dedo ou dedos diferentes interativamente. |

Os formatos PNG, TIFF, PGM, WSQ, `.iso19794` e `calibration.csv` são ignorados por Git,
mas exigem armazenamento e descarte adequados.

## Exportação de imagens sem perdas

O recurso `image-formats` vem habilitado por padrão. `encode_image` retorna os
bytes em memória e nunca aplica inversão, normalização ou realce por conta
própria: chame a transformação desejada antes de exportar. PNG e TIFF carregam
metadados físicos de 500 DPI; PGM P5 é destinado a diagnóstico e não possui
campo padronizado para DPI. Para intercâmbio biométrico a 500 DPI, mantenha
WSQ como o formato de referência.

```rust,no_run
use futronic_fs80h_rs::{FingerprintImageFormat, FutronicDevice};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut scanner = FutronicDevice::open_first()?;
let frame = scanner.capture_frame()?;

std::fs::write(
    "fingerprint.tiff",
    frame.encode_image(FingerprintImageFormat::Tiff)?,
)?;
std::fs::write(
    "fingerprint.pgm",
    frame.encode_image(FingerprintImageFormat::Pgm)?,
)?;
# Ok(())
# }
```

## NFIQ2 opcional

Instale NFIQ2 2.3 e dependências, defina `NFIQ2_DIR` para diretório com
`include/` e `lib/`, e informe o `model.info` do NIST:

```sh
NFIQ2_DIR=/opt/nfiq2 \
  cargo run --features nfiq2 --example capture_with_nfiq2_quality -- \
  /opt/nfiq2/model/model.info
```

O score não é prova de pressão, identidade ou vivacidade.

## LFD: pesquisa em FS80H isolado

> Não execute em produção. Os comandos não ativam nem certificam TLFD. Use
> apenas dados consentidos e diretórios protegidos.

| Operação | Comando | Efeito |
| --- | --- | --- |
| Coletar sequência | `cargo run --features lfd-research --example capture_lfd_research -- /private/tmp/lfd-research` | Grava RAW e métricas. |
| Plano CMOS offline | `cargo run --features lfd-research --example probe_lfd_colored_cmos -- --dry-run` | Não acessa USB. |
| Ler firmware | `cargo run --features lfd-research --example probe_lfd_colored_cmos -- --read-firmware-config` | Consulta somente leitura. |
| Uma requisição auxiliar | `cargo run --features lfd-research --example probe_lfd_auxiliary -- --send-unvalidated /private/tmp/probe.raw` | Envia requisição inferida; reconecte se falhar. |
| Triplet auxiliar | `cargo run --features lfd-research --example probe_lfd_auxiliary_triplet -- --send-unvalidated /private/tmp/lfd-triplet` | Coleta três streams. |
| Análise offline | `cargo run --features lfd-research --example analyze_lfd_auxiliary_triplet -- /private/tmp/lfd-triplet` | Lê os três `selector-*.raw`. |
| CMOS verificado | `cargo run --features lfd-research --example capture_lfd_colored_cmos -- --apply-verified-cmos /private/tmp/lfd-colored-cmos` | Escreve perfil e tenta restaurar `4A 03`. |
| Gate experimental | `cargo run --features lfd-experimental-decision --example evaluate_lfd_experimental -- --enable-experimental-decision config/lfd-experimental.example.conf /private/tmp/lfd-evaluation` | Grava `experimental-report.txt`. |

O gate é calibrado por dataset, não classificação de dedo vivo/falso. Recalibre
a política externa em todo ambiente.

## ABI C e pacote nativo

```sh
cargo build --release --features native,lfd-experimental-decision
cc -Iinclude examples/native_smoke.c -Ltarget/release \
  -lfutronic_fs80h_rs -o /tmp/futronic-native-smoke

# macOS
DYLD_LIBRARY_PATH=target/release /tmp/futronic-native-smoke

# Linux
LD_LIBRARY_PATH=target/release /tmp/futronic-native-smoke

./scripts/package-native.sh
```

No Linux, o script requer `patchelf`; no macOS usa `install_name_tool`. O
smoke test apenas enumera dispositivos e verifica a versão da ABI.
