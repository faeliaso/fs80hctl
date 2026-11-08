# futronic-fs80h-rs

[English README](README_EN.md) · [Comandos, exemplos e testes](EXAMPLES.md)

Driver USB nativo em Rust para os leitores ópticos Futronic FS80/FS80H e
FS81H. Captura imagens em escala de cinza, processa templates biométricos
localmente e pode produzir uma ABI C estável, sem ScanAPI proprietária.

## Status, escopo e limites

- Hardware: VID `0x1491`, PID `0x0020` (FS80/FS80H) e PID `0x0025` (FS81H).
- A captura normal retorna `RawFrame`: 320×480, 8 bits, escala de cinza e 500
  DPI. O stream bruto interno é 384×800 antes do reamostramento.
- O transporte usa `libusb-1.0` via `rusb`, Bulk OUT `0x01` e Bulk IN `0x82`.
  A dose padrão calibrada é `166` (`0xA6`).
- WSQ, ISO/IEC 19794-2 e ANSI INCITS 378 são implementações de formato; não
  há declaração de certificação governamental, ANSI, ISO, NIST ou de parceiros.
- Matching, NFIQ2 e pesquisa LFD executam localmente. Aplicações consumidoras
  continuam responsáveis pela proteção de dados biométricos.

> A pesquisa LFD não implementa a Traditional Live Finger Detection (TLFD)
> proprietária, não decide “dedo vivo/falso” e não deve ser usada como controle
> anti-spoof. Leia [Pesquisa LFD experimental](#pesquisa-lfd-experimental).

## Instalação e primeiro uso

```toml
[dependencies]
futronic-fs80h-rs = { path = "../futronic-fs80h-rs" }
```

O import Rust é `futronic_fs80h_rs`. Instale `libusb-1.0` e `pkg-config`:

```sh
# macOS
brew install libusb pkg-config

# Debian/Ubuntu
sudo apt-get install libusb-1.0-0-dev pkg-config
```

No Linux, conceda acesso USB à aplicação — por exemplo, com uma regra udev que
contenha `SUBSYSTEM=="usb", ATTR{idVendor}=="1491", MODE="0660",
GROUP="plugdev"` — recarregue as regras e reconecte o leitor.

```rust,no_run
use futronic_fs80h_rs::FutronicDevice;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut scanner = FutronicDevice::open_first()?;
let frame = scanner.capture_frame()?;
println!("{}×{} @ {} DPI", frame.width(), frame.height(), frame.dpi());
# Ok(())
# }
```

Para mais de um scanner, use `list_devices`, `DeviceId` e `open_device_by_id`;
`open_single_device` falha quando a seleção seria ambígua.

## Capacidades e features

| Área | API principal | Feature | Observações |
| --- | --- | --- | --- |
| Captura e descoberta | `FutronicDevice`, `RawFrame`, `DeviceId` | padrão | Captura direta, diodos e seleção por barramento/endereço. |
| Presença/auto-capture | `evaluate_frame_presence`, `AutoCaptureOptions` | padrão | Mede contato e estabiliza captura; não é liveness. |
| WSQ | `RawFrame::to_wsq`, `RawFrame::from_wsq` | `wsq` | Codificador/decodificador NBIS compilado localmente. |
| Imagens sem perdas | `RawFrame::encode_image` | `image-formats` | PNG e TIFF com 500 DPI; PGM para diagnóstico técnico. |
| Templates | `IsoTemplate`, `AnsiTemplate`, `TemplateOptions` | `templates`, `ansi-378` | ISO 2005/2011 e ANSI 378 de um dedo. |
| Matcher | `Matcher` | `matcher`, `matcher-base64` | Verificação 1:1, busca 1:N e Base64 explícito. |
| Núcleo biométrico | `extract_minutiae`, Bozorth3 | `biometric-core` | Extração, renderização e comparação de minúcias. |
| MCC | `MccTemplate`, `GalleryIndex` | `mcc` | Matching Bit-based MCC e busca paralela com Rayon. |
| Qualidade | `Nfiq2` | `nfiq2` | Requer NFIQ2 2.3, OpenCV e modelo externos. |
| Pesquisa LFD | módulo `lfd` | `lfd-research` | Coleta/análise experimental exclusiva do FS80H. |
| Gate de sinal | `decide_lfd_experimental` | `lfd-experimental-decision` | Política calibrada por dataset; não é TLFD. |
| ABI C | `futronic_fs80h.h` | `native` | Biblioteca dinâmica para outros runtimes. |

As features padrão incluem WSQ, formatos de imagem, templates, ANSI, matcher,
núcleo biométrico e MCC. `native` habilita a decisão LFD experimental porque a ABI a exporta.
`nfiq2` requer que `NFIQ2_DIR` aponte para uma instalação com `include/` e `lib/`.

## Fluxo biométrico local

`RawFrame` preserva a geometria do sensor e oferece inversão, normalização,
realce, reamostramento e métricas de contato. Para auto-capture, use
`wait_for_finger_present` ou `wait_for_finger_and_capture`; aguarde a remoção
com `wait_for_finger_removed` antes da próxima operação.

WSQ converte o frame de 153.600 bytes para formato FBI e o reconstrói como
`RawFrame`. ISO/IEC 19794-2:2011 é o padrão; ISO 2005 + Cor.1:2009 e ANSI
378-2004/2009/2009 AM1 são opções explícitas. Os parsers validam perfis
suportados e rejeitam registros malformados. Templates, como imagens, são dados
biométricos sensíveis.

`RawFrame::encode_image` exporta PNG, TIFF e PGM sem modificar os pixels atuais
do frame. PNG e TIFF são grayscale de 8 bits e incluem 500 DPI; TIFF usa Deflate
sem perdas. PGM (P5) é destinado a ferramentas técnicas e não transporta DPI
padronizado. Esses contêineres não substituem WSQ em intercâmbio biométrico.

```rust
use futronic_fs80h_rs::FingerprintImageFormat;

let png = frame.encode_image(FingerprintImageFormat::Png)?;
let tiff = frame.encode_image(FingerprintImageFormat::Tiff)?;
```

O `Matcher` compara ISO e ANSI em 1:1, retorna candidatos 1:N ordenados por
score e oferece Base64 por formato explícito. Não trate limiares demonstrativos
como FAR/FRR universal: calibre por dispositivo, população e cenário. O núcleo
biométrico expõe extração de minúcias, renderização, Bozorth3 e MCC. MCC inclui
galeria em memória e busca paralela — não banco de dados ou serviço remoto.

### Qualidade NFIQ2

NFIQ2 gera score de utilidade de imagem de 0 a 100 e feedback acionável antes
de criar um template. O limite padrão é 50 e pode ser alterado por
`Nfiq2Options`. Ele não mede pressão, não detecta vivacidade e não prova a
causa física de uma captura ruim. Modelo, NFIQ2 2.3 e OpenCV são externos;
veja o comando em [EXAMPLES.md](EXAMPLES.md).

## Integração nativa C

Com `--features native`, o crate produz `cdylib` além da biblioteca Rust. O
artefato é `libfutronic_fs80h_rs.dylib` no macOS e
`libfutronic_fs80h_rs.so` no Linux; clientes Unix devem linkar com
`-lfutronic_fs80h_rs`. O contrato está em
[`include/futronic_fs80h.h`](include/futronic_fs80h.h), e
[`examples/native_smoke.c`](examples/native_smoke.c) é o smoke test mínimo.

A ABI v1 usa `fs80h_status`, handles opacos e chamadas síncronas. Após erro,
leia `fs80h_last_error_message()` na mesma thread. Libere handles com seu
`*_free` e cada `fs80h_buffer` somente com `fs80h_buffer_free`; nunca use
`free`, `delete` ou o alocador do consumidor. Buffers de entrada pertencem ao
chamador, e um handle não pode ser usado simultaneamente por várias threads.

```c
#include "futronic_fs80h.h"

int main(void) {
  fs80h_device *device = NULL;
  if (fs80h_device_open_first(&device) != FS80H_OK) return 1;
  fs80h_device_close(device);
  return 0;
}
```

Use `fs80h_list_devices(NULL, 0, &count)` quando houver mais de um leitor;
aloque `fs80h_device_info[count]`, liste novamente e abra pelo ID desejado.

```sh
cargo build --release --features native,lfd-experimental-decision
./scripts/package-native.sh
```

O pacote em `dist/` reúne biblioteca, runtime `libusb`, header, READMEs,
`EXAMPLES.md` e licenças. Mantenha biblioteca e `libusb` no bundle ou em um
caminho do carregador: rpath/`LD_LIBRARY_PATH` no Linux,
`@loader_path`/rpath no macOS, ou diretório do executável no Windows.

## Pesquisa LFD experimental

`lfd-research` existe para investigar o FS80H em leitor isolado e com coleta
consentida. FS81H é reportado como não suportado. A sequência inferida por
análise estática é: configurar CMOS/backlight, coletar três frames auxiliares
na dose `0x2d` com seletores `0x84`, `0x81` e `0x01`, subtrair frame escuro,
calcular exposição, capturar frames finais e restaurar CMOS. Isso não prova
calibração ou limiares do fornecedor.

As evidências estáticas incluem `SetOptions`, `GetFrame`,
`CalculationDLFD` e `IsFakeFinger80H` nas bibliotecas do fornecedor, além da
ativação de `FTR_OPTIONS_DETECT_FAKE_FINGER` no demo Android. Elas demonstram
que a solução proprietária consome mais que o frame público 320×480, mas não
revelam constantes de calibração, features ou taxas de erro.

O coletor descarta duas capturas de aquecimento e grava streams 384×800,
timings, respostas de controle/descritor e métricas descritivas. O modo
colored-CMOS usa comandos Bulk OUT de oito bytes para `0x4A..0x4F` e restaura
`4A 03`; acione-o apenas com a flag explícita em leitor de teste. A consulta de
firmware é somente leitura. A análise offline subtrai o canal `0x01`, mede
diferença absoluta média e correlação; o limiar 20 é diagnóstico, não liveness.

`lfd-experimental-decision` lê política externa versionada e retorna
`biometric_signal_present` ou `insufficient_biometric_signal`. A política de
exemplo foi ajustada somente a capturas genuínas e luvas médicas de quatro
camadas; recalcule-a em cada ambiente com dados consentidos e rotulados. Não a
apresente como classificação de dedo vivo, anti-spoof ou equivalente à TLFD.
Não versione streams RAW; proteja e descarte dados conforme a política aplicável.

Uma coleta responsável registra PID, bytes de controle/descritor, dose, timing,
métricas e streams por sessão; mantém variação normal de posicionamento; e usa
amostras autorizadas e rotuladas antes de qualquer classificador. Próximos
marcos técnicos são validar a tabela de geometria e o transporte de registrador
contra traces independentes, manter representação não executável da sequência,
e avaliar taxas de erro em conjunto separado antes de qualquer uso além de
pesquisa.

## Arquitetura e qualidade

Os módulos são organizados por coesão: transporte/captura, frames, descoberta,
erros, presença, templates, matcher, WSQ, biometria, LFD, NFIQ2 e FFI. A
superfície pública é reexportada por `src/lib.rs`; protocolo e USB ficam
internos. Há testes unitários e suites de integração para frame, WSQ, presença,
descoberta, núcleo biométrico e MCC.

O workflow de release valida formatação, compilação, testes e Clippy; depois
constrói/testa ABI nativa nas plataformas configuradas. Rode localmente:

```sh
cargo fmt --check
cargo check
cargo test
cargo clippy -- -D warnings
```

Todos os comandos por suite, feature, exemplo, benchmark e saída estão em
[EXAMPLES.md](EXAMPLES.md).

## Evolução do projeto

1. Driver Rust inicial para FS80H/FS81H e captura USB.
2. WSQ, templates ISO 2011, ISO 2005 e ANSI 378.
3. Matcher local, NFIQ2 opcional e pesquisa de sinais LFD.
4. Transporte CMOS, extração de minúcias, Bozorth3 e MCC com benchmark.
5. Presença de dedo, auto-capture, Base64, descoberta por `DeviceId` e ABI C.
6. Nome público do crate, CI de qualidade e parser ISO resistente a truncamento.

## Licença

Código Rust sob [MIT](LICENSE). O código WSQ derivado do NBIS está sob
[`c_src/LICENSE-NIST`](c_src/LICENSE-NIST).
