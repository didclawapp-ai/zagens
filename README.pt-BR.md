<p align="center">
  <img src="assets/screenshot.png" alt="Captura de tela do Zagens" width="800" />
</p>

# Zagens — Console de Agent desktop

**[English](README.md)** · **[中文](README.zh-CN.md)** · **[日本語](README.ja.md)** | Português (BR)

Tarefas longas de Agent tendem a **parar no meio ou marcar “concluído” cedo demais**. Código e arquivos Office costumam ficar em **ferramentas separadas**. Agents locais precisam de **replay, aprovação e auditabilidade** — não só mais uma janela de chat.

**Zagens** é um **console de Agent desktop** feito para o **ecossistema [DeepSeek V4](https://deepseek.com/)**: otimizado para API DeepSeek, fluxos de raciocínio e chamadas de ferramentas (DeepSeek Pro / Flash por padrão). Escolha **desktop Tauri**, **`zagens-tui`** em tela cheia ou **`zagens` CLI** headless — todos compartilham runtime **Kernel V3** para workspaces Code e Office, **replay de sessão** turno a turno, **portões de conclusão** em camadas e recursos nativos de desktop quando aplicável (bandeja, notificações, PTY). Endpoints OpenAI-compatíveis seguem disponíveis como alternativa.

> **Nota dos autores:** Não acredite que um Agent de IA pode fazer qualquer coisa — ele tem limites. O que podemos fazer é ampliar esses limites.

> **Licença:** [MIT](LICENSE). Linhagem do runtime: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/](third-party/deepseek-tui/). Capacidades abaixo refletem **Zagens v0.7.5** — veja [CHANGELOG.md](CHANGELOG.md).

