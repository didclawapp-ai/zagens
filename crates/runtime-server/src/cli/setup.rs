//! Setup/bootstrap template helpers (retained for tests after CLI sunset).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::mcp::{McpConfig, McpServerConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteStatus {
    Created,
    Overwritten,
    SkippedExists,
}

pub(crate) fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory for {}", parent.display()))?;
    }
    Ok(())
}

pub(crate) fn write_template_file(path: &Path, contents: &str, force: bool) -> Result<WriteStatus> {
    ensure_parent_dir(path)?;

    if path.exists() && !force {
        return Ok(WriteStatus::SkippedExists);
    }

    let status = if path.exists() {
        WriteStatus::Overwritten
    } else {
        WriteStatus::Created
    };

    std::fs::write(path, contents)
        .with_context(|| format!("Failed to write template at {}", path.display()))?;

    Ok(status)
}

pub(crate) fn mcp_template_json() -> Result<String> {
    let mut cfg = McpConfig::default();
    cfg.servers.insert(
        "example".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["./path/to/your-mcp-server.js".to_string()],
            env: std::collections::HashMap::new(),
            url: None,
            transport: None,
            headers: std::collections::HashMap::new(),
            auth: None,
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: true,
            enabled: true,
            required: false,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
        },
    );
    serde_json::to_string_pretty(&cfg)
        .map_err(|e| anyhow!("Failed to render MCP template JSON: {e}"))
}

pub(crate) fn init_mcp_config(path: &Path, force: bool) -> Result<WriteStatus> {
    let template = mcp_template_json()?;
    write_template_file(path, &template, force)
}

pub(crate) fn skills_template(name: &str) -> String {
    format!(
        "\
---\n\
name: {name}\n\
description: Quick repo diagnostics and setup guidance\n\
allowed-tools: diagnostics, list_dir, read_file, grep_files, git_status, git_diff\n\
---\n\n\
When this skill is active:\n\
1. Run the diagnostics tool to report workspace and sandbox status.\n\
2. Skim key project files (README.md, Cargo.toml, AGENTS.md) before editing.\n\
3. Prefer small, validated changes and summarize what you verified.\n\
"
    )
}

pub(crate) fn init_skills_dir(skills_dir: &Path, force: bool) -> Result<(PathBuf, WriteStatus)> {
    std::fs::create_dir_all(skills_dir)
        .with_context(|| format!("Failed to create skills dir {}", skills_dir.display()))?;

    let skill_name = "getting-started";
    let skill_path = skills_dir.join(skill_name).join("SKILL.md");
    ensure_parent_dir(&skill_path)?;

    let status = write_template_file(&skill_path, &skills_template(skill_name), force)?;
    Ok((skill_path, status))
}

pub(crate) fn tools_readme_template() -> &'static str {
    "# Local tools\n\n\
     Drop self-describing scripts here so they can be discovered by setup/doctor.\n\n\
     Each script should start with a frontmatter-style header:\n\n\
     ```\n\
     # name: my-tool\n\
     # description: One-line summary\n\
     # usage: my-tool [args...]\n\
     ```\n"
}

pub(crate) fn tools_example_script() -> &'static str {
    "#!/usr/bin/env sh\n\
     # name: example\n\
     # description: Print a confirmation that local tool discovery works\n\
     # usage: example [name]\n\
     printf 'deepseek-runtime local tool ok: %s\\n' \"${1:-world}\"\n"
}

pub(crate) fn init_tools_dir(
    tools_dir: &Path,
    force: bool,
) -> Result<(PathBuf, WriteStatus, WriteStatus)> {
    std::fs::create_dir_all(tools_dir)
        .with_context(|| format!("Failed to create tools dir {}", tools_dir.display()))?;

    let readme_path = tools_dir.join("README.md");
    let readme_status = write_template_file(&readme_path, tools_readme_template(), force)?;

    let example_path = tools_dir.join("example.sh");
    let example_status = write_template_file(&example_path, tools_example_script(), force)?;

    Ok((tools_dir.to_path_buf(), readme_status, example_status))
}

pub(crate) fn plugins_readme_template() -> &'static str {
    "# Local plugins\n\n\
     Plugins live in subdirectories with a `PLUGIN.md` describing usage.\n"
}

pub(crate) fn plugin_example_template() -> &'static str {
    "---\n\
     name: example\n\
     description: Placeholder plugin\n\
     status: example\n\
     ---\n\n\
     Starter plugin layout for local experiments.\n"
}

pub(crate) fn init_plugins_dir(
    plugins_dir: &Path,
    force: bool,
) -> Result<(PathBuf, PathBuf, WriteStatus, WriteStatus)> {
    std::fs::create_dir_all(plugins_dir)
        .with_context(|| format!("Failed to create plugins dir {}", plugins_dir.display()))?;

    let readme_path = plugins_dir.join("README.md");
    let readme_status = write_template_file(&readme_path, plugins_readme_template(), force)?;

    let example_path = plugins_dir.join("example").join("PLUGIN.md");
    ensure_parent_dir(&example_path)?;
    let example_status = write_template_file(&example_path, plugin_example_template(), force)?;

    Ok((readme_path, example_path, readme_status, example_status))
}

