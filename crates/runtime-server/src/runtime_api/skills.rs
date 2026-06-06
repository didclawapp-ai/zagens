//! Skills install/list HTTP handlers (R-003 A4.5).

use std::fs;
use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::skills::install::{
    DEFAULT_MAX_SIZE_BYTES, InstallError, InstallOutcome, InstallSource, import_local_directory,
};
use crate::skills::{SkillRegistry, install};

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Serialize)]
pub(crate) struct SkillEntry {
    name: String,
    description: String,
    path: PathBuf,
}

#[derive(Debug, Serialize)]
pub(crate) struct SkillsResponse {
    directory: PathBuf,
    warnings: Vec<String>,
    skills: Vec<SkillEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSkillRequest {
    /// Directory name for the skill (becomes `<skills_root>/<name>/SKILL.md`).
    name: String,
    /// `global` → configured skills dir (default `~/.deepseek/skills`). `workspace`
    /// → workspace `.agents/skills` or `skills/`, creating `.agents/skills` when
    /// neither exists. Ignored when `parent_directory` is set.
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    /// Optional skills root chosen in the desktop folder picker; must match an
    /// allowed root after canonicalization (global dir or an existing workspace
    /// skills directory).
    #[serde(default)]
    parent_directory: Option<PathBuf>,
}

fn default_create_skill_scope() -> String {
    "workspace".to_string()
}

#[derive(Debug, Serialize)]
pub(crate) struct CreateSkillResponse {
    skill: SkillEntry,
    /// Directory scanned by `GET /v1/skills` (may differ from `skills_root`).
    directory: PathBuf,
    /// Skills root where this skill was created (`<skills_root>/<name>/SKILL.md`).
    skills_root: PathBuf,
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ImportSkillLocalRequest {
    /// Directory containing `SKILL.md` (the skill bundle root).
    source_directory: PathBuf,
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    #[serde(default)]
    parent_directory: Option<PathBuf>,
    /// When true, replace an existing skill directory with the same frontmatter name.
    #[serde(default)]
    replace: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstallSkillRemoteRequest {
    /// `github:owner/repo`, registry name, or direct tarball URL.
    spec: String,
    #[serde(default = "default_create_skill_scope")]
    scope: String,
    #[serde(default)]
    parent_directory: Option<PathBuf>,
    #[serde(default)]
    replace: bool,
}

pub(crate) async fn list_skills(
    State(state): State<RuntimeApiState>,
) -> Result<Json<SkillsResponse>, ApiError> {
    let skills_dir = resolve_skills_dir(&state.config, &state.workspace);
    let registry = SkillRegistry::discover(&skills_dir);
    let skills = registry
        .list()
        .iter()
        .map(|skill| SkillEntry {
            name: skill.name.clone(),
            description: skill.description.clone(),
            path: skills_dir.join(&skill.name).join("SKILL.md"),
        })
        .collect();
    Ok(Json(SkillsResponse {
        directory: skills_dir,
        warnings: registry.warnings().to_vec(),
        skills,
    }))
}

pub(crate) async fn create_skill(
    State(state): State<RuntimeApiState>,
    Json(req): Json<CreateSkillRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    let name = validate_skill_directory_name(&req.name)?;
    let name_for_task = name.clone();
    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory.clone();
    let scope = req.scope.clone();

    let (skills_root_used, skill_md_path, warnings) = tokio::task::spawn_blocking(move || {
        let root =
            resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)?;
        fs::create_dir_all(&root).map_err(|e| {
            ApiError::internal(format!(
                "failed to create skills directory {}: {e}",
                root.display()
            ))
        })?;

        let skill_dir = root.join(&name_for_task);
        if skill_dir.exists() {
            return Err(ApiError::conflict(format!(
                "skill directory already exists: {}",
                skill_dir.display()
            )));
        }
        fs::create_dir_all(&skill_dir)
            .map_err(|e| ApiError::internal(format!("failed to create skill directory: {e}")))?;
        let md_path = skill_dir.join("SKILL.md");
        if md_path.exists() {
            return Err(ApiError::conflict("SKILL.md already exists".to_string()));
        }
        let body = skill_md_template(&name_for_task);
        fs::write(&md_path, body).map_err(|e| ApiError::internal(e.to_string()))?;

        let registry = SkillRegistry::discover(&root);
        let warnings = registry.warnings().to_vec();
        Ok::<_, ApiError>((root, md_path, warnings))
    })
    .await
    .map_err(|e| ApiError::internal(format!("create skill task: {e}")))??;

    let list_directory = resolve_skills_dir(&state.config, &state.workspace);
    let reg_list = SkillRegistry::discover(&skills_root_used);
    let description = reg_list
        .get(&name)
        .map(|s| s.description.clone())
        .unwrap_or_else(|| "Describe what this skill does.".to_string());

    let skill_entry = SkillEntry {
        name,
        description,
        path: skill_md_path,
    };

    Ok((
        StatusCode::CREATED,
        Json(CreateSkillResponse {
            skill: skill_entry,
            directory: list_directory,
            skills_root: skills_root_used,
            warnings,
        }),
    ))
}

pub(crate) async fn import_skill_local(
    State(state): State<RuntimeApiState>,
    Json(req): Json<ImportSkillLocalRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    if req.source_directory.as_os_str().is_empty() {
        return Err(ApiError::bad_request("source_directory is required"));
    }
    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory.clone();
    let scope = req.scope.clone();
    let source_directory = req.source_directory.clone();
    let replace = req.replace;

    let (skills_root_used, installed) = tokio::task::spawn_blocking(move || {
        let root =
            resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)?;
        fs::create_dir_all(&root).map_err(|e| {
            ApiError::internal(format!(
                "failed to create skills directory {}: {e}",
                root.display()
            ))
        })?;
        let installed =
            import_local_directory(&source_directory, &root, replace, DEFAULT_MAX_SIZE_BYTES)
                .map_err(map_skill_install_api_error)?;
        Ok::<_, ApiError>((root, installed))
    })
    .await
    .map_err(|e| ApiError::internal(format!("import skill task: {e}")))??;

