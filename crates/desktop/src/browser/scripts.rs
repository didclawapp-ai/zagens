//! Injected page scripts for snapshot / console / interact / wait.

use serde::Deserialize;

use super::{BrowserA11yNode, BrowserSnapshotDto};

/// Shared helpers: stable refs (`role:slug:nth`), findByRef with DOM re-resolve.
const REF_HELPERS: &str = r#"
  var ZAGENS_SEL = 'a,button,input,textarea,select,option,[contenteditable],[role="button"],[role="link"],[role="textbox"],[role="checkbox"],[role="radio"],[role="menuitem"],[role="tab"],[role="option"],[role="switch"],[role="combobox"],h1,h2,h3';
  function zagensSlug(s) {
    var t = String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
    return (t || 'anon').slice(0, 40);
  }
  function zagensRole(el) {
    var r = (el.getAttribute('role') || el.tagName.toLowerCase() || 'unknown').toLowerCase();
    return r.replace(/[^a-z0-9-]+/g, '-') || 'unknown';
  }
  function zagensName(el) {
    return (el.getAttribute('aria-label') || el.getAttribute('name') || el.getAttribute('placeholder') ||
      el.getAttribute('title') || el.innerText || el.value || el.getAttribute('alt') || '').trim().slice(0, 120);
  }
  function zagensNameCandidates(el) {
    var out = [];
    function add(v) { var s = String(v || '').trim(); if (s) out.push(s.slice(0, 120)); }
    add(el.getAttribute && el.getAttribute('aria-label'));
    add(el.getAttribute && el.getAttribute('name'));
    add(el.getAttribute && el.getAttribute('placeholder'));
    add(el.getAttribute && el.getAttribute('title'));
    add(el.innerText);
    add(el.value);
    add(el.getAttribute && el.getAttribute('alt'));
    return out;
  }
  function zagensRoleMatches(el, wantRole) {
    if (zagensRole(el) === wantRole) return true;
    var tag = (el.tagName || '').toLowerCase();
    var type = ((el.getAttribute && el.getAttribute('type')) || '').toLowerCase();
    switch (wantRole) {
      case 'link': return tag === 'a';
      case 'button': return tag === 'button' || (tag === 'input' && (type === 'button' || type === 'submit' || type === 'reset' || type === 'image'));
      case 'textbox': case 'textfield': return tag === 'textarea' ||
        (tag === 'input' && ['', 'text', 'email', 'url', 'tel', 'password', 'number'].indexOf(type) >= 0);
      case 'searchbox': return tag === 'input' && type === 'search';
      case 'checkbox': return tag === 'input' && type === 'checkbox';
      case 'radio': return tag === 'input' && type === 'radio';
      case 'combobox': case 'listbox': return tag === 'select';
      case 'slider': return tag === 'input' && type === 'range';
      case 'heading': return tag === 'h1' || tag === 'h2' || tag === 'h3';
      case 'option': return tag === 'option';
      default: return false;
    }
  }
  function zagensEscAttr(s) {
    return String(s).replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }
  function zagensDocForFrame(idx) {
    if (idx === 0) return document;
    var frames = document.querySelectorAll('iframe');
    var fr = frames[idx - 1];
    if (!fr) return null;
    try { return fr.contentDocument || fr.contentWindow.document; } catch (e) { return null; }
  }
  function zagensParseRef(ref) {
    var s = String(ref || '');
    var fm = /^f(\d+):(.+)$/.exec(s);
    if (fm) return { doc: zagensDocForFrame(parseInt(fm[1], 10)), inner: fm[2] };
    return { doc: document, inner: s };
  }
  function zagensFindInDoc(doc, innerRef) {
    if (!doc) return null;
    try {
      var el = doc.querySelector('[data-zagens-ref="' + zagensEscAttr(innerRef) + '"]');
      if (el) return el;
    } catch (e) {}
    var m = /^([a-z0-9-]+):([a-z0-9-]+):(\d+)$/.exec(String(innerRef || ''));
    if (!m) return null;
    var wantRole = m[1], wantSlug = m[2], wantNth = parseInt(m[3], 10);
    var els = doc.querySelectorAll(ZAGENS_SEL);
    var n = 0;
    for (var i = 0; i < els.length; i++) {
      var cur = els[i];
      // AX-role aliases + visibility keep CDP snapshot refs (link/textbox/heading/...)
      // resolvable here, with nth counting aligned to the CDP visible-node filter.
      if (!zagensRoleMatches(cur, wantRole)) continue;
      if (!zagensIsVisible(cur)) continue;
      var names = zagensNameCandidates(cur);
      var hit = names.length === 0 && wantSlug === 'anon';
      for (var j = 0; j < names.length && !hit; j++) {
        if (zagensSlug(names[j]) === wantSlug) hit = true;
      }
      if (!hit) continue;
      if (n === wantNth) return cur;
      n++;
    }
    return null;
  }
  function zagensFindByRef(ref) {
    var p = zagensParseRef(ref);
    return zagensFindInDoc(p.doc, p.inner);
  }
  function zagensPagePoint(el, doc) {
    var rect = el.getBoundingClientRect();
    var x = rect.left + rect.width / 2;
    var y = rect.top + rect.height / 2;
    if (doc && doc !== document) {
      var frames = document.querySelectorAll('iframe');
      for (var i = 0; i < frames.length; i++) {
        try {
          if (frames[i].contentDocument === doc) {
            var fr = frames[i].getBoundingClientRect();
            x += fr.left;
            y += fr.top;
            break;
          }
        } catch (e) {}
      }
    }
    return { x: x, y: y };
  }
  function zagensIsVisible(el) {
    if (!el || typeof el.getBoundingClientRect !== 'function') return false;
    try {
      // Use the element's own window: getComputedStyle from the top window is
      // unreliable for elements inside iframe documents.
      var win = (el.ownerDocument && el.ownerDocument.defaultView) || window;
      var st = win.getComputedStyle(el);
      if (st.display === 'none' || st.visibility === 'hidden' || Number(st.opacity) === 0) return false;
      var r = el.getBoundingClientRect();
      if (r.width <= 0 && r.height <= 0) return false;
    } catch (e) { return false; }
    return true;
  }
  function zagensSetNativeValue(el, text) {
    try {
      var proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype
        : el instanceof HTMLInputElement ? HTMLInputElement.prototype : null;
      var desc = proto && Object.getOwnPropertyDescriptor(proto, 'value');
      if (desc && desc.set) { desc.set.call(el, text); return; }
    } catch (e) {}
    el.value = text;
  }
