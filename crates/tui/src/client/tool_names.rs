use regex::Regex;
use std::sync::OnceLock;

pub(super) fn to_api_tool_name(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else if ch == '-' {
            out.push_str("--");
        } else {
            out.push_str("-x");
            out.push_str(&format!("{:06X}", ch as u32));
            out.push('-');
        }
    }
    out
}

pub(super) fn from_api_tool_name(name: &str) -> String {
    let mut out = String::new();
    let mut iter = name.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch != '-' {
            out.push(ch);
            continue;
        }
        if let Some('-') = iter.peek().copied() {
            iter.next();
            out.push('-');
            continue;
        }
        if iter.peek().copied() == Some('x') {
            iter.next();
            let mut hex = String::new();
            for _ in 0..6 {
                if let Some(h) = iter.next() {
                    hex.push(h);
                } else {
                    break;
                }
            }
            if let Ok(code) = u32::from_str_radix(&hex, 16)
                && let Some(decoded) = std::char::from_u32(code)
            {
                if let Some('-') = iter.peek().copied() {
                    iter.next();
                }
                out.push(decoded);
                continue;
            }
            out.push('-');
            out.push('x');
            out.push_str(&hex);
            continue;
        }
        out.push('-');
    }

    // Second pass: decode bare hex escapes (e.g. `x00002E`) that the model
    // may produce when it mangles the `-x00002E-` delimiter form.  Only
    // decode when the resulting character is one that `to_api_tool_name`
    // would have encoded (not alphanumeric, not `_`, not `-`).
    decode_bare_hex_escapes(&out)
}

/// Decode bare `x[0-9A-Fa-f]{6}` sequences (optionally followed by `-`)
/// that survive the standard delimiter-based pass.  This handles cases
/// where the model strips or replaces the leading `-` of `-x00002E-`.
pub(super) fn decode_bare_hex_escapes(input: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;

    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"x([0-9A-Fa-f]{6})-?").unwrap());

    let result = re.replace_all(input, |caps: &regex::Captures| {
        let hex = &caps[1];
        if let Ok(code) = u32::from_str_radix(hex, 16)
            && let Some(decoded) = std::char::from_u32(code)
        {
            // Only decode characters that to_api_tool_name would have encoded
            if !decoded.is_ascii_alphanumeric() && decoded != '_' && decoded != '-' {
                return decoded.to_string();
            }
        }
        // Not a character we'd encode — leave as-is
        caps[0].to_string()
    });
    result.into_owned()
}
