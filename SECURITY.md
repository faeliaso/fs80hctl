# Política de Segurança e Governança de Dados Biométricos

## 1. Escopo e Fronteiras Operacionais

O `fs80hctl` é uma ferramenta de controle técnico e diagnóstico para leitores biométricos Futronic FS80 e FS81.

- **Finalidade Técnica**: O cálculo de similaridade 1:1 e os relatórios de qualidade gerados pelo `fs80h-core` são métricas diagnósticas determinísticas de engenharia.
- **Limites Estritos de Uso**:
  - Este software **não** realiza identificação civil ou criminal oficial.
  - **Não** emite laudos periciais automatizados.
  - **Não** realiza detecção de vivacidade (*liveness/anti-spoofing*) garantida contra ataques físicos de apresentação.
  - **Não** deve ser utilizado como único fator de autenticação em sistemas críticos de controle de acesso físico ou patrimonial sem camadas complementares de segurança e consentimento.

## 2. Proteção e Ciclo de Vida do Dado Biométrico

1. **Minimização**:
   - Capturas biométricas só devem ser efetuadas sob demanda expressa do operador.
   - O repositório e o `.gitignore` proíbem o versionamento de qualquer imagem ou manifesto fora da pasta de fixtures sintéticas (`tests/fixtures/`).
2. **Custódia e Integridade**:
   - O hash SHA-256 de custódia é gerado diretamente sobre o frame bruto do sensor antes da gravação do contêiner PNG.
   - Os metadados contêm informações técnicas do dispositivo para fins de rastreabilidade de hardware.
3. **Descarte e Retenção**:
   - Recomenda-se armazenar imagens biométricas apenas em memória ou em diretórios temporários (`mktemp -d`) com permissões restritas (ex: `0700`), descartando-os imediatamente após o uso pretendido.

## 3. Reporte Responsável de Vulnerabilidades

Caso você descubra uma vulnerabilidade de segurança neste projeto:
- **Não abra uma Issue pública** detalhando falhas de segurança exploráveis ou vazamento de dados.
- Envie um relatório por e-mail para o mantenedor do projeto ou abra um *Security Advisory* privado no repositório GitHub.
- Forneça detalhes de reprodução, impacto esperado e possíveis mitigações.
- Comprometemo-nos a responder e tratar relatos de segurança com prioridade máxima.