| Recurso | Link |
|---------|------|
| Guias do usuário | [zagens.com/docs](https://zagens.com/docs) |
| Downloads | [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) (último **`zagens-v0.7.5`**) · [zagens.com/download](https://zagens.com/download) |
| Especificações | [`docs/README.md`](docs/README.md) |
| Contribuição | [`CONTRIBUTING.md`](CONTRIBUTING.md) · [`LOCAL_DEV_VERIFY.md`](LOCAL_DEV_VERIFY.md) |
| Segurança | [`SECURITY.md`](SECURITY.md) |

---

## Para quem é / para quem não é

| Combina bem | Combina menos |
|-------------|---------------|
| **Usuários avançados de DeepSeek** — fluxos diários com API DeepSeek / V4 que querem um harness além do TUI oficial | SaaS hospedado com modelos e cobrança inclusos |
| Devs que querem um **harness desktop independente** (sem ficar preso a uma extensão de IDE) | Só chat — sem ferramentas, workspace ou replay |
| **Terminal-first** — **`zagens-tui`** em tela cheia, mesmo motor do desktop | Agents YOLO totalmente autônomos, sem guardrails |
| Times em **refactors longos** ou **entregas Office** no mesmo fluxo | Experiência mobile ou só navegador, zero setup |
| Quem valoriza **sidecar local**, MCP/skills e **aprovação de exec** na UI | Times que só querem copiloto web sem execução local |
| Usuários **Windows desktop** hoje; macOS/Linux via **TUI**, **CLI** ou build da fonte | |

---

## Três coisas que definem o Zagens

**1. Harness, não casca de chat** — Tarefas de código longas usam **portões de conclusão composáveis** (operador / modelo / toolchain), não “o modelo disse que terminou”. Spec: [LHT](docs/harness/LONG_HORIZON_CODE_TASKS.md) · fixtures: [`fixtures/harness/`](fixtures/harness/).

**2. Desktop + terminal, um motor** — Desktop [Tauri 2](https://tauri.app/) **ou** **`zagens-tui`** em tela cheia (ratatui) **ou** CLI headless **`zagens`** — todos rodam **Kernel V3** (`LiveTurnMachine` + `EffectInterpreter`, turns event-sourced, resume log-first). O desktop adiciona bandeja, WebView, PTY embutido e supervisão do sidecar; o TUI traz transcript/composer/inspector em 3 colunas + painel LHT no terminal.

**3. Code + Office, um runtime** — Tipos **Code / Office** compartilham ferramentas e config, com superfícies e prompts diferentes; trocar o tipo abre **nova sessão** para KV estável ([arquitetura](docs/task-type-prompt-architecture.md)). Office: `read_file` / **`write_office`** (xlsx em Rust; docx/pptx/pdf via Python embutido).

Também: **CRAFT multi-agent** (sub-agents, vereditos fix-loop, blackboard P1 — [notas](docs/craft-v2-improvements.md)), **índice de símbolos** lazy (`.zagens/symbols.json`), MCP, skills, hooks, tarefas agendadas, **`batch_edit`** / **`refactor_imports`** em lote.

---

## Problemas que priorizamos

| Dor | Abordagem do Zagens |
|-----|---------------------|
| Agent para no meio ou marca conclusão cedo | **Portões em camadas** + painel de tarefa longa ([harness composável](docs/harness/COMPOSABLE_HARNESS.md)) |
| Plugins de IDE vs agents de terminal sem história única | **Sidecar** único + threads SQLite, fork/retomar, **replay**, snapshots |
| Planilhas e docs fora do loop do agent de código | **Modo Office** + `write_office` + previews no desktop |
| Executar ferramentas localmente sem confiança cega | Política de exec, regras de rede, canonicalização de paths, UI de aprovação, token de runtime fora do WebView ([matriz de sandbox](docs/tech/SANDBOX_CAPABILITY_MATRIX.md)) |

---

## Disponível hoje (v0.7.5)

**Motor Kernel V3:** loop de turn event-sourced — log `KernelEvent` em `sessions.db`, planejamento `LiveTurnMachine`, IO `EffectInterpreter`, fixtures golden de replay. Spec: [AGENT_KERNEL_V3.md](docs/tech/AGENT_KERNEL_V3.md).

**Desktop (Tauri):** chat multi-sessão (stream/parar/pensamento), árvore + previews + diff, terminal PTY (Code), painel de sub-agents, UI de tasks/skills, MCP + roteamento, gráficos de uso, API key no keyring, UI em zh-Hans / en / ja / pt-BR.

**TUI terminal (`zagens-tui`):** shell 3 colunas — rail de sessões, transcript com streaming/pensamento/ferramentas, composer com `/model` e `/lht`, modal de aprovação, inspector (arquivos / diff / checklist / agents / MCP), painel LHT inferior recolhível, temas, restauração de sessão (`--fresh` para nova). Mesmas threads runtime e caminho Kernel V3 do desktop.

**Runtime:** threads, MCP, skills (`~/.zagens/skills`), hooks de ciclo de vida, roteamento multi-provedor, visão (`describe_image`), montagem de request ContextCompiler V2.

**Ferramentas (representativas):** arquivos (`read_file`, `write_file`, `edit_file`, `batch_edit`, `refactor_imports`, …), git, `exec_shell`, `write_office`, opcional `web_search` / `fetch_url`, memória. Lista completa: `crates/runtime-server/src/tools/` · [CHANGELOG.md](CHANGELOG.md).

---

## Limites conhecidos (leia antes de depender)

Preferimos escopo honesto a checklist de marketing.

| Tópico | Status |
|--------|--------|
| **Instaladores desktop** | **Windows** em [Releases](https://github.com/didclawapp-ai/zagens/releases). **Pacotes desktop macOS / Linux** — planejados. **`zagens` CLI** e **`zagens-tui`** nas três plataformas já disponíveis. |
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

**Pré-compilado (Windows):** [GitHub Releases](https://github.com/didclawapp-ai/zagens/releases) — zip desktop + CLI `zagens` / `zagens-tui`. SmartScreen: [SMARTSCREEN.md](docs/desktop/SMARTSCREEN.md).

**Da fonte — desktop:**

```bash
git clone https://github.com/didclawapp-ai/zagens.git
cd zagens

cargo build -p zagens-cli          # copia zagens-runtime para crates/desktop/binaries/

cd crates/desktop/web-ui && npm install
cd .. && cargo tauri dev

# API key: Configurações do Zagens ou ~/.zagens/config.toml
```

**Da fonte — TUI terminal** (mesmo Kernel V3, runtime in-process):

```bash
cargo build -p zagens-cli --features tui --bin zagens-tui
./target/debug/zagens-tui          # restaura última sessão; --fresh para nova
```

**CLI headless** (automação / sidecar HTTP):

```bash
cargo install zagens-cli --version 0.7.5 --bin zagens --locked

zagens doctor
zagens exec 'summarize src/' --json
zagens exec 'refactor auth module' --auto
zagens serve --http --port 7878
```

CLI pré-compilado + SHA-256: [Releases](https://github.com/didclawapp-ai/zagens/releases). Config: [config.example.toml](config.example.toml).

---

## Arquitetura

```
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Zagens Desktop  │  │   zagens-tui     │  │  zagens CLI      │
│  Tauri + WebView │  │  ratatui TUI     │  │  exec / serve    │
└────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘
         │ HTTP+SSE (loopback) │ in-process          │ in-process / HTTP
         ▼                     ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│  sidecar zagens-runtime  ·  motor de turn Kernel V3            │
│  LiveTurnMachine → EffectInterpreter → V3TurnHost               │
│  /v1/threads · MCP · skills · tools · log kernel_events         │
└───────────────────────────────┬─────────────────────────────────┘
                                ▼
         zagens-core · runtime-orchestrator · runtime-adapters
```

Limites: [`docs/tech/RUNTIME_ARCHITECTURE.md`](docs/tech/RUNTIME_ARCHITECTURE.md) · Kernel V3: [`docs/tech/AGENT_KERNEL_V3.md`](docs/tech/AGENT_KERNEL_V3.md) · HTTP: [`docs/tech/API_DESIGN.md`](docs/tech/API_DESIGN.md).

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
├── crates/desktop/        # app Tauri desktop
├── crates/runtime-server/ # sidecar zagens-runtime · CLI zagens · zagens-tui (feature `tui`)
├── crates/core/           # motor Kernel V3
├── docs/                  # specs públicas
├── fixtures/harness/      # LHT / replay kernel
└── config.example.toml
```

---

## Licença

[MIT](LICENSE) — Copyright (c) 2024-2026 Zagens Contributors. Atribuição adicional: [NOTICE.md](NOTICE.md) · [third-party/deepseek-tui/LICENSE](third-party/deepseek-tui/LICENSE).
