//! Write `docs/tech/openapi/zagens-runtime-v1.openapi.json` (D8).

use std::env;
use std::fs;
use std::path::PathBuf;

use zagens_runtime::harness::symbol_search::{SymbolSearchHit, SymbolSearchResult};
use zagens_runtime::runtime_api::openapi::export_openapi_json_with;
use zagens_runtime_api::openapi::SchemaExportFn;

fn symbol_search_hit_schema() -> schemars::Schema {
    schemars::schema_for!(SymbolSearchHit)
}

fn symbol_search_result_schema() -> schemars::Schema {
    schemars::schema_for!(SymbolSearchResult)
}

fn main() {
    let extra: &[(&str, SchemaExportFn)] = &[
        ("SymbolSearchHit", symbol_search_hit_schema),
        ("SymbolSearchResult", symbol_search_result_schema),
    ];
    let json = export_openapi_json_with(extra);
    let out = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/tech/openapi/zagens-runtime-v1.openapi.json")
    });
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).expect("create openapi output dir");
    }
    fs::write(&out, json).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
    eprintln!("wrote {}", out.display());
}