    build_skill_mutation_response(
        &state,
        &skills_root_used,
        &installed.name,
        installed.path.join("SKILL.md"),
    )
    .await
    .map(|json| (StatusCode::CREATED, json))
}

pub(crate) async fn install_skill_remote(
    State(state): State<RuntimeApiState>,
    Json(req): Json<InstallSkillRemoteRequest>,
) -> Result<(StatusCode, Json<CreateSkillResponse>), ApiError> {
    let spec = req.spec.trim();
    if spec.is_empty() {
        return Err(ApiError::bad_request("spec is required"));
    }
    let source = InstallSource::parse(spec)
        .map_err(|e| ApiError::bad_request(format!("invalid install spec: {e}")))?;

    let config = state.config.clone();
    let workspace = state.workspace.clone();
    let parent_directory = req.parent_directory;
    let scope = req.scope;
    let replace = req.replace;
    let skills_cfg = config.skills.as_ref();
    let registry_url = skills_cfg
        .map(crate::config::SkillsConfig::registry_url)
        .unwrap_or_else(|| install::DEFAULT_REGISTRY_URL.to_string());
    let max_size = skills_cfg
        .map(crate::config::SkillsConfig::max_install_size_bytes)
        .unwrap_or(DEFAULT_MAX_SIZE_BYTES);
    let network = config.network.clone().unwrap_or_default().into_runtime();

    let skills_root = tokio::task::spawn_blocking({
        let config = config.clone();
        let workspace = workspace.clone();
        move || resolve_create_skill_parent(&config, &workspace, parent_directory.as_ref(), &scope)
    })
    .await
    .map_err(|e| ApiError::internal(format!("resolve skills root: {e}")))??;

    fs::create_dir_all(&skills_root).map_err(|e| {
        ApiError::internal(format!(
            "failed to create skills directory {}: {e}",
            skills_root.display()
        ))
    })?;

    let outcome = install::install_with_registry(
        source,
        &skills_root,
        max_size,
        &network,
        replace,
        &registry_url,
    )
    .await
    .map_err(|e| ApiError::internal(format!("skill install: {e:#}")))?;

    let installed = match outcome {
        InstallOutcome::Installed(skill) => skill,
        InstallOutcome::NeedsApproval(host) => {
            return Err(ApiError::forbidden(format!(
                "network host '{host}' requires approval; add it to [network] allow in config.toml and retry"
            )));
        }
        InstallOutcome::NetworkDenied(host) => {
            return Err(ApiError::forbidden(format!(
                "network host '{host}' is denied by policy"
            )));
        }
    };

    build_skill_mutation_response(
        &state,
        &skills_root,
        &installed.name,
        installed.path.join("SKILL.md"),
    )
    .await
    .map(|json| (StatusCode::CREATED, json))
}

async fn build_skill_mutation_response(
    state: &RuntimeApiState,
    skills_root_used: &Path,
    skill_name: &str,
    skill_md_path: PathBuf,
) -> Result<Json<CreateSkillResponse>, ApiError> {
    let list_directory = resolve_skills_dir(&state.config, &state.workspace);
    let reg_list = SkillRegistry::discover(skills_root_used);
    let description = reg_list
        .get(skill_name)
        .map(|s| s.description.clone())
        .unwrap_or_else(|| "Describe what this skill does.".to_string());
    let warnings = reg_list.warnings().to_vec();
    Ok(Json(CreateSkillResponse {
        skill: SkillEntry {
            name: skill_name.to_string(),
            description,
            path: skill_md_path,
        },
        directory: list_directory,
        skills_root: skills_root_used.to_path_buf(),
        warnings,
    }))
}

fn map_skill_install_api_error(err: anyhow::Error) -> ApiError {
    if let Some(InstallError::AlreadyInstalled(name)) = err.downcast_ref() {
        return ApiError::conflict(format!(
            "skill already installed: {name} (set replace=true to overwrite)"
        ));
    }
    ApiError::bad_request(format!("{err:#}"))
}

pub(crate) fn validate_skill_directory_name(raw: &str) -> Result<String, ApiError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ApiError::bad_request("name is required"));
    }
    if s.len() > 96 {
        return Err(ApiError::bad_request("name is too long (max 96)"));
    }
    if s == "." || s == ".." {
        return Err(ApiError::bad_request("invalid skill name"));
    }
    for c in s.chars() {
        if c == '/' || c == '\\' {
            return Err(ApiError::bad_request(
                "skill name must not contain path separators",
            ));
        }
        if !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            return Err(ApiError::bad_request(
                "skill name may only contain ASCII letters, digits, '.', '-', and '_'",
            ));
        }
    }
    Ok(s.to_string())
}

