# fs80hctl

CLI técnica em Rust para o leitor biométrico Futronic FS80/FS80H. Esta é a
Fase 1: descoberta de dispositivo, captura de imagem, integridade de captura,
avaliação de qualidade e comparação 1:1 local.

> Os resultados são indicadores técnicos de similaridade. Eles não constituem
> identificação civil, laudo pericial, decisão de vivacidade ou anti-spoofing.

## O que está disponível

- Descoberta do scanner via USB.
- Captura de frame L8 320×480, 500 DPI.
- Exportação PNG sem perdas e manifesto adjacente de cadeia de custódia.
- SHA-256 calculado sobre o frame bruto antes da gravação do PNG.
- Avaliação de qualidade por variância local (blocos de 16×16) e saturação.
- Verificação 1:1 determinística por minúcias espaciais, com score de 0 a 100.
- Score adicional do matcher nativo quando a criação de template ISO for aceita.
- Saída humana padrão ou JSON com `--json`.
- Fixtures sintéticas e testes executáveis sem hardware.

## Requisitos

- Rust/Cargo compatível com a edição 2024.
- macOS Apple Silicon para o bundle atualmente incluído.
- Para captura real: leitor Futronic FS80/FS80H conectado por USB.

O workspace usa diretamente o crate Rust `futronic-fs80h-rs` do projeto irmão.
Não há bridge C, biblioteca dinâmica vendorizada ou bundle de ABI para manter.

## Compilar e testar

```sh
cd /Volumes/MAIN/Workspace/fs80hctl
cargo check --workspace
cargo test --workspace
```

Os testes geram imagens sintéticas e não exigem scanner conectado.

Para reconstruir os fixtures versionados:

```sh
cargo run -p fs80h-core --example generate_fixtures -- tests/fixtures
```

## Uso da CLI

Durante desenvolvimento, execute a CLI pelo Cargo:

```sh
cargo run -p fs80h-cli -- <comando>
```

### Listar dispositivos

```sh
cargo run -p fs80h-cli -- devices
cargo run -p fs80h-cli -- devices --json
```

Exemplo de resposta JSON para um FS80 detectado:

```json
[
  {
    "vendor_id": 5265,
    "product_id": 32,
    "bus_number": 1,
    "address": 1,
    "model": 80
  }
]
```

### Inspecionar o leitor

```sh
cargo run -p fs80h-cli -- device inspect
```

Para selecionar um dispositivo específico:

```sh
cargo run -p fs80h-cli -- device inspect --bus 1 --address 1
```

### Capturar

Capture imediatamente:

```sh
cargo run -p fs80h-cli -- capture --output captura.png
```

A captura gera:

```text
captura.png
captura.png.meta.json
```

O manifesto contém UTC, SHA-256 do frame bruto, tamanho/geometria/DPI,
resultado de qualidade, versão da ferramenta e identificação USB quando
disponível. Ele não contém dados pessoais.

### Verificar duas imagens

```sh
cargo run -p fs80h-cli -- \
  verify captura.png captura.png --threshold 40
```

Para saída estruturada:

```sh
cargo run -p fs80h-cli -- \
  --json verify captura.png captura.png --threshold 40
```

`--threshold` aceita valores entre 0 e 100 e aplica-se ao score determinístico
do core.

## Limites da Fase 1

Ainda não há auto-captura, banco SQLite, cadastro de pessoas, busca 1:N,
exportação WSQ pela CLI, calibração estatística de limiares ou decisão pericial. Imagens biométricas
e manifestos devem ser armazenados e descartados conforme a política aplicável.
