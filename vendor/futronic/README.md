# Bundles nativos Futronic

Este diretório contém as distribuições nativas autocontidas consumidas por
`fs80hctl`. O código-fonte do driver permanece exclusivamente no repositório
`futronic-fs80h-rs`; aqui entram somente artefatos gerados, header,
documentação e licenças.

## Regra de seleção

Em cada compilação, os `build.rs` do driver procuram diretórios com o nome:

```text
futronic-fs80h-native-v<major>.<minor>.<patch>-<alvo>
```

Para o alvo atual, um bundle só é elegível se o `manifest.json` declarar:

- `target` igual ao alvo da compilação (por exemplo, `darwin-arm64`);
- `abi_version: 2`;
- uma versão semântica válida no nome do diretório.

Entre os elegíveis, a maior versão semântica vence. Portanto, ao incluir
`futronic-fs80h-native-v0.3.0-darwin-arm64`, ele será preferido a `v0.2.0` sem
nenhuma alteração no CLI. Bundles ABI v1 são deliberadamente ignorados.

ABI é o contrato binário da dylib: assinaturas das funções, structs, enums e
propriedade de memória. A versão do pacote pode avançar livremente dentro da
mesma ABI; uma ABI diferente exige bindings compatíveis no CLI.

## Publicar uma atualização compatível

1. Gere e teste o pacote no `futronic-fs80h-rs`.
2. Confirme que `manifest.json` possui `abi_version: 2`, o alvo correto e hashes
   dos artefatos.
3. Importe a pasta gerada:

   ```sh
   ./scripts/vendor-futronic.sh /caminho/futronic-fs80h-native-v0.3.0-darwin-arm64
   cargo check --workspace
   cargo test --workspace
   ```

O importador valida header ABI v2, alvo e SHA-256 antes de copiar. Ele somente
substitui o diretório de mesma versão/alvo; versões anteriores podem coexistir
para auditoria e rollback por commit.

## Evoluir a ABI

Uma nova ABI não deve reutilizar o número 2. O fluxo é: atualizar header e
bindings Rust juntos, testar a nova ABI, alterar a regra de compatibilidade do
CLI e então importar o novo bundle. Nunca remova a validação de ABI para
aceitar uma dylib desconhecida: uma assinatura incompatível pode causar
corrupção de memória, não apenas uma feature ausente.

`FS80H_DRIVER_DIST_DIR` serve apenas para desenvolvimento. O bundle vendorizado
compatível continua tendo prioridade.