"#;

/// Snapshot: visible text + a11y nodes with stable `role:slug:nth` refs (B1/B3).
pub fn snapshot_js() -> String {
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var text = document.body ? (document.body.innerText || '').slice(0, 50000) : '';
    var title = document.title || '';
    var url = location.href || '';
    var iframeCount = 0;
    try {{ iframeCount = document.querySelectorAll('iframe').length; }} catch (e) {{}}
    if (iframeCount > 0) {{
      text = text + '\n\n[zagens] page has ' + iframeCount + ' iframe(s); content inside iframes is not snapshotted.';
    }}
    var nodes = [];
    var counts = {{}};
    var els = document.querySelectorAll(ZAGENS_SEL);
    for (var i = 0; i < els.length && nodes.length < 120; i++) {{
      var el = els[i];
      if (!zagensIsVisible(el)) continue;
      var role = zagensRole(el);
      var name = zagensName(el);
      var slug = zagensSlug(name);
      var key = role + ':' + slug;
      var nth = counts[key] || 0;
      counts[key] = nth + 1;
      var r = role + ':' + slug + ':' + nth;
      try {{ el.setAttribute('data-zagens-ref', r); }} catch (e) {{}}
      nodes.push({{ ref: r, role: role, name: name }});
    }}
    return JSON.stringify({{ url: url, title: title, text: text, nodes: nodes, iframeCount: iframeCount }});
  }} catch (err) {{
    return JSON.stringify({{ url: location.href || '', title: '', text: String(err), nodes: [], iframeCount: 0 }});
  }}
}})()"#,
        helpers = REF_HELPERS
    )
}

