//! System-skill installer: bundles skill-creator, audit-repo, multi-search-engine,
//! zagens-office (external CLI), and auto-installs on first launch.

use std::fs;
use std::path::Path;

/// Bump when bundled skill set changes so existing installs refresh.
const BUNDLED_SKILL_VERSION: &str = "13";

struct BundledFile {
    path: &'static str,
    body: &'static str,
}

struct BundledSkill {
    name: &'static str,
    files: &'static [BundledFile],
}

const SKILL_CREATOR: BundledSkill = BundledSkill {
    name: "skill-creator",
    files: &[BundledFile {
        path: "SKILL.md",
        body: include_str!("../../assets/skills/skill-creator/SKILL.md"),
    }],
};

const AUDIT_REPO: BundledSkill = BundledSkill {
    name: "audit-repo",
    files: &[BundledFile {
        path: "SKILL.md",
        body: include_str!("../../assets/skills/audit-repo/SKILL.md"),
    }],
};

const MULTI_SEARCH_ENGINE: BundledSkill = BundledSkill {
    name: "multi-search-engine",
    files: &[
        BundledFile {
            path: "SKILL.md",
            body: include_str!("../../assets/skills/multi-search-engine/SKILL.md"),
        },
        BundledFile {
            path: "config.json",
            body: include_str!("../../assets/skills/multi-search-engine/config.json"),
        },
        BundledFile {
            path: "CHANGELOG.md",
            body: include_str!("../../assets/skills/multi-search-engine/CHANGELOG.md"),
        },
        BundledFile {
            path: "references/advanced-search.md",
            body: include_str!(
                "../../assets/skills/multi-search-engine/references/advanced-search.md"
            ),
        },
        BundledFile {
            path: "references/international-search.md",
            body: include_str!(
                "../../assets/skills/multi-search-engine/references/international-search.md"
            ),
        },
    ],
};

const ZAGENS_OFFICE: BundledSkill = BundledSkill {
    name: "zagens-office",
    files: &[BundledFile {
        path: "SKILL.md",
        body: include_str!("../../assets/skills/zagens-office/SKILL.md"),
    }],
};

const BUNDLED_SKILLS: &[BundledSkill] = &[
    SKILL_CREATOR,
    AUDIT_REPO,
    MULTI_SEARCH_ENGINE,
    ZAGENS_OFFICE,
];

/// Former office-* scenario skills removed in v12; delete leftover dirs on bump.
const REMOVED_BUNDLED_SKILLS: &[&str] = &[
    "office-weekly-report",
    "office-meeting-minutes",
    "office-project-report",
    "office-data-report",
    "office-competitive-analysis",
    "office-contract-draft",
    "office-resume",
    "office-release-notes",
    "office-executive-daily-brief",
    "office-customer-quote",
    "office-production-daily-report",
];

fn should_install_skill(
    skills_dir: &Path,
    skill_name: &str,
    installed_version: Option<&str>,
) -> bool {
    let target_dir = skills_dir.join(skill_name);
    match installed_version {
        None => !target_dir.exists(),
        Some(v) if v != BUNDLED_SKILL_VERSION => true, // bump: refresh existing + add new bundled skills
        Some(_) => false,                              // at current version: respect user deletion
    }
}

fn install_bundled_skill(skills_dir: &Path, skill: &BundledSkill) -> std::io::Result<()> {
    let target_dir = skills_dir.join(skill.name);
    fs::create_dir_all(&target_dir)?;
    for file in skill.files {
        let dest = target_dir.join(file.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dest, file.body)?;
    }
    Ok(())
}

fn skill_md_body(skill: &BundledSkill) -> Option<&'static str> {
    skill
        .files
        .iter()
        .find(|file| file.path == "SKILL.md")
        .map(|file| file.body)
}

