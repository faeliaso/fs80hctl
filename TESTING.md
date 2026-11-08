# Testes

## Sem hardware

```sh
cargo check --workspace
cargo test --workspace
```

## Leitor físico

Instale `libusb-1.0` e `pkg-config`; no Linux, dê à sua conta acesso ao USB
do dispositivo Futronic (VID `1491`). Conecte o leitor e execute:

```sh
./scripts/test-physical.sh
```

Com múltiplos leitores, obtenha o barramento e endereço com `devices` e rode:

```sh
FS80H_BUS=1 FS80H_ADDRESS=4 ./scripts/test-physical.sh
```

Se a captura em frame mode expirar, teste o caminho direto de dose variável:

```sh
FS80H_VARIABLE_DOSE=1 ./scripts/test-physical.sh
```

O script produz a captura e manifesto em uma pasta temporária `fs80h-physical.*`
dentro de `~/Downloads`, faz uma auto-verificação local da imagem e informa o
diretório ao final. Para escolher outro local, defina `FS80H_OUTPUT_DIR`.
Dados
biométricos são sensíveis: revise e descarte esse diretório depois do teste.
