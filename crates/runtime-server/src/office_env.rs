//! Office Python environment diagnostics (for settings UI and errors).

use serde_json::{Value, json};
use std::process::Command;

/// Probe bundled/system Python and office library imports.
pub fn office_environment_status() -> Value {
    let bundled = crate::python_env::find_bundled_python()
        .map(|p| p.display().to_string());
    let system_python = crate::python_env::find_python().map(|(bin, maj, min)| {
        json!({ "binary": bin, "version": format!("{maj}.{min}") })
    });
    let venv_dir = crate::python_env::office_venv_dir()
        .map(|p| p.display().to_string());
    let venv_ready = venv_dir
        .as_ref()
        .map(|d| std::path::Path::new(d).join(".requirements-installed-v2").is_file())
        .unwrap_or(false);

    let resolve = crate::python_env::resolve_python_for_office().ok();
    let mut deps = json!({});
    if let Some(ref py) = resolve {
        deps = probe_office_imports(py);
    }

    json!({
        "bundled_python": bundled,
        "system_python": system_python,
        "office_venv_dir": venv_dir,
        "office_venv_ready": venv_ready,
        "resolved_python": resolve.as_ref().map(|p| p.display().to_string()),
        "imports": deps,
        "ready": resolve.is_some() && deps.get("python_docx").and_then(|v| v.as_bool()) == Some(true)
            && deps.get("python_pptx").and_then(|v| v.as_bool()) == Some(true)
            && deps.get("reportlab").and_then(|v| v.as_bool()) == Some(true),
    })
}

fn probe_office_imports(python: &std::path::Path) -> Value {
    let script = r#"
import json
mods = ["docx", "pptx", "reportlab"]
out = {}
for m in mods:
    try:
        __import__(m)
        out[m] = True
    except Exception as e:
        out[m] = str(e)
print(json.dumps(out))
"#;
    let output = Command::new(python)
        .args(["-c", script])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            if let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) {
                return json!({
                    "python_docx": v.get("docx").map(|x| x == &json!(true)).unwrap_or(false),
                    "python_pptx": v.get("pptx").map(|x| x == &json!(true)).unwrap_or(false),
                    "reportlab": v.get("reportlab").map(|x| x == &json!(true)).unwrap_or(false),
                    "detail": v,
                });
            }
            json!({ "error": "parse_failed", "stdout": stdout.to_string() })
        }
        Ok(o) => json!({
            "error": "probe_failed",
            "stderr": String::from_utf8_lossy(&o.stderr).to_string(),
        }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}
