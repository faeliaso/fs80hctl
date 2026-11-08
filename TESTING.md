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

O script produz a captura e manifesto apenas em `/private/tmp`, faz uma
auto-verificação local da imagem e informa o diretório ao final. Dados
biométricos são sensíveis: revise e descarte esse diretório depois do teste.