/// Early init script for WebView builders (B4) — not wrapped as eval return value.
pub const CONSOLE_HOOK_INIT: &str = r#"(function(){
  try {
    if (window.__zagensConsoleHooked) {
      if (!window.__zagensConsole) window.__zagensConsole = [];
      return;
    }
    window.__zagensConsoleHooked = true;
    window.__zagensConsole = [];
    function push(level, args) {
      try {
        var msg = Array.prototype.slice.call(args).map(function(a){
          try { return typeof a === 'string' ? a : JSON.stringify(a); } catch (e) { return String(a); }
        }).join(' ');
        window.__zagensConsole.push({ level: level, message: String(msg).slice(0, 2000), ts: Date.now() });
        if (window.__zagensConsole.length > 200) window.__zagensConsole.shift();
      } catch (e) {}
    }
    ['log','info','warn','error','debug'].forEach(function(level){
      var orig = console[level];
      console[level] = function(){
        push(level, arguments);
        if (typeof orig === 'function') return orig.apply(console, arguments);
      };
    });
    window.addEventListener('error', function(ev){
      push('error', [ev.message || 'error']);
    });
  } catch (e) {}
})();"#;

/// Idempotent console hook for eval after load (same logic as init).
pub const CONSOLE_HOOK_JS: &str = r#"(function(){
  try {
    if (window.__zagensConsoleHooked) {
      if (!window.__zagensConsole) window.__zagensConsole = [];
      return true;
    }
    window.__zagensConsoleHooked = true;
    window.__zagensConsole = [];
    function push(level, args) {
      try {
        var msg = Array.prototype.slice.call(args).map(function(a){
          try { return typeof a === 'string' ? a : JSON.stringify(a); } catch (e) { return String(a); }
        }).join(' ');
        window.__zagensConsole.push({ level: level, message: String(msg).slice(0, 2000), ts: Date.now() });
        if (window.__zagensConsole.length > 200) window.__zagensConsole.shift();
      } catch (e) {}
    }
    ['log','info','warn','error','debug'].forEach(function(level){
      var orig = console[level];
      console[level] = function(){
        push(level, arguments);
        if (typeof orig === 'function') return orig.apply(console, arguments);
      };
    });
    window.addEventListener('error', function(ev){
      push('error', [ev.message || 'error']);
    });
    return true;
  } catch (e) { return false; }
})()"#;

/// Clear ring buffer on navigation start (hook stays installed across same-document reuse).
pub const CONSOLE_CLEAR_JS: &str = r#"(function(){
  try {
    window.__zagensConsole = [];
    return true;
  } catch (e) { return false; }
})()"#;

pub const CONSOLE_TAIL_JS: &str = r#"(function(){
  try {
    return JSON.stringify(window.__zagensConsole || []);
  } catch (e) {
    return '[]';
  }
})()"#;

pub const HISTORY_BACK_JS: &str = "history.back(); true";
pub const HISTORY_FORWARD_JS: &str = "history.forward(); true";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSnap {
    url: String,
    title: String,
    text: String,
    nodes: Vec<BrowserA11yNode>,
    #[serde(default)]
    iframe_count: Option<u32>,
}

/// WebView2 `eval_with_callback` often JSON-encodes string return values once more.
/// Unwrap a leading JSON string layer so `JSON.stringify(...)` results parse cleanly.
pub fn normalize_eval_json(raw: &str) -> String {
    let trimmed = raw.trim();
    match serde_json::from_str::<String>(trimmed) {
        Ok(inner) => inner,
        Err(_) => trimmed.to_string(),
    }
}

pub fn parse_snapshot_json(raw: &str) -> BrowserSnapshotDto {
    let normalized = normalize_eval_json(raw);
    // Tolerate a second accidental string wrap from older hosts / bridges.
    let normalized = normalize_eval_json(&normalized);
    let parsed: RawSnap = serde_json::from_str(&normalized).unwrap_or(RawSnap {
        url: String::new(),
        title: String::new(),
        text: normalized.clone(),
        nodes: vec![],
        iframe_count: None,
    });
    let iframe_note = parsed.iframe_count.and_then(|n| {
        if n > 0 {
            Some(format!(
                "{n} iframe(s) present; inner content not snapshotted"
            ))
        } else {
            None
        }
    });
    BrowserSnapshotDto {
        url: parsed.url,
        title: parsed.title,
        text: parsed.text,
        nodes: parsed.nodes,
        screenshot: None,
        screenshot_note: None,
        iframe_note,
    }
}

