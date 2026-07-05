//! Shared report document model for markdown + Office templates (Phase 2b).

use serde::Serialize;

/// One logical report (harness telemetry or night-queue briefing).
#[derive(Debug, Clone, Serialize)]
pub struct ReportContext {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub generated_at: String,
    pub sections: Vec<ReportSection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReportSection {
    Summary {
        items: Vec<String>,
    },
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    Table {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

impl ReportContext {
    #[must_use]
    pub fn slug(&self) -> String {
        self.title
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else if c.is_whitespace() || c == '-' {
                    '-'
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .chars()
            .take(48)
            .collect()
    }
}
