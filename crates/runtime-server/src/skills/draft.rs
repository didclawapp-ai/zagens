//! H4 skill drafting — model writes to `.zagens/skill-drafts/`, human promotes via CLI.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zagens_config::workspace_meta_file_write;
use zagens_core::long_horizon::HarnessContract;

use crate::skills::install::{
    DEFAULT_MAX_SIZE_BYTES, INSTALLED_FROM_MARKER, import_local_directory,
};
use crate::skills::{agents_global_skills_dir, default_skills_dir};

pub const SKILL_DRAFTS_REL: &str = "skill-drafts";
pub const HUMAN_REVIEWED_MARKER: &str = ".human-reviewed";
pub const MAX_SKILL_BODY_BYTES: usize = 256 * 1024;
pub const MAX_HARNESS_TOML_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDraftRecord {
    pub name: String,
    pub path: PathBuf,
    pub has_harness: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftWriteOutcome {
    pub name: String,
    pub draft_dir: PathBuf,
    pub harness_valid: bool,
    pub harness_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromoteOutcome {
    pub name: String,
    pub installed_path: PathBuf,
    pub skills_dir: PathBuf,
}

/// Skill id: lowercase ASCII, digits, hyphens; 1–64 chars; no leading/trailing hyphen.
pub fn validate_skill_id(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 {
        bail!("skill id must be 1–64 characters");
    }
    if name.starts_with('-') || name.ends_with('-') {
        bail!("skill id must not start or end with `-`");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        bail!("skill id must use lowercase letters, digits, and hyphens only");
    }
    Ok(())
}

pub fn skill_draft_dir(workspace: &Path, name: &str) -> PathBuf {
    workspace_meta_file_write(workspace, &format!("{SKILL_DRAFTS_REL}/{name}"))
}

pub fn list_drafts(workspace: &Path) -> Result<Vec<SkillDraftRecord>> {
    let root = workspace_meta_file_write(workspace, SKILL_DRAFTS_REL);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in fs::read_dir(&root).with_context(|| root.display().to_string())? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.join("SKILL.md").is_file() {
            out.push(SkillDraftRecord {
                name,
                has_harness: path.join("harness.toml").is_file(),
                path,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn write_draft(
    workspace: &Path,
    name: &str,
    description: &str,
    body: &str,
    harness_toml: Option<&str>,
    replace: bool,
) -> Result<DraftWriteOutcome> {
    validate_skill_id(name)?;
    let description = description.trim();
    if description.is_empty() {
        bail!("description must not be empty");
    }
    if description.len() > 512 {
        bail!("description must be at most 512 characters");
    }
    let body = body.trim();
    if body.is_empty() {
        bail!("body must not be empty");
    }
    if body.len() > MAX_SKILL_BODY_BYTES {
        bail!("body exceeds {MAX_SKILL_BODY_BYTES} byte limit");
    }

    let mut harness_valid = true;
    let mut harness_warnings = Vec::new();
    if let Some(raw) = harness_toml {
        if raw.len() > MAX_HARNESS_TOML_BYTES {
            bail!("harness_toml exceeds {MAX_HARNESS_TOML_BYTES} byte limit");
        }
        let contract = HarnessContract::parse_toml(raw)
            .context("harness_toml is not valid HarnessContract TOML")?;
        let report = contract.validate();
        harness_valid = report.ok;
        harness_warnings = report.warnings;
        if !report.ok {
            bail!(
                "harness_toml failed validation: {}",
                report.errors.join("; ")
            );
        }
    }

    let draft_dir = skill_draft_dir(workspace, name);
    if draft_dir.exists() && !replace {
        bail!(
            "draft `{name}` already exists at {} — pass replace=true to overwrite",
            draft_dir.display()
        );
    }
    fs::create_dir_all(&draft_dir)
        .with_context(|| format!("create draft dir {}", draft_dir.display()))?;

    let skill_md = format_skill_md(name, description, body);
    fs::write(draft_dir.join("SKILL.md"), skill_md)
        .with_context(|| format!("write {}", draft_dir.join("SKILL.md").display()))?;

    if let Some(raw) = harness_toml {
        fs::write(draft_dir.join("harness.toml"), raw.trim())
            .with_context(|| format!("write {}", draft_dir.join("harness.toml").display()))?;
    } else if draft_dir.join("harness.toml").exists() && replace {
        fs::remove_file(draft_dir.join("harness.toml")).ok();
    }

    Ok(DraftWriteOutcome {
        name: name.to_string(),
        draft_dir,
        harness_valid,
        harness_warnings,
    })
}

pub fn promote_draft(
    workspace: &Path,
    name: &str,
    global: bool,
    replace: bool,
) -> Result<PromoteOutcome> {
    validate_skill_id(name)?;
    let draft_dir = skill_draft_dir(workspace, name);
    if !draft_dir.join("SKILL.md").is_file() {
        bail!(
            "no draft `{name}` at {} — run draft_skill or check `zagens skill drafts list`",
            draft_dir.display()
        );
    }

    if let Some(raw) = fs::read_to_string(draft_dir.join("harness.toml")).ok() {
        let contract = HarnessContract::parse_toml(&raw)?;
        let report = contract.validate();
        if !report.ok {
            bail!(
                "draft harness.toml failed validation: {}",
                report.errors.join("; ")
            );
        }
    }

    let skills_dir = resolve_promote_skills_dir(workspace, global)?;
    let installed =
        import_local_directory(&draft_dir, &skills_dir, replace, DEFAULT_MAX_SIZE_BYTES)?;

    let reviewed = serde_json::json!({
        "promoted_at": chrono::Utc::now().to_rfc3339(),
        "source": "draft_skill",
        "draft_path": draft_dir.display().to_string(),
        "reviewer": "cli_promote",
    });
    fs::write(
        installed.path.join(HUMAN_REVIEWED_MARKER),
        serde_json::to_string_pretty(&reviewed)?,
    )
    .with_context(|| {
        format!(
            "write {}",
            installed.path.join(HUMAN_REVIEWED_MARKER).display()
        )
    })?;

    // Mark community-style install provenance so update/uninstall paths recognize it.
    let marker_body = serde_json::json!({
        "spec": format!("draft:{}", draft_dir.display()),
        "url": "",
        "checksum": installed.source_checksum,
    })
    .to_string();
    fs::write(installed.path.join(INSTALLED_FROM_MARKER), marker_body)?;

    Ok(PromoteOutcome {
        name: installed.name,
        installed_path: installed.path,
        skills_dir,
    })
}

fn resolve_promote_skills_dir(workspace: &Path, global: bool) -> Result<PathBuf> {
    let dir = if global {
        agents_global_skills_dir().unwrap_or_else(default_skills_dir)
    } else {
        workspace.join(".agents").join("skills")
    };
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    Ok(dir)
}

fn format_skill_md(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n{body}\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_skill_id_rejects_invalid() {
        assert!(validate_skill_id("").is_err());
        assert!(validate_skill_id("-bad").is_err());
        assert!(validate_skill_id("Bad").is_err());
        assert!(validate_skill_id("ok-skill").is_ok());
    }

    #[test]
    fn write_and_list_draft() {
        let dir = tempdir().unwrap();
        let out = write_draft(
            dir.path(),
            "demo-skill",
            "Demo skill",
            "# Steps\n1. Do thing\n",
            None,
            false,
        )
        .unwrap();
        assert!(out.draft_dir.join("SKILL.md").is_file());
        let drafts = list_drafts(dir.path()).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].name, "demo-skill");
    }

    #[test]
    fn promote_installs_to_workspace_agents_skills() {
        let dir = tempdir().unwrap();
        write_draft(
            dir.path(),
            "promote-me",
            "x",
            "body",
            Some(
                r#"
schema_version = 1
[harness]
id = "promote-me"
[[verify]]
id = "ok"
predicate = "file_exists"
args = { path = "README.md" }
"#,
            ),
            false,
        )
        .unwrap();
        let out = promote_draft(dir.path(), "promote-me", false, false).unwrap();
        assert!(out.installed_path.join("SKILL.md").is_file());
        assert!(out.installed_path.join(HUMAN_REVIEWED_MARKER).is_file());
    }
}