/// Embed `s` as a JS string literal (quotes + escapes). Safe against injection;
/// do **not** wrap again in `JSON.parse` — that treats the content as JSON text
/// and breaks refs like `button:anon:0`.
fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// Click target center for CDP `Input.dispatchMouseEvent`.
pub fn click_point_js(ref_id: &str) -> String {
    let r = js_str(ref_id);
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var ref = {r};
    var el = zagensFindByRef(ref);
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    try {{ el.scrollIntoView({{ block: 'center', inline: 'nearest' }}); }} catch (e) {{}}
    var p = zagensParseRef(ref);
    var pt = zagensPagePoint(el, p.doc);
    var name = zagensName(el);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: zagensRole(el),
      name: name,
      x: pt.x,
      y: pt.y
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        r = r
    )
}

/// Focus a ref target before CDP `Input.insertText`.
pub fn focus_ref_js(ref_id: &str) -> String {
    let r = js_str(ref_id);
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var ref = {r};
    var el = zagensFindByRef(ref);
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    try {{ el.scrollIntoView({{ block: 'center', inline: 'nearest' }}); }} catch (e) {{}}
    el.focus();
    var name = zagensName(el);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: zagensRole(el),
      name: name,
      x: 0,
      y: 0
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        r = r
    )
}

/// Click element by stable snapshot ref (`role:slug:nth` or legacy tagged attr).
pub fn click_js(ref_id: &str) -> String {
    let r = js_str(ref_id);
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var ref = {r};
    var el = zagensFindByRef(ref);
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    try {{ el.scrollIntoView({{ block: 'center', inline: 'nearest' }}); }} catch (e) {{}}
    el.click();
    var name = zagensName(el);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: zagensRole(el),
      name: name
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        r = r
    )
}

/// Type into a ref target (input/textarea/contenteditable).
/// Agent strings are embedded via `js_str` (JSON string literals) — not `JSON.parse`.
pub fn type_js(ref_id: &str, text: &str) -> String {
    let r = js_str(ref_id);
    let t = js_str(text);
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var ref = {r};
    var text = {t};
    var el = zagensFindByRef(ref);
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    el.focus();
    if ('value' in el) {{
      zagensSetNativeValue(el, text);
      el.dispatchEvent(new Event('input', {{ bubbles: true }}));
      el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }} else if (el.isContentEditable) {{
      el.textContent = text;
      el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    }} else {{
      return JSON.stringify({{ ok: false, error: 'not_editable', ref: ref }});
    }}
    var name = zagensName(el);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: zagensRole(el),
      name: name,
      typedLen: text.length
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        r = r,
        t = t
    )
}