pub(crate) fn deepseek_home_dir() -> PathBuf {
    zagens_config::user_data_root()
        .unwrap_or_else(|_| PathBuf::from(zagens_config::USER_DATA_DIR_NAME))
}

pub(crate) fn default_checkpoints_dir() -> PathBuf {
    deepseek_home_dir().join("sessions").join("checkpoints")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CleanPlan {
    pub(crate) targets: Vec<PathBuf>,
}

pub(crate) fn collect_clean_targets(checkpoints_dir: &Path) -> CleanPlan {
    let candidates = ["latest.json", "offline_queue.json"];
    let targets = candidates
        .iter()
        .map(|name| checkpoints_dir.join(name))
        .filter(|p| p.exists())
        .collect();
    CleanPlan { targets }
}

pub(crate) fn execute_clean_plan(plan: &CleanPlan) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::with_capacity(plan.targets.len());
    for path in &plan.targets {
        std::fs::remove_file(path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
        removed.push(path.clone());
    }
    Ok(removed)
}

pub(crate) fn dotenv_status_line(workspace: &Path) -> String {
    let dotenv = workspace.join(".env");
    if dotenv.exists() {
        return format!(".env present at {}", dotenv.display());
    }

    if workspace.join(".env.example").exists() {
        return ".env not present in workspace (run `cp .env.example .env` and edit)".to_string();
    }

    ".env not present in workspace".to_string()
}

pub(crate) fn run_setup_clean(checkpoints_dir: &Path, force: bool) -> Result<()> {
    use colored::Colorize;

    if !checkpoints_dir.exists() {
        println!(
            "Nothing to clean — checkpoints dir does not exist: {}",
            checkpoints_dir.display()
        );
        return Ok(());
    }

    let plan = collect_clean_targets(checkpoints_dir);
    if plan.targets.is_empty() {
        println!(
            "Nothing to clean — no checkpoint files in {}",
            checkpoints_dir.display()
        );
        return Ok(());
    }

    if !force {
        println!(
            "Would remove {} checkpoint file(s) (use --force to apply):",
            plan.targets.len()
        );
        for path in &plan.targets {
            println!("  · {}", path.display());
        }
        return Ok(());
    }

    let removed = execute_clean_plan(&plan)?;
    println!("{}", "Cleaned checkpoints:".bold());
    for path in &removed {
        println!("  ✓ {}", path.display());
    }
    Ok(())
}

pub(crate) fn is_command_available(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            if candidate.extension().is_none() && candidate.with_extension("exe").is_file() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiKeySource {
    Env,
    Config,
    Keyring,
    Missing,
}

pub(crate) fn resolve_api_key_source(config: &crate::config::Config) -> ApiKeySource {
    if std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .is_some()
    {
        match std::env::var("DEEPSEEK_API_KEY_SOURCE").ok().as_deref() {
            Some("config") => return ApiKeySource::Config,
            Some("keyring") => return ApiKeySource::Keyring,
            _ => {}
        }
    }

    if config
        .api_key
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty())
        || config
            .provider_config()
            .and_then(|entry| entry.api_key.as_ref())
            .is_some_and(|k| !k.trim().is_empty())
    {
        ApiKeySource::Config
    } else if std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .is_some()
    {
        ApiKeySource::Env
    } else {
        ApiKeySource::Missing
    }
}

pub(crate) fn skills_count_for(dir: &Path) -> usize {
    if !dir.exists() {
        return 0;
    }
    crate::skills::SkillRegistry::discover(dir).len()
}

pub(crate) fn merge_project_config(config: &mut crate::config::Config, workspace: &Path) {
    let path = zagens_config::workspace_meta_file_read(workspace, "config.toml");
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => return,
    };
    let project: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };
    let table = match project.as_table() {
        Some(t) => t,
        None => return,
    };

    const DENY_AT_PROJECT_SCOPE: &[&str] = &["api_key", "base_url", "provider", "mcp_config_path"];
    for key in DENY_AT_PROJECT_SCOPE {
        if table.contains_key(*key) {
            eprintln!(
                "warning: project-scope config key `{key}` is ignored — \
                 set it in `~/.zagens/config.toml` instead. \
                 (See #417 for the deny-list rationale.)"
            );
        }
    }

    for (key, field) in [
        ("model", &mut config.default_text_model),
        ("reasoning_effort", &mut config.reasoning_effort),
        ("approval_policy", &mut config.approval_policy),
        ("sandbox_mode", &mut config.sandbox_mode),
        ("notes_path", &mut config.notes_path),
    ] {
        if let Some(v) = table.get(key).and_then(toml::Value::as_str)
            && !v.is_empty()
        {
            let is_escalation = matches!(
                (key, v),
                ("approval_policy", "auto") | ("sandbox_mode", "danger-full-access")
            );
            if is_escalation {
                eprintln!(
                    "warning: project-scope `{key} = \"{v}\"` is ignored — \
                     project config cannot escalate to the loosest value. \
                     (See #417.)"
                );
                continue;
            }
            *field = Some(v.to_string());
        }
    }

    if let Some(v) = table.get("max_subagents").and_then(toml::Value::as_integer)
        && v > 0
    {
        config.max_subagents = Some((v as usize).clamp(1, crate::config::MAX_SUBAGENTS));
    }
    if let Some(v) = table.get("allow_shell").and_then(toml::Value::as_bool) {
        config.allow_shell = Some(v);
    }

    if let Some(arr) = table.get("instructions").and_then(toml::Value::as_array) {
        let entries: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .filter(|s| !s.trim().is_empty())
            .collect();
        config.instructions = Some(entries);
    }
}

