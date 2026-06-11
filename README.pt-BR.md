<p align="center">
  <img src="assets/screenshot.png" alt="Captura de tela do Zagens" width="800" />
</p>

# Zagens — Console de Agent desktop

**[English](README.md)** · **[中文](README.zh-CN.md)** · **[日本語](README.ja.md)** | Português (BR)

Tarefas longas de Agent tendem a **parar no meio ou marcar “concluído” cedo demais**. Código e arquivos Office costumam ficar em **ferramentas separadas**. Agents locais precisam de **replay, aprovação e auditabilidade** — não só mais uma janela de chat.

**Zagens** é um **console de Agent desktop** feito para o **ecossistema [DeepSeek V4](https://deepseek.com/)**: otimizado para API DeepSeek, fluxos de raciocínio e chamadas de ferramentas (DeepSeek Pro / Flash por padrão). Um **runtime sidecar** local para workspaces Code e Office, **replay de sessão** turno a turno, **portões de conclusão** em camadas e shell nativo (bandeja, notificações, terminal embutido). Endpoints OpenAI-compatíveis seguem disponíveis como alternativa.

> **Nota dos autores:** Não acredite que um Agent de IA pode fazer qualquer coisa — ele tem limites. O que podemos fazer é ampliar esses limites.

> **Licença:** [MIT](LICENSE). Linhagem do runtime: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/). Capacidades abaixo refletem **Zagens v0.7.4** — veja [CHANGELOG.md](CHANGELOG.md).

