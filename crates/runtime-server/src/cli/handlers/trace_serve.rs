//! `zagens trace serve` — local preview server with optional live watch.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use zagens_core::engine::trace_bundle_to_json;
use zagens_core::engine::{TraceBundle, build_trace_bundle_from_fixture};

use crate::cli::args::TraceServeArgs;
use crate::cli::context::CliContext;
use crate::cli::trace_thread::build_trace_bundle_for_thread_cli;
use crate::trace_export::{
    inject_trace_watch_script, load_trace_report_template, render_trace_html, trace_bundle_revision,
};

struct TraceServeState {
    ctx: CliContext,
    thread_id: Option<String>,
    fixture: Option<PathBuf>,
    include_harness: bool,
    redact: bool,
    template: String,
    watch: bool,
    watch_interval_secs: u64,
}

pub async fn run(ctx: &CliContext, args: TraceServeArgs) -> Result<ExitCode> {
    if args.watch && args.fixture.is_some() {
        bail!("--watch requires --thread (fixtures are static)");
    }
    if args.thread.is_none() && args.fixture.is_none() {
        bail!("specify --thread or --fixture");
    }
    if args.thread.is_some() && args.fixture.is_some() {
        bail!("--thread and --fixture are mutually exclusive");
    }

    let template = load_trace_report_template(args.template.as_deref())?;
    let state = Arc::new(TraceServeState {
        ctx: ctx.clone(),
        thread_id: args.thread.clone(),
        fixture: args.fixture.clone(),
        include_harness: args.include_harness,
        redact: !args.no_redact,
        template,
        watch: args.watch,
        watch_interval_secs: args.watch_interval_secs,
    });

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/api/bundle.json", get(serve_bundle))
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("invalid bind address {}:{}", args.host, args.port))?;

    eprintln!(
        "Kernel Trace Report preview → http://{addr}/{}",
        if args.watch { " (watch mode)" } else { "" }
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind {addr}"))?;
    axum::serve(listener, app).await.context("trace serve")?;

    Ok(ExitCode::SUCCESS)
}

async fn serve_index(
    State(state): State<Arc<TraceServeState>>,
) -> Result<Html<String>, StatusCode> {
    let state = state.clone();
    let html = tokio::task::spawn_blocking(move || build_index_html(&state))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(html))
}

async fn serve_bundle(State(state): State<Arc<TraceServeState>>) -> Result<Response, StatusCode> {
    let state = state.clone();
    let bundle = tokio::task::spawn_blocking(move || build_serve_bundle(&state))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let revision = trace_bundle_revision(&bundle);
    let json = trace_bundle_to_json(&bundle).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if let Ok(value) = HeaderValue::from_str(&revision) {
        headers.insert("X-Trace-Revision", value);
    }
    Ok((headers, json).into_response())
}

fn build_serve_bundle(state: &TraceServeState) -> Result<TraceBundle> {
    if let Some(id) = state.thread_id.as_deref() {
        return build_trace_bundle_for_thread_cli(
            &state.ctx,
            id,
            state.include_harness,
            state.redact,
        );
    }
    if let Some(path) = state.fixture.as_ref() {
        return build_trace_bundle_from_fixture(path)
            .with_context(|| format!("load fixture {}", path.display()));
    }
    bail!("no trace source configured")
}

fn build_index_html(state: &TraceServeState) -> Result<String> {
    let bundle = build_serve_bundle(state)?;
    let mut html = render_trace_html(&bundle, &state.template)?;
    if state.watch {
        html = inject_trace_watch_script(&html, state.watch_interval_secs);
    }
    Ok(html)
}
