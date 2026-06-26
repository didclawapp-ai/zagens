//! Extended context diagnostics for `zagens doctor` (instructions, skills, hooks, config layers).

use std::path::Path;

use serde_json::{Value, json};

use crate::cli::context::display_path;
use crate::config::Config;
use crate::project_doc;
use crate::prompts::merge_instruction_paths_with_pick_rules;
use crate::skills;

/// Build the "context" section of a doctor report (instructions, skills, hooks, merge layers).
#[must_use]
pub fn build_context_report(config: &Config, workspace: &Path) -> Value {
    json!({
        "config_layers": config_layers(workspace),
        "instructions": instruction_report(config, workspace),
        "skills": skills_report(workspace),
        "hooks": hooks_report(config),
        "memory": memory_report(config),
        "merge_notes": merge_notes(),
    })
}

fn config_layers(workspace: &Path) -> Value {
    let user_path = crate::cli::context::default_config_path();
    let project_path = zagens_config::workspace_meta_file_read(workspace, "config.toml");
    let env_overrides = active_env_override_keys();

    json!({
        "user_config": {
            "path": user_path.display().to_string(),
            "present": user_path.exists(),
        },
        "project_config": {
            "path": project_path.display().to_string(),
            "present": project_path.exists(),
        },
        "env_overrides": env_overrides,
    })
}

fn active_env_override_keys() -> Vec<&'static str> {
    const KEYS: &[&str] = &[
        "DEEPSEEK_PROVIDER",
        "DEEPSEEK_BASE_URL",
        "DEEPSEEK_MODEL",
        "DEEPSEEK_DEFAULT_TEXT_MODEL",
        "DEEPSEEK_API_KEY",
        "DEEPSEEK_SKILLS_DIR",
        "DEEPSEEK_MCP_CONFIG",
        "DEEPSEEK_MEMORY",
        "DEEPSEEK_MEMORY_PATH",
        "DEEPSEEK_ALLOW_SHELL",
        "DEEPSEEK_TRUST_MODE",
        "DEEPSEEK_APPROVAL_POLICY",
        "DEEPSEEK_SANDBOX_MODE",
        "DEEPSEEK_MAX_SUBAGENTS",
        "DEEPSEEK_PROFILE",
        "DEEPSEEK_CONFIG_PATH",
    ];
    KEYS.iter()
        .copied()
        .filter(|key| std::env::var_os(key).is_some())
        .collect()
}

fn instruction_report(config: &Config, workspace: &Path) -> Value {
    let explicit = config
        .instructions
        .as_ref()
        .is_some_and(|entries| entries.iter().any(|s| !s.trim().is_empty()));
    let source = if explicit {
        "explicit_config"
    } else {
        "auto_discovered"
    };

    let config_paths = config.instructions_paths(workspace);
    let merged = merge_instruction_paths_with_pick_rules(workspace, config_paths);

    let paths: Vec<Value> = merged
        .iter()
        .map(|path| {
            let kind = instruction_path_kind(path, workspace);
            json!({
                "path": path.display().to_string(),
                "present": path.is_file(),
                "kind": kind,
            })
        })
        .collect();

    let project_docs: Vec<Value> = project_doc::discover_paths(workspace)
        .into_iter()
        .map(|path| {
            json!({
                "path": path.display().to_string(),
                "present": path.is_file(),
            })
        })
        .collect();

    json!({
        "source": source,
        "paths": paths,
        "project_docs": project_docs,
    })
}

fn instruction_path_kind(path: &Path, workspace: &Path) -> &'static str {
    let pick = zagens_config::workspace_meta_file_read(workspace, "pick-rules.md");
    if paths_same_file(path, &pick) {
        return "pick_rules";
    }
    if path.file_name().is_some_and(|n| n == "PROJECT_RULES.md") {
        return "project_rules";
    }
    if path.extension().is_some_and(|e| e == "mdc") {
        return "cursor_rule";
    }
    "config"
}

fn paths_same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn skills_report(workspace: &Path) -> Value {
    let directories: Vec<Value> = skills::skills_directories(workspace)
        .iter()
        .map(|dir| {
            let registry = skills::SkillRegistry::discover(dir);
            json!({
                "path": dir.display().to_string(),
                "count": registry.len(),
            })
        })
        .collect();

    let registry = skills::discover_in_workspace(workspace);
    let skill_entries: Vec<Value> = registry
        .list()
        .iter()
        .map(|skill| {
            json!({
                "name": skill.name,
                "path": skill.path.display().to_string(),
            })
        })
        .collect();

    json!({
        "directories": directories,
        "skills": skill_entries,
        "warnings": registry.warnings(),
    })
}

fn hooks_report(config: &Config) -> Value {
    let hooks_cfg = config.hooks_config();
    let entries: Vec<Value> = hooks_cfg
        .hooks
        .iter()
        .map(|hook| {
            json!({
                "name": hook.name.as_deref().unwrap_or("(unnamed)"),
                "event": hook.event.as_str(),
                "background": hook.background,
            })
        })
        .collect();

    json!({
        "enabled": hooks_cfg.enabled,
        "count": hooks_cfg.hooks.len(),
        "entries": entries,
    })
}

