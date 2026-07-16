//! Injected page scripts for snapshot / console capture.

use serde::Deserialize;

use super::{BrowserA11yNode, BrowserSnapshotDto};

pub const SNAPSHOT_JS: &str = r#"(function(){
  try {
    var text = document.body ? (document.body.innerText || '').slice(0, 50000) : '';
    var title = document.title || '';
    var url = location.href || '';
    var nodes = [];
    var ref = 0;
    var els = document.querySelectorAll('a,button,input,textarea,select,[role="button"],[role="link"],h1,h2,h3');
    for (var i = 0; i < els.length && nodes.length < 100; i++) {
      var el = els[i];
      var r = 'e' + (++ref);
      try { el.setAttribute('data-zagens-ref', r); } catch (e) {}
      var name = (el.getAttribute('aria-label') || el.innerText || el.value || el.getAttribute('placeholder') || '').trim().slice(0, 120);
      nodes.push({ ref: r, role: el.getAttribute('role') || el.tagName.toLowerCase(), name: name });
    }
    return JSON.stringify({ url: url, title: title, text: text, nodes: nodes });
  } catch (err) {
    return JSON.stringify({ url: location.href || '', title: '', text: String(err), nodes: [] });
  }
})()"#;

/// Install a ring buffer of console messages on the page (idempotent).
pub const CONSOLE_HOOK_JS: &str = r#"(function(){
  try {
    if (window.__zagensConsoleHooked) return true;
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
struct RawSnap {
    url: String,
    title: String,
    text: String,
    nodes: Vec<BrowserA11yNode>,
}

pub fn parse_snapshot_json(raw: &str) -> BrowserSnapshotDto {
    let parsed: RawSnap = serde_json::from_str(raw).unwrap_or(RawSnap {
        url: String::new(),
        title: String::new(),
        text: raw.to_string(),
        nodes: vec![],
    });
    BrowserSnapshotDto {
        url: parsed.url,
        title: parsed.title,
        text: parsed.text,
        nodes: parsed.nodes,
        screenshot: None,
        screenshot_note: None,
    }
}

fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// Click element previously tagged by snapshot (`data-zagens-ref`).
pub fn click_js(ref_id: &str) -> String {
    let r = js_str(ref_id);
    format!(
        r#"(function(){{
  try {{
    var ref = {r};
    var el = document.querySelector('[data-zagens-ref="' + ref + '"]');
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    try {{ el.scrollIntoView({{ block: 'center', inline: 'nearest' }}); }} catch (e) {{}}
    el.click();
    var name = (el.getAttribute('aria-label') || el.innerText || el.value || '').trim().slice(0, 120);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: el.getAttribute('role') || el.tagName.toLowerCase(),
      name: name
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#
    )
}

/// Type into a ref target (input/textarea/contenteditable).
pub fn type_js(ref_id: &str, text: &str) -> String {
    let r = js_str(ref_id);
    let t = js_str(text);
    format!(
        r#"(function(){{
  try {{
    var ref = {r};
    var text = {t};
    var el = document.querySelector('[data-zagens-ref="' + ref + '"]');
    if (!el) return JSON.stringify({{ ok: false, error: 'ref_not_found', ref: ref }});
    el.focus();
    if ('value' in el) {{
      el.value = text;
      el.dispatchEvent(new Event('input', {{ bubbles: true }}));
      el.dispatchEvent(new Event('change', {{ bubbles: true }}));
    }} else if (el.isContentEditable) {{
      el.textContent = text;
      el.dispatchEvent(new Event('input', {{ bubbles: true }}));
    }} else {{
      return JSON.stringify({{ ok: false, error: 'not_editable', ref: ref }});
    }}
    var name = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || '').trim().slice(0, 120);
    return JSON.stringify({{
      ok: true,
      ref: ref,
      role: el.getAttribute('role') || el.tagName.toLowerCase(),
      name: name,
      typedLen: text.length
    }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#
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
    var el = (ref && typeof ref === 'string')
      ? document.querySelector('[data-zagens-ref="' + ref + '"]')
      : null;
    if (el && typeof el.scrollBy === 'function') el.scrollBy(dx, dy);
    else window.scrollBy(dx, dy);
    return JSON.stringify({{ ok: true, ref: ref, direction: direction, amount: amount }});
  }} catch (err) {{
    return JSON.stringify({{ ok: false, error: String(err) }});
  }}
}})()"#
    )
}
