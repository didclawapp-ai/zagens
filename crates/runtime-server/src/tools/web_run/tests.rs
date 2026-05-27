use super::html::{
    parse_bing_results, parse_html, percent_decode, wrap_line,
};
use super::search::extract_duckduckgo_vqd;
use super::state::{
    get_page, next_turn_for_namespace, reset_web_run_state, scoped_ref_prefix, store_page,
};
use super::types::WebPage;
use std::path::PathBuf;

fn sample_page(url: &str) -> WebPage {
    WebPage {
        url: url.to_string(),
        title: Some("Example".to_string()),
        content_type: Some("text/html".to_string()),
        lines: vec!["example line".to_string()],
        links: Vec::new(),
        pdf_pages: None,
    }
}

#[test]
fn html_link_parsing_extracts_links() {
    let html = r#"
            <html><body>
            <p>Hello <a href="https://example.com">Example</a> world.</p>
            </body></html>
        "#;
    let (lines, links, title) = parse_html(html, "https://example.com");
    assert!(title.is_none());
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].url, "https://example.com");
    assert!(lines.iter().any(|line| line.contains("Example")));
}

#[test]
fn wrap_line_splits_long_lines() {
    let line = "This is a long line that should wrap cleanly at word boundaries";
    let wrapped = wrap_line(line, 20);
    assert!(wrapped.len() > 1);
    assert!(wrapped.iter().all(|l| l.len() <= 20));
}

#[test]
fn extracts_duckduckgo_vqd_token() {
    let html_single = "<script>var x = {vqd='3-1234567890'};</script>";
    assert_eq!(
        extract_duckduckgo_vqd(html_single),
        Some("3-1234567890".to_string())
    );

    let html_double = "<script>var x = {vqd=\"3-abcdef\"};</script>";
    assert_eq!(
        extract_duckduckgo_vqd(html_double),
        Some("3-abcdef".to_string())
    );

    let html_plain = "https://duckduckgo.com/?q=test&vqd=3-xyz_123&ia=images";
    assert_eq!(
        extract_duckduckgo_vqd(html_plain),
        Some("3-xyz_123".to_string())
    );
}

#[test]
fn parses_bing_results_and_decodes_redirect_url() {
    let html = r#"
            <ol>
              <li class="b_algo">
                <h2><a href="https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9wYXRoP3E9MQ">Example &amp; Result</a></h2>
                <div class="b_caption"><p>A <strong>useful</strong> snippet.</p></div>
              </li>
            </ol>
        "#;

    let results = parse_bing_results(html, 5);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].title, "Example & Result");
    assert_eq!(results[0].url, "https://example.com/path?q=1");
    assert_eq!(results[0].snippet.as_deref(), Some("A useful snippet."));
}

#[test]
fn percent_decode_handles_utf8_multibyte_sequences() {
    assert_eq!(percent_decode("Hello %E4%B8%AA%E4%BA%BA"), "Hello 个人");
    assert_eq!(percent_decode("%E7%B4%A0%E6%9D%90"), "素材");
    assert_eq!(
        percent_decode("https://example.com/%E9%A1%B5%E9%9D%A2"),
        "https://example.com/页面"
    );
    assert_eq!(percent_decode("查询 keyword"), "查询 keyword");
    assert_eq!(percent_decode("foo+bar%20baz"), "foo+bar baz");
}

#[test]
fn scoped_ref_prefix_is_session_specific() {
    reset_web_run_state();
    let alpha = scoped_ref_prefix("session-alpha");
    let beta = scoped_ref_prefix("session-beta");

    assert_ne!(alpha, beta);
    assert!(alpha.starts_with('s'));
    assert!(alpha.ends_with('_'));
    assert_eq!(alpha.len(), 18);
}

#[test]
fn stored_pages_do_not_cross_scoped_sessions() {
    reset_web_run_state();
    let shared_suffix = "turn1search1";
    let ref_alpha = format!("{}{}", scoped_ref_prefix("session-alpha"), shared_suffix);
    let ref_beta = format!("{}{}", scoped_ref_prefix("session-beta"), shared_suffix);

    store_page(
        "session-alpha",
        &ref_alpha,
        sample_page("https://example.com/alpha"),
    );

    assert!(get_page(&ref_alpha).is_some());
    assert!(get_page(&ref_beta).is_none());
}

#[test]
fn turn_counters_are_scoped_per_session() {
    reset_web_run_state();

    assert_eq!(next_turn_for_namespace("session-alpha"), 0);
    assert_eq!(next_turn_for_namespace("session-alpha"), 1);
    assert_eq!(next_turn_for_namespace("session-beta"), 0);
}

#[test]
fn stale_session_pages_are_evicted() {
    use super::state::with_state;
    use super::WEB_RUN_SESSION_TTL;
    use std::time::{Duration, Instant};

    reset_web_run_state();
    let namespace = "session-alpha";
    let ref_id = format!("{}turn0search1", scoped_ref_prefix(namespace));
    store_page(namespace, &ref_id, sample_page("https://example.com/alpha"));

    let stale = WEB_RUN_SESSION_TTL + Duration::from_secs(1);
    let can_test = with_state(|state| {
        let session = state
            .sessions
            .get_mut(namespace)
            .expect("session should exist");
        match Instant::now().checked_sub(stale) {
            Some(past) => {
                session.last_access = past;
                true
            }
            None => false,
        }
    });
    if !can_test {
        return;
    }

    let _ = next_turn_for_namespace("session-beta");

    assert!(get_page(&ref_id).is_none());
}

#[test]
fn direct_urls_remain_compatible_open_refs() {
    use super::page::looks_like_url;

    assert!(looks_like_url("https://example.com"));
    assert!(looks_like_url("http://example.com"));
    assert!(!looks_like_url("turn0search0"));
}

#[test]
fn network_policy_denies_direct_open_url() {
    use super::page::check_network_policy;
    use crate::network_policy::{Decision, NetworkPolicy, NetworkPolicyDecider};
    use crate::tools::spec::ToolContext;

    let policy = NetworkPolicy {
        default: Decision::Deny.into(),
        allow: vec!["api.deepseek.com".to_string()],
        deny: vec![],
        audit: false,
    };
    let decider = NetworkPolicyDecider::new(policy, None);
    let ctx = ToolContext::new(PathBuf::from(".")).with_network_policy(decider);

    let err = check_network_policy("https://example.com/private", &ctx)
        .expect_err("blocked host should fail");
    assert!(format!("{err}").contains("blocked by network policy"));
}
