//! Write `docs/tech/openapi/zagens-runtime-v1.openapi.json` (D8).

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let json = deepseek_runtime::runtime_api::openapi::export_openapi_json();
    let out = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/tech/openapi/zagens-runtime-v1.openapi.json")
        });
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create openapi output dir");
    }
    fs::write(&out, json).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
}
