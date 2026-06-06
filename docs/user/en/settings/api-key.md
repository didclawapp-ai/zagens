# API key & providers

Zagens talks to LLM providers through your **API key** and `~/.zagens/config.toml` (shared with the runtime sidecar).

## First-time setup

1. Obtain a key from your provider (e.g. [DeepSeek platform](https://platform.deepseek.com/)).
2. Open **Settings → API Key** in Zagens.
3. Paste the key — on desktop it is stored via the **OS keyring** when available.

The sidebar warns **API key not configured** until a valid key is present.

## Vision bridge (optional)

**Settings → API Key → Vision bridge** configures the vision model endpoint (scans, `describe_image`, …):

- **API key** stored in the OS keyring (`vision` entry)
- **Base URL / model** written to `[vision]` in `~/.zagens/config.toml`

Without it, `read_office` still works; scanned-document OCR paths rely on the vision bridge.

## Multiple providers

You can configure several OpenAI-compatible providers and switch at runtime. Example endpoints: DeepSeek, NVIDIA NIM, Fireworks, OpenRouter, local vLLM/Ollama.

See `config.example.toml` in the repo for `[providers]` samples.

## Model selection

Pick models in the composer or settings rail. Model IDs change with provider catalogs — treat docs as patterns, not guarantees.

## Security

- Never commit keys to git or share exports containing credentials.
- Runtime token for the local sidecar is separate from provider keys.

Related: [Vision tools](/docs/tools/vision) · [Tool approval](/docs/settings/approval) · [Model routing](/docs/settings/routing)