pub(crate) fn default_tools_dir() -> PathBuf {
    deepseek_home_dir().join("tools")
}

pub(crate) fn default_plugins_dir() -> PathBuf {
    deepseek_home_dir().join("plugins")
}

pub(crate) fn count_dir_entries(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().count())
        .unwrap_or(0)
}

pub(crate) fn run_setup(
    config: &crate::config::Config,
    workspace: &Path,
    args: crate::cli::args::SetupArgs,
) -> Result<()> {
    use colored::Colorize;

    if args.status {
        return run_setup_status(config, workspace);
    }
    if args.clean {
        return run_setup_clean(&default_checkpoints_dir(), args.force);
    }

    let any_explicit = args.mcp || args.skills || args.tools || args.plugins;
    let run_mcp = args.mcp || args.all || !any_explicit;
    let run_skills = args.skills || args.all || !any_explicit;
    let run_tools = args.tools || args.all;
    let run_plugins = args.plugins || args.all;

    println!("{}", "Zagens Setup".bold());
    println!("Workspace: {}", crate::utils::display_path(workspace));

    if run_mcp {
        let mcp_path = config.mcp_config_path();
        let status = init_mcp_config(&mcp_path, args.force)?;
        report_write_status("MCP config", &mcp_path, status);
        println!("    Next: edit the file, then run `zagens mcp list` or `zagens mcp tools`.");
    }

    if run_skills {
        let skills_dir = if args.local {
            workspace.join("skills")
        } else {
            config.skills_dir()
        };
        let (skill_path, status) = init_skills_dir(&skills_dir, args.force)?;
        report_write_status("Example skill", &skill_path, status);
        println!(
            "    Skills dir: {}",
            crate::utils::display_path(&skills_dir)
        );
    }

    if run_tools {
        let tools_dir = default_tools_dir();
        let (_, readme_status, example_status) = init_tools_dir(&tools_dir, args.force)?;
        report_write_status("Tools README", &tools_dir.join("README.md"), readme_status);
        report_write_status(
            "Tools example",
            &tools_dir.join("example.sh"),
            example_status,
        );
    }

    if run_plugins {
        let plugins_dir = default_plugins_dir();
        let (_, example_path, readme_status, example_status) =
            init_plugins_dir(&plugins_dir, args.force)?;
        report_write_status(
            "Plugins README",
            &plugins_dir.join("README.md"),
            readme_status,
        );
        report_write_status("Plugin example", &example_path, example_status);
    }

    Ok(())
}

fn report_write_status(label: &str, path: &Path, status: WriteStatus) {
    match status {
        WriteStatus::Created => println!("  ✓ Created {label} at {}", path.display()),
        WriteStatus::Overwritten => println!("  ✓ Overwrote {label} at {}", path.display()),
        WriteStatus::SkippedExists => println!("  · {label} already exists at {}", path.display()),
    }
}

pub(crate) fn run_setup_status(config: &crate::config::Config, workspace: &Path) -> Result<()> {
    use colored::Colorize;

    println!("{}", "Zagens Status".bold());
    println!("workspace: {}", workspace.display());

    match resolve_api_key_source(config) {
        ApiKeySource::Env => println!("  ✓ api_key: set via DEEPSEEK_API_KEY"),
        ApiKeySource::Keyring => println!("  ✓ api_key: set via OS keyring"),
        ApiKeySource::Config => println!("  ✓ api_key: set via config"),
        ApiKeySource::Missing => {
            println!("  ✗ api_key: missing (run `zagens login` or set DEEPSEEK_API_KEY)");
        }
    }
    println!("  · base_url: {}", config.deepseek_base_url());
    println!(
        "  · default_model: {}",
        config
            .default_text_model
            .clone()
            .unwrap_or_else(|| config.default_model())
    );
    println!("  · {}", dotenv_status_line(workspace));

    let mcp_path = config.mcp_config_path();
    println!(
        "  · mcp_config: {} ({})",
        mcp_path.display(),
        if mcp_path.exists() {
            "present"
        } else {
            "missing"
        }
    );

    let skills_dir = config.skills_dir();
    println!(
        "  · skills: {} ({} discovered)",
        skills_dir.display(),
        skills_count_for(&skills_dir)
    );

    let tools_dir = default_tools_dir();
    println!(
        "  · tools: {} ({} entries)",
        tools_dir.display(),
        if tools_dir.exists() {
            count_dir_entries(&tools_dir)
        } else {
            0
        }
    );

    Ok(())
}