| Recurso | Link |
|---------|------|
| Guias do usuário | [zagens.com/docs](https://zagens.com/docs) |
| Downloads | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) (último **`zagens-v0.7.4`**) · [zagens.com/download](https://zagens.com/download) |
| Especificações | [`docs/README.md`](docs/README.md) |
| Contribuição | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| Segurança | [`SECURITY.md`](SECURITY.md) |

---

## Para quem é / para quem não é

| Combina bem | Combina menos |
|-------------|---------------|
| **Usuários avançados de DeepSeek** — fluxos diários com API DeepSeek / V4 que querem um harness desktop além do TUI oficial | SaaS hospedado com modelos e cobrança inclusos |
| Devs que querem um **harness desktop independente** (sem ficar preso a uma extensão de IDE) | Só chat — sem ferramentas, workspace ou replay |
| Times em **refactors longos** ou **entregas Office** no mesmo fluxo | Agents YOLO totalmente autônomos, sem guardrails |
| Quem valoriza **sidecar local**, MCP/skills e **aprovação de exec** na UI | Experiência mobile ou só navegador, zero setup |
| Usuários **Windows desktop** hoje; macOS/Linux via **CLI** ou build da fonte | Times que só querem copiloto web sem execução local |

---

## Três coisas que definem o Zagens

**1. Harness, não casca de chat** — Tarefas de código longas usam **portões de conclusão composáveis** (operador / modelo / toolchain), não “o modelo disse que terminou”. Spec: [LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · fixtures: [`fixtures/harness/`](fixtures/harness/).

**2. Plano de controle desktop-native** — UI [Tauri 2](https://tauri.app/) sobre **sidecar** loopback (`zagens-runtime`): bandeja, notificações, diff, **replay de sessão**, **PTY** em workspaces Code, aprovação HTTP de ferramentas. Mesmo motor do CLI headless **`zagens`**.

**3. Code + Office, um runtime** — Tipos **Code / Office** compartilham ferramentas e config, com superfícies e prompts diferentes; trocar o tipo abre **nova sessão** para KV estável ([arquitetura](docs/task-type-prompt-architecture.md)). Office: `read_file` / **`write_office`** (xlsx em Rust; docx/pptx/pdf via Python embutido).

Também: **CRAFT multi-agent** (sub-agents, vereditos fix-loop, blackboard P1 — [notas](docs/craft-v2-improvements.md)), **índice de símbolos** lazy (`.zagens/symbols.json`), MCP, skills, hooks, tarefas agendadas.

---

## Problemas que priorizamos

| Dor | Abordagem do Zagens |
|-----|---------------------|
| Agent para no meio ou marca conclusão cedo | **Portões em camadas** + painel de tarefa longa ([harness composável](docs/harness/COMPOSABLE_HARNESS.md)) |
| Plugins de IDE vs agents de terminal sem história única | **Sidecar** único + threads SQLite, fork/retomar, **replay**, snapshots |
| Planilhas e docs fora do loop do agent de código | **Modo Office** + `write_office` + previews no desktop |
| Executar ferramentas localmente sem confiança cega | Política de exec, regras de rede, canonicalização de paths, UI de aprovação, token de runtime fora do WebView ([matriz de sandbox](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)) |

---

## Disponível hoje (v0.7.4)

**Desktop:** chat multi-sessão (stream/parar/pensamento), árvore + previews + diff, terminal PTY (Code), painel de sub-agents, UI de tasks/skills, MCP + roteamento, gráficos de uso, API key no keyring, UI em zh-Hans / en / ja / pt-BR.

**Runtime:** threads, MCP, skills (`~/.zagens/skills`), hooks de ciclo de vida, roteamento multi-provedor, visão (`describe_image`).

**Ferramentas (representativas):** arquivos (`read_file`, `write_file`, `edit_file`, …), git, `exec_shell`, `write_office`, opcional `web_search` / `fetch_url`, memória. Lista completa: `crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md).

---

## Limites conhecidos (leia antes de depender)

Preferimos escopo honesto a checklist de marketing.

| Tópico | Status |
|--------|--------|
| **Instaladores desktop** | **Windows** em [Releases](https://github.com/didclawapp-ai/zagens/releases). **Pacotes desktop macOS / Linux** — planejados. **CLI** nas três plataformas já disponível. |
| **Sandbox no OS** | **macOS Seatbelt** — aplicado quando `sandbox-exec` existe. **Windows** — sandbox nativo implementado (`elevated` recomendado após `zagens sandbox setup`; `unelevated`: isolamento de escrita no workspace). Configurações → **Sandbox** assistente na primeira execução. **Linux** — política declarada, **sem enforcement no OS** (degraded). Detalhes: [`SANDBOX_CAPABILITY_MATRIX.md`](docs/tech/SANDBOX_CAPABILITY_MATRIX.md). |
| **Provedores** | Otimizado para **DeepSeek V4** (Pro / Flash); você traz API keys. Endpoints OpenAI-compatíveis também — **não hospedamos modelos**. |
| **Longo prazo & multi-agent** | Portões e CRAFT **usáveis em produção, ainda evoluindo**; edge cases e novos tipos de portão em desenvolvimento. |
| **Profundidade Office** | Leitura/escrita core ok; conectores enterprise, voz e alguns templates de cenário são **futuro** ([cenários Office](docs/desktop/OFFICE_SCENARIOS.md)). |

Reporte segurança via [`SECURITY.md`](SECURITY.md).

---

## Para onde vamos

Specs públicas em [`docs/`](docs/README.md). Direção:

- **Paridade de plataforma** — instaladores desktop macOS/Linux; sandbox nativo **Linux** (Landlock/bwrap). Sandbox nativo Windows entregue na 0.7.x.
- **Tarefas longas confiáveis** — portões mais rígidos, fixtures de harness, fluxos de operador com replay.
- **Fluxos Office** — mais cenários sem separar do runtime compartilhado.
- **Endurecimento** — melhorias de segurança e exec policy em [CHANGELOG](CHANGELOG.md) e [SECURITY.md](SECURITY.md).

---

## Início rápido

**Pré-compilado (Windows):** [GitHub Releases `zagens-v0.7.3`](https://github.com/didclawapp-ai/zagens/releases) — zip instalador + CLI. SmartScreen: [SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md).

**Da fonte:**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # sidecar copiado para crates/desktop/binaries/

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API key: Configurações do Zagens ou ~/.zagens/config.toml
```

**CLI headless** (mesmo runtime do desktop):

```bash
cargo install zagens-cli --version 0.7.3 --bin zagens --locked

zagens doctor
zagens exec 'summarize src/' --json
zagens exec 'refactor auth module' --auto
zagens serve --http --port 7878
```

CLI pré-compilado + SHA-256: [Releases](https://github.com/didclawapp-ai/zagens/releases). Config: [config.example.toml](config.example.toml).

---

## Arquitetura

```
┌──────────────────────────────────────────────────────────────┐
│                     Zagens (Tauri 2)                         │
│  ┌─────────────────┐  ┌───────────────────────────────────┐  │
│  │   WebView UI    │  │         Rust Shell                │  │
│  │   React / TS    │◄─┤  commands, sidecar supervisor,    │  │
│  └────────┬────────┘  └───────────────┬───────────────────┘  │
│           │ HTTP + SSE                │                       │
│           ▼                           ▼                       │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │     Runtime API (embedded sidecar, loopback HTTP/SSE)    │  │
│  │  /v1/threads, /v1/skills, /v1/symbol-index, ...         │  │
│  └───────────────────────┬─────────────────────────────────┘  │
│                          ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Shared crates: agent, core, config, state, tools, mcp   │  │
│  └─────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

Limites: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · HTTP: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md).

### Modos de segurança (`sandbox_mode`)

| Modo | Descrição |
|------|-----------|
| `read-only` | Sem exec shell nem escrita em arquivos |
| `workspace-write` | Shell e escrita só no workspace (padrão recomendado) |
| `danger-full-access` | Acesso total ao filesystem — use com cuidado |
| `external-sandbox` | Encaminha `exec_shell` para API compatível OpenSandbox |

Políticas de aprovação (`on-request` / `untrusted` / `never`), regras de rede por domínio, keyring do OS. Token de runtime nunca entra no WebView.

---

## Desenvolvimento

**Pré-requisitos:** Rust 1.88+ (MSRV; CI fixa **1.96**), Node.js 20 LTS, Python 3.8+, [Tauri CLI 2](https://v2.tauri.app/start/prerequisites/).

Veja **[CONTRIBUTING.md](CONTRIBUTING.md)** e **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)**.

| Comando | Descrição |
|---------|-----------|
| `bash scripts/ci/verify-lint.sh` | Espelho de lint CI |
| `bash scripts/ci/verify-workspace.sh` | Lint + testes do workspace |
| `cargo test --workspace --all-features` | Todos os testes |
| `cd crates/desktop && cargo tauri dev` | Desktop em modo dev |

Windows: `pwsh -File scripts/ci/verify-lint.ps1`

```
zagens/
├── crates/desktop/       # App Tauri
├── crates/runtime-server/ # Sidecar HTTP/SSE
├── docs/                 # Specs públicas
├── fixtures/harness/     # Fixtures LHT / Office
└── config.example.toml
```

---

## Licença

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors. Atribuição adicional: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE).
