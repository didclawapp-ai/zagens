//! System-skill installer: bundles skill-creator, audit-repo, multi-search-engine,
//! office task skills, and auto-installs on first launch.

use std::fs;
use std::path::Path;

const BUNDLED_SKILL_VERSION: &str = "8";

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

macro_rules! bundled_md_skill {
    ($name:expr, $path:literal) => {
        BundledSkill {
            name: $name,
            files: &[BundledFile {
                path: "SKILL.md",
                body: include_str!($path),
            }],
        }
    };
}

const OFFICE_WEEKLY_REPORT: BundledSkill = bundled_md_skill!(
    "office-weekly-report",
    "../../assets/skills/office-weekly-report/SKILL.md"
);
const OFFICE_MEETING_MINUTES: BundledSkill = bundled_md_skill!(
    "office-meeting-minutes",
    "../../assets/skills/office-meeting-minutes/SKILL.md"
);
const OFFICE_PROJECT_REPORT: BundledSkill = bundled_md_skill!(
    "office-project-report",
    "../../assets/skills/office-project-report/SKILL.md"
);
const OFFICE_DATA_REPORT: BundledSkill = bundled_md_skill!(
    "office-data-report",
    "../../assets/skills/office-data-report/SKILL.md"
);
const OFFICE_COMPETITIVE_ANALYSIS: BundledSkill = bundled_md_skill!(
    "office-competitive-analysis",
    "../../assets/skills/office-competitive-analysis/SKILL.md"
);
const OFFICE_CONTRACT_DRAFT: BundledSkill = bundled_md_skill!(
    "office-contract-draft",
    "../../assets/skills/office-contract-draft/SKILL.md"
);
const OFFICE_RESUME: BundledSkill = bundled_md_skill!(
    "office-resume",
    "../../assets/skills/office-resume/SKILL.md"
);
const OFFICE_RELEASE_NOTES: BundledSkill = bundled_md_skill!(
    "office-release-notes",
    "../../assets/skills/office-release-notes/SKILL.md"
);
const OFFICE_EXECUTIVE_DAILY_BRIEF: BundledSkill = bundled_md_skill!(
    "office-executive-daily-brief",
    "../../assets/skills/office-executive-daily-brief/SKILL.md"
);
const OFFICE_CUSTOMER_QUOTE: BundledSkill = bundled_md_skill!(
    "office-customer-quote",
    "../../assets/skills/office-customer-quote/SKILL.md"
);
const OFFICE_PRODUCTION_DAILY_REPORT: BundledSkill = bundled_md_skill!(
    "office-production-daily-report",
    "../../assets/skills/office-production-daily-report/SKILL.md"
);

const BUNDLED_SKILLS: &[BundledSkill] = &[
    SKILL_CREATOR,
    AUDIT_REPO,
    MULTI_SEARCH_ENGINE,
    OFFICE_WEEKLY_REPORT,
    OFFICE_MEETING_MINUTES,
    OFFICE_PROJECT_REPORT,
    OFFICE_DATA_REPORT,
    OFFICE_COMPETITIVE_ANALYSIS,
    OFFICE_CONTRACT_DRAFT,
    OFFICE_RESUME,
    OFFICE_RELEASE_NOTES,
    OFFICE_EXECUTIVE_DAILY_BRIEF,
    OFFICE_CUSTOMER_QUOTE,
    OFFICE_PRODUCTION_DAILY_REPORT,
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
#[allow(dead_code)]
pub fn uninstall_system_skills(skills_dir: &Path) -> std::io::Result<()> {
    let marker = skills_dir.join(".system-installed-version");
    for skill in BUNDLED_SKILLS {
        let target_dir = skills_dir.join(skill.name);
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }
    }
    if marker.exists() {
        fs::remove_file(&marker)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn skill_file(tmp: &TempDir, name: &str) -> std::path::PathBuf {
        tmp.path().join(name).join("SKILL.md")
    }

    fn marker_file(tmp: &TempDir) -> std::path::PathBuf {
        tmp.path().join(".system-installed-version")
    }

    #[test]
    fn fresh_install_creates_skills_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            let path = skill_file(&tmp, skill.name);
            assert!(path.exists(), "{}/SKILL.md should be created", skill.name);
            assert_eq!(
                fs::read_to_string(path).unwrap(),
                skill_md_body(skill).unwrap()
            );
        }
        assert!(marker_file(&tmp).exists());
        assert_eq!(
            fs::read_to_string(marker_file(&tmp)).unwrap().trim(),
            BUNDLED_SKILL_VERSION
        );
    }

    #[test]
    fn multi_search_engine_includes_reference_docs() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        let skill_dir = tmp.path().join("multi-search-engine");
        assert!(skill_dir.join("config.json").exists());
        assert!(
            skill_dir
                .join("references")
                .join("advanced-search.md")
                .exists()
        );
        assert!(
            skill_dir
                .join("references")
                .join("international-search.md")
                .exists()
        );
    }

    #[test]
    fn calling_twice_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            fs::write(skill_file(&tmp, skill.name), "sentinel").unwrap();
        }

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert_eq!(
                fs::read_to_string(skill_file(&tmp, skill.name)).unwrap(),
                "sentinel",
                "second install should not overwrite when version is current"
            );
        }
    }

    #[test]
    fn user_deleted_dir_is_not_recreated() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        fs::remove_dir_all(tmp.path().join("skill-creator")).unwrap();

        install_system_skills(tmp.path()).unwrap();

        assert!(!skill_file(&tmp, "skill-creator").exists());
        assert!(skill_file(&tmp, "audit-repo").exists());
    }

    #[test]
    fn version_bump_installs_new_bundled_skill() {
        let tmp = TempDir::new().unwrap();
        let skill_dir = tmp.path().join("skill-creator");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "old").unwrap();
        fs::write(marker_file(&tmp), "1").unwrap();

        install_system_skills(tmp.path()).unwrap();

        assert!(skill_file(&tmp, "audit-repo").exists());
        assert!(skill_file(&tmp, "multi-search-engine").exists());
    }

    #[test]
    fn outdated_marker_triggers_reinstall() {
        let tmp = TempDir::new().unwrap();
        for skill in BUNDLED_SKILLS {
            let skill_dir = tmp.path().join(skill.name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), "old content").unwrap();
        }
        fs::write(marker_file(&tmp), "0").unwrap();

        install_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert_eq!(
                fs::read_to_string(skill_file(&tmp, skill.name)).unwrap(),
                skill_md_body(skill).unwrap()
            );
        }
    }

    #[test]
    fn uninstall_removes_skills_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        uninstall_system_skills(tmp.path()).unwrap();

        for skill in BUNDLED_SKILLS {
            assert!(!skill_file(&tmp, skill.name).exists());
        }
        assert!(!marker_file(&tmp).exists());
    }

    #[test]
    fn uninstall_on_clean_dir_is_a_noop() {
        let tmp = TempDir::new().unwrap();
        uninstall_system_skills(tmp.path()).unwrap();
    }
}
