//! web.run wire and internal page types.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

#[derive(Default)]
pub(in crate::tools::web_run) struct WebRunState {
    pub(in crate::tools::web_run) sessions: HashMap<String, WebRunSessionState>,
    pub(in crate::tools::web_run) pages: HashMap<String, StoredWebPage>,
}

pub(in crate::tools::web_run) struct WebRunSessionState {
    pub(in crate::tools::web_run) next_turn: u64,
    pub(in crate::tools::web_run) refs: VecDeque<String>,
    pub(in crate::tools::web_run) last_access: Instant,
}

impl Default for WebRunSessionState {
    fn default() -> Self {
        Self {
            next_turn: 0,
            refs: VecDeque::new(),
            last_access: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::tools::web_run) struct StoredWebPage {
    pub(in crate::tools::web_run) namespace: String,
    pub(in crate::tools::web_run) page: WebPage,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct WebLink {
    pub(in crate::tools::web_run) id: usize,
    pub(in crate::tools::web_run) url: String,
    pub(in crate::tools::web_run) text: String,
}

#[derive(Debug, Clone)]
pub(in crate::tools::web_run) struct WebPage {
    pub(in crate::tools::web_run) url: String,
    pub(in crate::tools::web_run) title: Option<String>,
    pub(in crate::tools::web_run) content_type: Option<String>,
    pub(in crate::tools::web_run) lines: Vec<String>,
    pub(in crate::tools::web_run) links: Vec<WebLink>,
    pub(in crate::tools::web_run) pdf_pages: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::tools::web_run) enum ResponseLength {
    Short,
    Medium,
    Long,
}

impl ResponseLength {
    pub(in crate::tools::web_run) fn from_input(input: Option<&Value>) -> Self {
        let raw = input.and_then(|v| v.as_str()).unwrap_or("medium");
        match raw.to_lowercase().as_str() {
            "short" => Self::Short,
            "long" => Self::Long,
            _ => Self::Medium,
        }
    }

    pub(in crate::tools::web_run) fn view_lines(self) -> usize {
        match self {
            Self::Short => 40,
            Self::Medium => 80,
            Self::Long => 160,
        }
    }

    pub(in crate::tools::web_run) fn wrap_width(self) -> usize {
        match self {
            Self::Short => 88,
            Self::Medium => 110,
            Self::Long => 140,
        }
    }

    pub(in crate::tools::web_run) fn max_results(self) -> usize {
        match self {
            Self::Short => 5,
            Self::Medium => 8,
            Self::Long => 10,
        }
    }

    pub(in crate::tools::web_run) fn max_find_matches(self) -> usize {
        match self {
            Self::Short => 8,
            Self::Medium => 15,
            Self::Long => 30,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct SearchEntry {
    pub(in crate::tools::web_run) title: String,
    pub(in crate::tools::web_run) url: String,
    pub(in crate::tools::web_run) snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct SearchResult {
    pub(in crate::tools::web_run) ref_id: String,
    pub(in crate::tools::web_run) query: String,
    pub(in crate::tools::web_run) source: String,
    pub(in crate::tools::web_run) count: usize,
    pub(in crate::tools::web_run) results: Vec<SearchEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct PageViewResult {
    pub(in crate::tools::web_run) ref_id: String,
    pub(in crate::tools::web_run) url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) content_type: Option<String>,
    pub(in crate::tools::web_run) line_start: usize,
    pub(in crate::tools::web_run) line_end: usize,
    pub(in crate::tools::web_run) total_lines: usize,
    pub(in crate::tools::web_run) content: String,
    pub(in crate::tools::web_run) links: Vec<WebLink>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct FindMatch {
    pub(in crate::tools::web_run) line: usize,
    pub(in crate::tools::web_run) text: String,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct FindResult {
    pub(in crate::tools::web_run) ref_id: String,
    pub(in crate::tools::web_run) pattern: String,
    pub(in crate::tools::web_run) count: usize,
    pub(in crate::tools::web_run) matches: Vec<FindMatch>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct ScreenshotResult {
    pub(in crate::tools::web_run) ref_id: String,
    pub(in crate::tools::web_run) pageno: usize,
    pub(in crate::tools::web_run) total_pages: usize,
    pub(in crate::tools::web_run) content: String,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct ImageResultEntry {
    pub(in crate::tools::web_run) image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) height: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::tools::web_run) struct ImageQueryResult {
    pub(in crate::tools::web_run) query: String,
    pub(in crate::tools::web_run) source: String,
    pub(in crate::tools::web_run) count: usize,
    pub(in crate::tools::web_run) results: Vec<ImageResultEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub(in crate::tools::web_run) struct WebRunOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) search_query: Option<Vec<SearchResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) image_query: Option<Vec<ImageQueryResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) open: Option<Vec<PageViewResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) click: Option<Vec<PageViewResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) find: Option<Vec<FindResult>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::tools::web_run) screenshot: Option<Vec<ScreenshotResult>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(in crate::tools::web_run) warnings: Vec<String>,
}
