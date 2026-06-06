//! Shared outbound network policy + SSRF helpers for model-visible tools (D16 E1-a6).

use std::net::IpAddr;

use crate::network_policy::{Decision, NetworkPolicy, NetworkPolicyDecider, host_from_url};

/// Policy gate failure for an outbound network call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkGateError {
    Denied { host: String, tool: String },
    PromptRequired { host: String, tool: String },
}

impl NetworkGateError {
    #[must_use]
    pub fn denial_message(&self) -> String {
        match self {
            Self::Denied { host, .. } => {
                format!("network call to '{host}' blocked by network policy")
            }
            Self::PromptRequired { host, .. } => format!(
                "network call to '{host}' requires approval; \
                 re-run after `/network allow {host}` or set network.default = \"allow\" in config"
            ),
        }
    }
}

/// Evaluate per-domain policy for a known host. No-op when `decider` is `None`.
pub fn check_host_policy(
    decider: Option<&NetworkPolicyDecider>,
    tool_name: &str,
    host: &str,
) -> Result<(), NetworkGateError> {
    let Some(decider) = decider else {
        return Ok(());
    };
    match decider.evaluate(host, tool_name) {
        Decision::Allow => Ok(()),
        Decision::Deny => Err(NetworkGateError::Denied {
            host: host.to_string(),
            tool: tool_name.to_string(),
        }),
        Decision::Prompt => Err(NetworkGateError::PromptRequired {
            host: host.to_string(),
            tool: tool_name.to_string(),
        }),
    }
}

/// Extract host from `url` and evaluate policy. Returns the host when present.
pub fn check_url_policy(
    decider: Option<&NetworkPolicyDecider>,
    tool_name: &str,
    url: &str,
) -> Result<Option<String>, NetworkGateError> {
    let Some(host) = host_from_url(url) else {
        return Ok(None);
    };
    check_host_policy(decider, tool_name, &host)?;
    Ok(Some(host))
}

/// Evaluate a static [`NetworkPolicy`] (no session cache) for a known host.
pub fn check_host_with_policy(
    policy: &NetworkPolicy,
    tool_name: &str,
    host: &str,
) -> Result<(), NetworkGateError> {
    match policy.decide(host) {
        Decision::Allow => Ok(()),
        Decision::Deny => Err(NetworkGateError::Denied {
            host: host.to_string(),
            tool: tool_name.to_string(),
        }),
        Decision::Prompt => Err(NetworkGateError::PromptRequired {
            host: host.to_string(),
            tool: tool_name.to_string(),
        }),
    }
}

/// Decision for `host` against a static policy (no session cache).
#[must_use]
pub fn host_policy_decision(policy: &NetworkPolicy, host: &str) -> Decision {
    policy.decide(host)
}

/// True when `url` uses http/https (case-insensitive on scheme).
#[must_use]
pub fn is_http_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("http://") || trimmed.starts_with("https://")
}

/// Check if an IP address is loopback, private, link-local, cloud-metadata,
/// multicast, or reserved — SSRF prevention for LLM-initiated fetches.
#[must_use]
pub fn is_restricted_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || matches!(v4.octets(), [100, 64..=127, ..])
                || *ip == IpAddr::V4(std::net::Ipv4Addr::new(169, 254, 169, 254))
                || matches!(v4.octets(), [198, 18..=19, ..])
                || v4.octets()[0] >= 240
        }
        IpAddr::V6(v6) => {
            if v6.is_unspecified()
                || matches!(v6.octets(), [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, ..])
            {
                return true;
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_restricted_ip(&IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_multicast()
                || matches!(v6.segments(), [0xfc00..=0xfdff, ..])
                || matches!(v6.segments(), [0xfe80..=0xfebf, ..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network_policy::{Decision, NetworkPolicy, NetworkPolicyDecider};

    #[test]
    fn restricted_ip_detects_loopback() {
        assert!(is_restricted_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_restricted_ip(&"::1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_private_ranges() {
        assert!(is_restricted_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_restricted_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_restricted_ip(&"192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_metadata_and_cgnat() {
        assert!(is_restricted_ip(&"169.254.169.254".parse().unwrap()));
        assert!(is_restricted_ip(&"100.64.0.1".parse().unwrap()));
        assert!(!is_restricted_ip(&"100.63.0.1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_link_local() {
        assert!(is_restricted_ip(&"169.254.1.1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_ipv6_ula() {
        assert!(is_restricted_ip(&"fc00::1".parse().unwrap()));
        assert!(is_restricted_ip(&"fd12:3456::1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_unspecified() {
        assert!(is_restricted_ip(&"::".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_allows_public() {
        assert!(!is_restricted_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!is_restricted_ip(&"93.184.216.34".parse().unwrap()));
        assert!(!is_restricted_ip(&"2606:4700::1".parse().unwrap()));
    }

    #[test]
    fn restricted_ip_detects_ipv4_mapped_private() {
        assert!(is_restricted_ip(&"::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_restricted_ip(&"::ffff:169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn check_url_policy_denies_blocked_host() {
        let policy = NetworkPolicy {
            default: Decision::Allow.into(),
            allow: vec![],
            deny: vec!["example.com".into()],
            audit: false,
        };
        let decider = NetworkPolicyDecider::with_default_audit(policy);
        let err = check_url_policy(Some(&decider), "fetch_url", "https://example.com/private")
            .expect_err("deny");
        assert!(matches!(err, NetworkGateError::Denied { .. }));
    }

    #[test]
    fn check_host_with_policy_denies_blocked_host() {
        let policy = NetworkPolicy {
            default: Decision::Allow.into(),
            allow: vec![],
            deny: vec!["example.com".into()],
            audit: false,
        };
        let err =
            check_host_with_policy(&policy, "skills_install", "example.com").expect_err("deny");
        assert!(matches!(err, NetworkGateError::Denied { .. }));
    }

    #[test]
    fn check_host_policy_allows_when_decider_missing() {
        check_host_policy(None, "fetch_url", "example.com").expect("permissive default");
    }
}
