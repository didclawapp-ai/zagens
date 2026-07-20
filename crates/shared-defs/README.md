# Shared defs

Single source of truth for cross-crate / Rust↔TS shared metadata.

## `model-catalog.json`

Model capability families: context window, max output, sampling omit, thinking flags,
and optional `effort_map` (UI/config effort aliases → wire `reasoning_effort`).

- **Rust:** `zagens-core` embeds this file via `include_str!` and evaluates match rules at runtime (`resolve_model_caps`).
- **TypeScript:** `just model-catalog` → `crates/desktop/web-ui/src/lib/generated/modelCatalog.ts`
- **CI:** `just model-catalog-check`

Match predicates run on a lowercased model id. Families are tried in order; first match wins.

## `providers.toml`

Provider identity: ids, aliases, default base_url/model, keyring slot, env vars (`env_api_key` / `env_base_url` / `env_model`), desktop flags.

- **Generate:** `just providers` →
  - `crates/config/src/generated/{provider_kind,providers_toml,provider_defaults,provider_env}.rs`
  - `crates/runtime-server/src/config/generated/{api_provider,provider_defaults,provider_env}.rs`
  - `crates/runtime-server/src/config/generated/providers_config.rs` (included from `types.rs`)
  - `crates/secrets/src/generated/keyring_slots.rs`
  - `crates/shared-defs/generated/providers.example.toml.fragment` — default stubs + env names
  - `crates/shared-defs/generated/provider_ids.txt` — facade id drift list
- **CI:** `just providers-check`
- **Example docs:** rich commentary stays in `crates/config/assets/config.advanced.example.toml`; the fragment above is the machine-readable stub SSOT.
- **Still hand-written:** `apply_reasoning_effort` dialects, catalog keep/output_limit rules, Custom providers UI, Nvidia↔`DEEPSEEK_BASE_URL` legacy glue in `apply_env_overrides`.

Ollama default model is `qwen2.5-coder:7b` (facade/desktop SSOT); runtime defaults are generated from the same TOML.
`deepseek-cn` is `runtime_only` (ApiProvider only; not a facade ProviderKind).
