//! Symbol index search API (Phase 3.5).

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

use crate::harness::symbol_search::search_workspace_symbols;

use super::{ApiError, RuntimeApiState};

#[derive(Debug, Deserialize)]
pub(crate) struct SymbolSearchQuery {
    pub q: String,
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

pub(crate) async fn search_symbol_index(
    State(state): State<RuntimeApiState>,
    Query(query): Query<SymbolSearchQuery>,
) -> Result<Json<crate::harness::SymbolSearchResult>, ApiError> {
    let limit = query.limit.unwrap_or(25);
    let result = search_workspace_symbols(
        state.workspace.as_path(),
        &query.q,
        query.kind.as_deref(),
        limit,
    );
    Ok(Json(result))
}
