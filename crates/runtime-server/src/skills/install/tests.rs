use std::path::Path;

use super::*;
use super::download::{is_safe_path, parse_frontmatter_name, strip_prefix};
use super::local::source_spec_string;

#[test]
fn parse_github_source() {
    let s = InstallSource::parse("github:Hmbown/test-skill").unwrap();
    assert_eq!(
        s,
        InstallSource::GitHubRepo("Hmbown/test-skill".to_string())
    );
}

#[test]
fn parse_github_source_rejects_missing_repo() {
    let err = InstallSource::parse("github:Hmbown").unwrap_err();
    assert!(err.to_string().contains("github source must"), "got: {err}");
}

#[test]
fn parse_github_source_rejects_extra_slashes() {
    let err = InstallSource::parse("github:Hmbown/repo/extra").unwrap_err();
    assert!(err.to_string().contains("github source must"), "got: {err}");
}

#[test]
fn parse_direct_url_source() {
    let s = InstallSource::parse("https://example.com/skill.tar.gz").unwrap();
    assert_eq!(
        s,
        InstallSource::DirectUrl("https://example.com/skill.tar.gz".to_string())
    );
    let s = InstallSource::parse("http://example.com/skill.tar.gz").unwrap();
    assert_eq!(
        s,
        InstallSource::DirectUrl("http://example.com/skill.tar.gz".to_string())
    );
}

#[test]
fn parse_github_browser_url_routes_to_github_repo() {
    // Regression for #269: `https://github.com/<owner>/<repo>` was being
    // parsed as a DirectUrl, so the installer downloaded the HTML repo
    // page and tried to gzip-decode HTML ("invalid gzip header").
    for spec in [
        "https://github.com/obra/superpowers",
        "https://github.com/obra/superpowers/",
        "https://github.com/obra/superpowers.git",
        "https://github.com/obra/superpowers.git/",
        "https://www.github.com/obra/superpowers",
        "http://github.com/obra/superpowers",
        "  https://github.com/obra/superpowers  ",
    ] {
        let parsed = InstallSource::parse(spec)
            .unwrap_or_else(|err| panic!("parse({spec}) failed: {err}"));
        assert_eq!(
            parsed,
            InstallSource::GitHubRepo("obra/superpowers".to_string()),
            "spec {spec} must route to GitHubRepo",
        );
    }
}

#[test]
fn parse_github_archive_url_stays_direct() {
    // URLs that point at a specific subresource (archive tarball, blob,
    // tree) are real direct URLs — the user picked that exact path.
    for spec in [
        "https://github.com/obra/superpowers/archive/refs/heads/main.tar.gz",
        "https://github.com/obra/superpowers/blob/main/README.md",
        "https://github.com/obra/superpowers/tree/main",
    ] {
        let parsed = InstallSource::parse(spec).unwrap();
        assert!(
            matches!(parsed, InstallSource::DirectUrl(_)),
            "spec {spec} must stay DirectUrl, got {parsed:?}",
        );
    }
}

#[test]
fn parse_registry_source() {
    let s = InstallSource::parse("my-skill").unwrap();
    assert_eq!(s, InstallSource::Registry("my-skill".to_string()));
}

#[test]
fn parse_rejects_empty() {
    assert!(InstallSource::parse("").is_err());
    assert!(InstallSource::parse("   ").is_err());
}

#[test]
fn is_safe_path_rejects_traversal() {
    assert!(!is_safe_path(Path::new("../etc/passwd")));
    assert!(!is_safe_path(Path::new("foo/../bar")));
    assert!(!is_safe_path(Path::new("/etc/passwd")));
    assert!(is_safe_path(Path::new("foo/bar/baz")));
    assert!(is_safe_path(Path::new("SKILL.md")));
}

#[test]
fn parse_frontmatter_extracts_name() {
    let body = b"---\nname: hello\ndescription: greeter\n---\nbody\n";
    assert_eq!(parse_frontmatter_name(body).unwrap(), "hello");
}

#[test]
fn parse_frontmatter_missing_name_fails() {
    let body = b"---\ndescription: x\n---\n";
    let err = parse_frontmatter_name(body).unwrap_err();
    assert!(format!("{err}").contains("name"));
}

#[test]
fn parse_frontmatter_missing_description_fails() {
    let body = b"---\nname: x\n---\n";
    let err = parse_frontmatter_name(body).unwrap_err();
    assert!(format!("{err}").contains("description"));
}

#[test]
fn parse_frontmatter_rejects_unsafe_name() {
    let body = b"---\nname: ../evil\ndescription: x\n---\n";
    assert!(parse_frontmatter_name(body).is_err());

    let body = b"---\nname: a name with spaces\ndescription: x\n---\n";
    assert!(parse_frontmatter_name(body).is_err());
}

#[test]
fn parse_frontmatter_requires_opening_fence() {
    let body = b"name: hello\ndescription: x\n";
    assert!(parse_frontmatter_name(body).is_err());
}

#[test]
fn strip_prefix_handles_all_cases() {
    assert_eq!(strip_prefix("foo/bar", "foo"), "bar");
    assert_eq!(strip_prefix("foo", "foo"), "");
    assert_eq!(strip_prefix("baz/bar", "foo"), "baz/bar");
    assert_eq!(strip_prefix("foo/bar", ""), "foo/bar");
}

#[test]
fn source_spec_string_roundtrips() {
    assert_eq!(
        source_spec_string(&InstallSource::GitHubRepo("a/b".into())),
        "github:a/b"
    );
    assert_eq!(
        source_spec_string(&InstallSource::DirectUrl("https://x".into())),
        "https://x"
    );
    assert_eq!(
        source_spec_string(&InstallSource::Registry("x".into())),
        "x"
    );
}
