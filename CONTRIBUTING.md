# Guia de Contribuição — fs80hctl

Obrigado pelo interesse em contribuir com o `fs80hctl`!

## Regra Fundamental: Proibição de Biometria Real

> [!CAUTION]
> **Nunca inclua impressões digitais reais em Pull Requests, Issues ou arquivos de teste.**
> Todo teste automatizado deve utilizar imagens sintéticas geradas programaticamente ou os fixtures determinísticos já aprovados em `tests/fixtures/`.

Para gerar novos fixtures sintéticos, utilize as funções em `tests/fixture_support.rs`.

## Ambiente de Desenvolvimento e Ferramentas

- **Rust**: Edição 2024 / Toolchain estável recente.
- **Formatação e Linters**:
  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  ```
- **Testes sem hardware**:
  ```sh
  cargo test --workspace
  ```
- **Verificação anti-vazamento de dados sensíveis**:
  ```sh
  ./scripts/check-clean-workspace.sh
  ```

## Estrutura do Workspace

- [`crates/fs80h-core`](crates/fs80h-core): Lógica pura de domínio, decodificação PNG, hash SHA-256 de custódia, métricas de qualidade e matcher determinístico 1:1. Zero dependência de hardware USB.
- [`crates/fs80h-driver`](crates/fs80h-driver): Camada de integração com o driver `futronic-fs80h-rs`.
- [`crates/fs80h-cli`](crates/fs80h-cli): Interface de linha de comando (`fs80hctl`).

## Padrões de Código

- Mantenha a separação rígida de camadas: `fs80h-core` não deve depender de USB.
- Toda nova funcionalidade deve acompanhar testes unitários ou de integração sintéticos.
- Todo comando de saída estruturada deve suportar a flag global `--json`.