fn memory_report(config: &Config) -> Value {
    json!({
        "enabled": config.memory_enabled(),
        "path": config.memory_path().display().to_string(),
        "present": config.memory_path().is_file(),
    })
}

fn merge_notes() -> Value {
    json!([
        "User config loads from ~/.zagens/config.toml (or DEEPSEEK_CONFIG_PATH), then env overrides apply.",
        "Project .zagens/config.toml merges selected keys; api_key, base_url, provider, and mcp_config_path are ignored at project scope.",
        "Project instructions replace the user instructions array wholesale — list global paths inside the project array if you need both.",
        "Project config cannot escalate approval_policy to \"auto\" or sandbox_mode to \"danger-full-access\".",
        ".zagens/pick-rules.md prepends to the instructions list when non-empty.",
    ])
}

pub fn print_context_human(config: &Config, workspace: &Path) {
    use colored::Colorize;

    println!();
    println!("{}", "Config layers".bold());
    let user_path = crate::cli::context::default_config_path();
    let project_path = zagens_config::workspace_meta_file_read(workspace, "config.toml");
    println!(
        "  user: {} ({})",
        display_path(&user_path),
        if user_path.exists() {
            "present"
        } else {
            "missing"
        }
    );
    println!(
        "  project: {} ({})",
        display_path(&project_path),
        if project_path.exists() {
            "present"
        } else {
            "absent"
        }
    );
    let env_keys = active_env_override_keys();
    if env_keys.is_empty() {
        println!("  env overrides: none");
    } else {
        println!("  env overrides: {}", env_keys.join(", "));
    }

    println!();
    println!("{}", "Instructions".bold());
    let report = instruction_report(config, workspace);
    println!(
        "  source: {}",
        report["source"].as_str().unwrap_or("unknown")
    );
    if let Some(paths) = report["paths"].as_array() {
        if paths.is_empty() {
            println!("  (no instruction files resolved)");
        }
        for entry in paths {
            let path = entry["path"].as_str().unwrap_or("?");
            let kind = entry["kind"].as_str().unwrap_or("?");
            let present = entry["present"].as_bool().unwrap_or(false);
            let mark = if present { "✓" } else { "✗" };
            println!("  {mark} [{kind}] {path}");
        }
    }
    if let Some(docs) = report["project_docs"].as_array()
        && !docs.is_empty()
    {
        println!("  project docs:");
        for doc in docs {
            let path = doc["path"].as_str().unwrap_or("?");
            let present = doc["present"].as_bool().unwrap_or(false);
            let mark = if present { "✓" } else { "✗" };
            println!("    {mark} {path}");
        }
    }

    println!();
    println!("{}", "Skills".bold());
    let skills = skills_report(workspace);
    if let Some(dirs) = skills["directories"].as_array() {
        for dir in dirs {
            let path = dir["path"].as_str().unwrap_or("?");
            let count = dir["count"].as_u64().unwrap_or(0);
            println!("  · {path} ({count})");
        }
    }
    if let Some(entries) = skills["skills"].as_array() {
        if entries.is_empty() {
            println!("  (no skills discovered)");
        }
        for skill in entries {
            let name = skill["name"].as_str().unwrap_or("?");
            let path = skill["path"].as_str().unwrap_or("?");
            println!("  - {name}: {path}");
        }
    }
    if let Some(warnings) = skills["warnings"].as_array()
        && !warnings.is_empty()
    {
        for warning in warnings {
            if let Some(text) = warning.as_str() {
                println!("  ! {text}");
            }
        }
    }

    println!();
    println!("{}", "Hooks".bold());
    let hooks = hooks_report(config);
    let enabled = hooks["enabled"].as_bool().unwrap_or(true);
    let count = hooks["count"].as_u64().unwrap_or(0);
    if !enabled {
        println!("  disabled globally");
    } else if count == 0 {
        println!("  (none configured)");
    } else if let Some(entries) = hooks["entries"].as_array() {
        for entry in entries {
            let name = entry["name"].as_str().unwrap_or("?");
            let event = entry["event"].as_str().unwrap_or("?");
            println!("  - {name} ({event})");
        }
    }

    println!();
    println!("{}", "Memory".bold());
    let memory = memory_report(config);
    let enabled = memory["enabled"].as_bool().unwrap_or(false);
    let path = memory["path"].as_str().unwrap_or("?");
    let present = memory["present"].as_bool().unwrap_or(false);
    println!(
        "  enabled: {enabled}, file: {path} ({})",
        if present { "present" } else { "absent" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn context_report_includes_merge_notes() {
        let config = Config::default();
        let tmp = tempfile::tempdir().unwrap();
        let report = build_context_report(&config, tmp.path());
        assert!(report.get("merge_notes").is_some());
        assert!(report.get("instructions").is_some());
    }

    #[test]
    fn instruction_kind_detects_pick_rules() {
        let tmp = tempfile::tempdir().unwrap();
        let pick = zagens_config::workspace_meta_file_read(tmp.path(), "pick-rules.md");
        std::fs::create_dir_all(pick.parent().unwrap()).unwrap();
        std::fs::write(&pick, "pick body").unwrap();
        assert_eq!(instruction_path_kind(&pick, tmp.path()), "pick_rules");
    }
}
