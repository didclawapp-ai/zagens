//! Write harness report deliverables from a [`ReportContext`] (markdown-only).

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use super::context::ReportContext;
use super::render::render_markdown;

#[derive(Debug, Clone, Default)]
pub struct ReportFormats {
    pub markdown: bool,
    pub docx: bool,
    pub xlsx: bool,
    pub pptx: bool,
}

impl ReportFormats {
    #[must_use]
    pub fn markdown_only() -> Self {
        Self {
            markdown: true,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn from_csv(raw: &str) -> Self {
        let parts: Vec<_> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            return Self::default_bundle();
        }
        let wants_office = parts.iter().any(|p| matches!(*p, "docx" | "xlsx" | "pptx"));
        Self {
            markdown: parts.contains(&"md") || parts.contains(&"markdown") || wants_office,
            docx: false,
            xlsx: false,
            pptx: false,
        }
    }

    #[must_use]
    pub fn default_bundle() -> Self {
        Self::markdown_only()
    }

    #[must_use]
    pub fn any_selected(&self) -> bool {
        self.markdown || self.docx || self.xlsx || self.pptx
    }
}

#[derive(Debug, Clone, Default)]
pub struct WrittenReport {
    pub out_dir: PathBuf,
    pub markdown: Option<PathBuf>,
    pub docx: Option<PathBuf>,
    pub xlsx: Option<PathBuf>,
    pub pptx: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub fn default_out_dir(workspace: &Path, ctx: &ReportContext) -> PathBuf {
    let stamp = ctx.generated_at.replace(':', "-").replace(' ', "_");
    workspace
        .join(".zagens")
        .join("deliverables")
        .join(format!("{}-{}", ctx.slug(), stamp))
}

pub fn write_report_bundle(
    workspace: &Path,
    out_dir: &Path,
    ctx: &ReportContext,
    formats: &ReportFormats,
) -> Result<WrittenReport> {
    if !formats.any_selected() {
        bail!("no output format selected — use --format md");
    }

    if formats.docx || formats.xlsx || formats.pptx {
        bail!(
            "built-in Office export (docx/xlsx/pptx) was removed; use --format md or the zagens-office skill"
        );
    }

    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    let base = ctx.slug();
    let mut written = WrittenReport {
        out_dir: out_dir.to_path_buf(),
        ..WrittenReport::default()
    };

    if formats.markdown {
        let path = out_dir.join(format!("{base}.md"));
        std::fs::write(&path, render_markdown(ctx))
            .with_context(|| format!("write markdown {}", path.display()))?;
        written.markdown = Some(path);
    }

    let _ = workspace;
    Ok(written)
}
