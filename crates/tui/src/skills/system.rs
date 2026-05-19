//! System-skill installer: bundles skill-creator, audit-repo, and auto-installs on first launch.

use std::fs;
use std::path::Path;

const BUNDLED_SKILL_VERSION: &str = "3";
const SKILL_CREATOR_BODY: &str = include_str!("../../assets/skills/skill-creator/SKILL.md");
const AUDIT_REPO_BODY: &str = include_str!("../../assets/skills/audit-repo/SKILL.md");

const BUNDLED_SKILLS: &[(&str, &str)] = &[
    ("skill-creator", SKILL_CREATOR_BODY),
    ("audit-repo", AUDIT_REPO_BODY),
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

/// Install bundled system skills into `skills_dir`.
///
/// Behaviour:
/// - Fresh install (no marker, no dirs): installs all bundled `SKILL.md` files and writes
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
        .any(|(name, _)| should_install_skill(skills_dir, name, version_ref));

    if !any_install {
        return Ok(());
    }

    fs::create_dir_all(skills_dir)?;
    for (name, body) in BUNDLED_SKILLS {
        if should_install_skill(skills_dir, name, version_ref) {
            let target_dir = skills_dir.join(name);
            fs::create_dir_all(&target_dir)?;
            fs::write(target_dir.join("SKILL.md"), body)?;
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
    for (name, _) in BUNDLED_SKILLS {
        let target_dir = skills_dir.join(name);
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

        for (name, body) in BUNDLED_SKILLS {
            let path = skill_file(&tmp, name);
            assert!(path.exists(), "{name}/SKILL.md should be created");
            assert_eq!(fs::read_to_string(path).unwrap(), *body);
        }
        assert!(marker_file(&tmp).exists());
        assert_eq!(
            fs::read_to_string(marker_file(&tmp)).unwrap().trim(),
            BUNDLED_SKILL_VERSION
        );
    }

    #[test]
    fn calling_twice_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();

        for (name, _) in BUNDLED_SKILLS {
            fs::write(skill_file(&tmp, name), "sentinel").unwrap();
        }

        install_system_skills(tmp.path()).unwrap();

        for (name, _) in BUNDLED_SKILLS {
            assert_eq!(
                fs::read_to_string(skill_file(&tmp, name)).unwrap(),
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
    }

    #[test]
    fn outdated_marker_triggers_reinstall() {
        let tmp = TempDir::new().unwrap();
        for (name, _) in BUNDLED_SKILLS {
            let skill_dir = tmp.path().join(name);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), "old content").unwrap();
        }
        fs::write(marker_file(&tmp), "0").unwrap();

        install_system_skills(tmp.path()).unwrap();

        for (name, body) in BUNDLED_SKILLS {
            assert_eq!(fs::read_to_string(skill_file(&tmp, name)).unwrap(), *body);
        }
    }

    #[test]
    fn uninstall_removes_skills_and_marker() {
        let tmp = TempDir::new().unwrap();
        install_system_skills(tmp.path()).unwrap();
        uninstall_system_skills(tmp.path()).unwrap();

        for (name, _) in BUNDLED_SKILLS {
            assert!(!skill_file(&tmp, name).exists());
        }
        assert!(!marker_file(&tmp).exists());
    }

    #[test]
    fn uninstall_on_clean_dir_is_a_noop() {
        let tmp = TempDir::new().unwrap();
        uninstall_system_skills(tmp.path()).unwrap();
    }
}