fn skill_md_template(name: &str) -> String {
    format!(
        r#"---
name: {name}
description: Describe what this skill does.
allowed-tools: read_file, list_dir
---

Write skill instructions here.
"#
    )
}

/// Skills roots that exist (and global dir, which is created if missing) for
/// validating `parent_directory` from the desktop folder picker.
fn allowed_skill_roots_for_picker(
    config: &Config,
    workspace: &std::path::Path,
) -> Result<Vec<PathBuf>, ApiError> {
    let mut roots: Vec<PathBuf> = Vec::new();

    let global = config.skills_dir();
    fs::create_dir_all(&global).map_err(|e| {
        ApiError::internal(format!(
            "failed to ensure global skills dir {}: {e}",
            global.display()
        ))
    })?;
    let global_canon = global.canonicalize().map_err(|e| {
        ApiError::internal(format!("failed to canonicalize global skills dir: {e}"))
    })?;
    roots.push(global_canon);

    let ws = workspace
        .canonicalize()
        .map_err(|e| ApiError::bad_request(format!("workspace path could not be resolved: {e}")))?;
    for rel in [".agents/skills", "skills"] {
        let p = ws.join(rel);
        if p.is_dir() {
            if let Ok(c) = p.canonicalize() {
                roots.push(c);
            }
        }
    }

    roots.sort_unstable();
    roots.dedup();
    Ok(roots)
}

fn resolve_create_skill_parent(
    config: &Config,
    workspace: &std::path::Path,
    parent_directory: Option<&PathBuf>,
    scope: &str,
) -> Result<PathBuf, ApiError> {
    if let Some(user_parent) = parent_directory {
        let user = user_parent.canonicalize().map_err(|_| {
            ApiError::bad_request(
                "parent_directory must exist and be readable (pick an existing skills root)",
            )
        })?;
        let allowed = allowed_skill_roots_for_picker(config, workspace)?;
        if !allowed.iter().any(|r| r == &user) {
            return Err(ApiError::bad_request(
                "parent_directory is not an allowed skills root; use global or workspace skills directory",
            ));
        }
        return Ok(user);
    }

    match scope.trim().to_ascii_lowercase().as_str() {
        "global" => {
            let global = config.skills_dir();
            fs::create_dir_all(&global).map_err(|e| {
                ApiError::internal(format!(
                    "failed to create global skills dir {}: {e}",
                    global.display()
                ))
            })?;
            Ok(global)
        }
        "workspace" => {
            let ws = workspace.canonicalize().map_err(|e| {
                ApiError::bad_request(format!("workspace path could not be resolved: {e}"))
            })?;
            let agents = ws.join(".agents").join("skills");
            let flat = ws.join("skills");
            if agents.is_dir() {
                Ok(agents)
            } else if flat.is_dir() {
                Ok(flat)
            } else {
                fs::create_dir_all(&agents).map_err(|e| {
                    ApiError::internal(format!(
                        "failed to create workspace skills dir {}: {e}",
                        agents.display()
                    ))
                })?;
                Ok(agents)
            }
        }
        _ => Err(ApiError::bad_request(
            "scope must be \"global\" or \"workspace\" (or pass parent_directory)",
        )),
    }
}

fn resolve_skills_dir(config: &Config, workspace: &std::path::Path) -> PathBuf {
    let agents_skills = workspace.join(".agents").join("skills");
    if agents_skills.exists() {
        return agents_skills;
    }
    let local_skills = workspace.join("skills");
    if local_skills.exists() {
        return local_skills;
    }
    config.skills_dir()
}