/// Scroll window or a ref container. direction: up|down|left|right.
pub fn scroll_js(ref_id: Option<&str>, direction: &str, amount: f64) -> String {
    let r = ref_id.map(js_str).unwrap_or_else(|| "null".into());
    let d = js_str(direction);
    let a = if amount.is_finite() && amount > 0.0 {
        amount
    } else {
        400.0
    };
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var ref = {r};
    var direction = {d};
    var amount = {a};
    var dx = 0, dy = 0;
    if (direction === 'up') dy = -amount;
    else if (direction === 'down') dy = amount;
    else if (direction === 'left') dx = -amount;
    else if (direction === 'right') dx = amount;
    else return JSON.stringify({{ ok: false, error: 'bad_direction', direction: direction }});
    var el = (ref && typeof ref === 'string') ? zagensFindByRef(ref) : null;
    if (el && typeof el.scrollBy === 'function') el.scrollBy(dx, dy);
    else window.scrollBy(dx, dy);
    return JSON.stringify({{ ok: true, ref: ref, direction: direction, amount: amount }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        r = r,
        d = d,
        a = a
    )
}

/// One-shot wait predicate check. kind: text|ref|selector|load.
pub fn wait_check_js(kind: &str, value: &str) -> String {
    let k = js_str(kind);
    let v = js_str(value);
    format!(
        r#"(function(){{
  {helpers}
  try {{
    var kind = {k};
    var value = {v};
    if (kind === 'load') {{
      var ready = document.readyState === 'complete' || document.readyState === 'interactive';
      return JSON.stringify({{ ok: ready, kind: kind, detail: document.readyState }});
    }}
    if (kind === 'text') {{
      var body = document.body ? (document.body.innerText || '') : '';
      var hit = body.indexOf(value) >= 0;
      return JSON.stringify({{ ok: hit, kind: kind, detail: hit ? 'found' : 'missing' }});
    }}
    if (kind === 'ref') {{
      var el = zagensFindByRef(value);
      return JSON.stringify({{ ok: !!el, kind: kind, detail: el ? 'found' : 'missing' }});
    }}
    if (kind === 'selector') {{
      var sel = null;
      try {{ sel = document.querySelector(value); }} catch (e) {{
        return JSON.stringify({{ ok: false, kind: kind, detail: 'bad_selector', error: String(e) }});
      }}
      return JSON.stringify({{ ok: !!sel, kind: kind, detail: sel ? 'found' : 'missing' }});
    }}
    return JSON.stringify({{ ok: false, kind: kind, detail: 'bad_kind' }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, kind: 'error', detail: String(err) }});
  }}
}})()"#,
        helpers = REF_HELPERS,
        k = k,
        v = v
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure Rust mirror of the page-side slug/ref algorithm (T4).
    fn stable_ref_rust(role: &str, name: &str, nth: usize) -> String {
        let role = role
            .to_ascii_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let role = if role.is_empty() {
            "unknown".into()
        } else {
            role
        };
        let slug = {
            let t = name
                .to_ascii_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect::<String>();
            let t = t.trim_matches('-').chars().take(40).collect::<String>();
            if t.is_empty() { "anon".into() } else { t }
        };
        format!("{role}:{slug}:{nth}")
    }

    #[test]
    fn snapshot_js_uses_stable_ref_shape() {
        let js = snapshot_js();
        assert!(js.contains("role + ':' + slug + ':' + nth"));
        assert!(js.contains("[contenteditable"));
        assert!(js.contains("iframe"));
        assert!(js.contains("zagensIsVisible"));
    }

    /// CDP snapshot emits AX roles (link/textbox/heading/...); the JS resolver must
    /// accept them via the alias table with visibility-aligned nth counting.
    #[test]
    fn find_by_ref_supports_ax_role_aliases_and_visibility() {
        let js = click_js("link:home:0");
        assert!(js.contains("zagensRoleMatches(cur, wantRole)"));
        assert!(js.contains("zagensIsVisible(cur)"));
        assert!(js.contains("zagensNameCandidates"));
        for role in [
            "link",
            "textbox",
            "textfield",
            "searchbox",
            "checkbox",
            "radio",
            "combobox",
            "listbox",
            "slider",
            "heading",
            "option",
            "button",
        ] {
            assert!(
                js.contains(&format!("case '{role}'")),
                "missing zagensRoleMatches alias case for {role}"
            );
        }
    }

    #[test]
    fn click_point_js_returns_coordinates() {
        let js = click_point_js("button:go:0");
        assert!(js.contains("zagensPagePoint"));
        assert!(js.contains("zagensFindByRef"));
        let frame_js = click_point_js("f1:button:go:0");
        assert!(frame_js.contains("zagensParseRef"));
    }

    #[test]
    fn focus_ref_js_focuses_element() {
        let js = focus_ref_js("input:name:0");
        assert!(js.contains(".focus()"));
    }

    #[test]
    fn click_js_uses_find_by_ref_and_js_string_literal() {
        let js = click_js("button:submit:0");
        assert!(js.contains("zagensFindByRef"));
        assert!(js.contains("var ref = \"button:submit:0\""));
        assert!(!js.contains("var ref = JSON.parse("));
    }

    #[test]
    fn wait_check_js_kinds() {
        let js = wait_check_js("text", "Hello");
        assert!(js.contains("indexOf"));
        let js = wait_check_js("ref", "a:home:0");
        assert!(js.contains("zagensFindByRef"));
        let js = wait_check_js("load", "");
        assert!(js.contains("readyState"));
    }

    #[test]
    fn parse_snapshot_keeps_stable_refs() {
        let raw = r#"{"url":"http://x/","title":"t","text":"hi","nodes":[{"ref":"button:go:0","role":"button","name":"Go"}],"iframeCount":1}"#;
        let snap = parse_snapshot_json(raw);
        assert_eq!(snap.nodes[0].r#ref, "button:go:0");
        assert!(snap.iframe_note.unwrap().contains("iframe"));
    }

    /// T1: agent-controlled strings must appear as JSON string literals, never `JSON.parse`
    /// (refs like `button:anon:0` are not valid JSON text).
    #[test]
    fn type_js_escapes_injection_payloads() {
        let evil = r#"';alert(1)//"#;
        let js = type_js("input:name:0", evil);
        assert!(js.contains("zagensSetNativeValue"));
        assert!(js.contains("var text = "));
        assert!(js.contains("var ref = "));
        assert!(!js.contains("var text = JSON.parse("));
        assert!(!js.contains("var ref = JSON.parse("));
        let encoded = serde_json::to_string(evil).unwrap();
        assert!(
            js.contains(&format!("var text = {encoded}")),
            "payload must be a JS string literal (JSON-encoded)"
        );
        // Unescaped break-out (quote then statement) must not appear without the JSON escape.
        assert!(!js.contains(r#"= "";alert(1)"#));
    }

    #[test]
    fn click_and_scroll_js_use_js_string_literals_for_args() {
        let click = click_js(r#"a:x":0"#);
        assert!(!click.contains("var ref = JSON.parse("));
        assert!(click.contains(&format!(
            "var ref = {}",
            serde_json::to_string(r#"a:x":0"#).unwrap()
        )));

        let scroll = scroll_js(Some("div:box:1"), "down", 120.0);
        assert!(!scroll.contains("var direction = JSON.parse("));
        assert!(scroll.contains(&format!(
            "var direction = {}",
            serde_json::to_string("down").unwrap()
        )));
        assert!(!scroll.contains("el.scrollBy(dx, dy); alert"));
    }

    #[test]
    fn parse_snapshot_unwraps_webview_string_encoding() {
        let inner = r#"{"url":"http://x/","title":"t","text":"hi","nodes":[{"ref":"button:go:0","role":"button","name":"Go"}]}"#;
        let wrapped = serde_json::to_string(inner).unwrap();
        let snap = parse_snapshot_json(&wrapped);
        assert_eq!(snap.url, "http://x/");
        assert_eq!(snap.nodes[0].r#ref, "button:go:0");
        assert_eq!(snap.text, "hi");
    }

    #[test]
    fn normalize_eval_json_leaves_objects() {
        let raw = r#"{"ok":true}"#;
        assert_eq!(normalize_eval_json(raw), raw);
        let wrapped = serde_json::to_string(raw).unwrap();
        assert_eq!(normalize_eval_json(&wrapped), raw);
    }

    /// T4: same role/name/nth → same stable ref; punctuation collapses in slug.
    #[test]
    fn stable_ref_is_deterministic_across_name_noise() {
        assert_eq!(stable_ref_rust("button", "Submit", 0), "button:submit:0");
        assert_eq!(stable_ref_rust("button", "Submit!!!", 0), "button:submit:0");
        assert_eq!(stable_ref_rust("button", "Submit", 1), "button:submit:1");
        assert_ne!(
            stable_ref_rust("a", "Home", 0),
            stable_ref_rust("button", "Home", 0)
        );
    }

    #[test]
    fn find_by_ref_regex_accepts_stable_shape() {
        let js = click_js("button:submit:0");
        assert!(
            js.contains(r#"/^([a-z0-9-]+):([a-z0-9-]+):(\d+)$/"#),
            "findByRef must parse role:slug:nth"
        );
    }

    #[test]
    fn snapshot_then_reparse_preserves_refs_for_click() {
        // Simulate: snapshot nodes → click uses same ref string.
        let snap = parse_snapshot_json(
            r#"{"url":"http://x/","title":"t","text":"hi","nodes":[
              {"ref":"button:go:0","role":"button","name":"Go"},
              {"ref":"button:go:1","role":"button","name":"Go"}
            ],"iframeCount":0}"#,
        );
        assert_eq!(snap.nodes.len(), 2);
        let js = click_js(&snap.nodes[1].r#ref);
        assert!(
            js.contains("button:go:1")
                || js.contains(&serde_json::to_string("button:go:1").unwrap())
        );
    }
}
