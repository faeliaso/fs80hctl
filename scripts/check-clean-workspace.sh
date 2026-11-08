#!/usr/bin/env bash
set -euo pipefail

# Script de auditoria para garantir que nenhum artefato biométrico acidental
# (PNG, metadados ou logs) esteja rastreado no git fora de tests/fixtures/.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> Verificando arquivos versionados e staged no Git..."

# Procura por qualquer .png ou .meta.json rastreado ou staged fora de tests/fixtures
LEAKED_TRACKED=$(git ls-files | grep -E '\.(png|meta\.json)$' | grep -v '^tests/fixtures/' || true)
if [[ -n "$LEAKED_TRACKED" ]]; then
    echo "ERRO CRÍTICO: Arquivos biométricos ou manifestos rastreados detectados fora de tests/fixtures:" >&2
    echo "$LEAKED_TRACKED" >&2
    exit 1
fi

# Verifica também arquivos staged no índice
LEAKED_STAGED=$(git diff --cached --name-only | grep -E '\.(png|meta\.json)$' | grep -v '^tests/fixtures/' || true)
if [[ -n "$LEAKED_STAGED" ]]; then
    echo "ERRO CRÍTICO: Arquivos biométricos ou manifestos staged para commit detectados fora de tests/fixtures:" >&2
    echo "$LEAKED_STAGED" >&2
    exit 1
fi

echo "OK: Nenhum dado biométrico indevido rastreado ou staged."