fn remove_retired_bundled_skills(skills_dir: &Path) {
    for name in REMOVED_BUNDLED_SKILLS {
        let dir = skills_dir.join(name);
        if dir.is_dir() {
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

/// Install bundled system skills into `skills_dir`.
///
/// Behaviour:
/// - Fresh install (no marker, no dirs): installs all bundled skill files and writes
///   the version marker.
/// - Version bump (marker present with older version, dirs present): re-installs those dirs.
/// - User deleted a dir while marker still present at same version: leaves it gone.
/// - Idempotent: calling twice with no changes is a no-op.
///
/// Errors are I/O errors from the filesystem; the caller should log them but not
/// abort startup.
pub fn install_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");
    let installed_version = fs::read_to_string(&marker)
        .ok()
        .map(|s| s.trim().to_string());
    let version_ref = installed_version.as_deref();

    let version_bumped = matches!(
        version_ref,
        Some(v) if v != BUNDLED_SKILL_VERSION
    );
    if version_bumped {
        remove_retired_bundled_skills(skills_dir);
    }

    let any_install = BUNDLED_SKILLS
        .iter()
        .any(|skill| should_install_skill(skills_dir, skill.name, version_ref));

    if !any_install {
        return Ok(());
    }

    fs::create_dir_all(skills_dir)?;
    for skill in BUNDLED_SKILLS {
        if should_install_skill(skills_dir, skill.name, version_ref) {
            install_bundled_skill(skills_dir, skill)?;
        }
    }
    fs::write(&marker, BUNDLED_SKILL_VERSION)?;
    Ok(())
}

/// Remove bundled system skills and the version marker.
///
/// Intended for tests and `deepseek setup --clean`.  Ignores missing files.
pub fn uninstall_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    for skill in BUNDLED_SKILLS {
        let dir = skills_dir.join(skill.name);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
    }
    remove_retired_bundled_skills(skills_dir);
    let marker = skills_dir.join(".system-installed-version");
    if marker.exists() {
        let _ = fs::remove_file(marker);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fresh_install_writes_all_bundled_skills() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        for skill in BUNDLED_SKILLS {
            assert!(
                tmp.path().join(skill.name).join("SKILL.md").is_file(),
                "missing {}",
                skill.name
            );
        }
        let ver = fs::read_to_string(tmp.path().join(".system-installed-version")).unwrap();
        assert_eq!(ver.trim(), BUNDLED_SKILL_VERSION);
    }

    #[test]
    fn zagens_office_skill_mentions_cli() {
        let body = skill_md_body(&ZAGENS_OFFICE).expect("skill body");
        assert!(body.contains("zagens-office"));
        assert!(body.contains("exec_shell") || body.contains("schema write"));
        assert!(
            body.contains("纪律") || body.contains("Forbidden"),
            "skill must hard-ban Agent-tool fallbacks for Office work"
        );
        assert!(body.contains("填写表格") || body.contains("做报告"));
    }

    #[test]
    fn idempotent_at_same_version() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        let marker_mtime = fs::metadata(tmp.path().join(".system-installed-version"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        install_system_skills(tmp.path()).unwrap();
        let marker_mtime2 = fs::metadata(tmp.path().join(".system-installed-version"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(marker_mtime, marker_mtime2);
    }

    #[test]
    fn respects_user_deletion_at_same_version() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        fs::remove_dir_all(tmp.path().join("zagens-office")).unwrap();
        install_system_skills(tmp.path()).unwrap();
        assert!(!tmp.path().join("zagens-office").exists());
    }

    #[test]
    fn version_bump_installs_new_bundled_skill() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(tmp.path().join(".system-installed-version"), "1").unwrap();
        install_system_skills(tmp.path()).unwrap();
        assert!(tmp.path().join("zagens-office").join("SKILL.md").is_file());
    }

    #[test]
    fn version_bump_removes_retired_office_skills() {
        let tmp = TempDir::new().unwrap();
        let old = tmp.path().join("office-weekly-report");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("SKILL.md"), "old").unwrap();
        fs::write(tmp.path().join(".system-installed-version"), "11").unwrap();
        install_system_skills(tmp.path()).unwrap();
        assert!(!old.exists());
        assert!(tmp.path().join("zagens-office").join("SKILL.md").is_file());
    }

    #[test]
    fn uninstall_removes_marker_and_dirs() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        uninstall_system_skills(tmp.path()).unwrap();
        assert!(!tmp.path().join(".system-installed-version").exists());
        assert!(!tmp.path().join("zagens-office").exists());
    }
}
