//! Write Office deliverables from a [`ReportContext`].

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use super::context::ReportContext;
use super::render::{build_docx_payload, build_pptx_progress_payload, build_xlsx_evidence_payload};

#[derive(Debug, Clone, Default)]
pub struct ReportFormats {
    pub markdown: bool,
    pub docx: bool,
    pub xlsx: bool,
    pub pptx: bool,
}

impl ReportFormats {
    #[must_use]
    pub fn all_office() -> Self {
        Self {
            markdown: true,
            docx: true,
            xlsx: true,
            pptx: true,
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
        Self {
            markdown: parts.contains(&"md") || parts.contains(&"markdown"),
            docx: parts.contains(&"docx"),
            xlsx: parts.contains(&"xlsx"),
            pptx: parts.contains(&"pptx"),
        }
    }

    #[must_use]
    pub fn default_bundle() -> Self {
        Self {
            markdown: true,
            docx: true,
            xlsx: true,
            pptx: false,
        }
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
        bail!("no output format selected — use --format md,docx,xlsx,pptx");
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
        std::fs::write(&path, super::render::render_markdown(ctx))
            .with_context(|| format!("write markdown {}", path.display()))?;
        written.markdown = Some(path);
    }

    if formats.docx {
        let path = out_dir.join(format!("{base}.docx"));
        let payload = build_docx_payload(ctx);
        crate::tools::office_write::write_office_docx(&path, &payload)
            .map_err(|e| anyhow::anyhow!(e))
            .with_context(|| format!("write docx {}", path.display()))?;
        written.docx = Some(path);
    }

    if formats.xlsx {
        let path = out_dir.join(format!("{base}-evidence.xlsx"));
        let payload = build_xlsx_evidence_payload(ctx);
        crate::tools::office_write::write_office_xlsx(&path, workspace, &payload)
            .map_err(|e| anyhow::anyhow!(e))
            .with_context(|| format!("write xlsx {}", path.display()))?;
        written.xlsx = Some(path);
    }

    if formats.pptx {
        let path = out_dir.join(format!("{base}-progress.pptx"));
        let payload = build_pptx_progress_payload(ctx);
        match crate::tools::office_write::write_office_pptx(&path, &payload) {
            Ok(_engine) => {
                written.pptx = Some(path);
            }
            Err(err) => {
                written
                    .warnings
                    .push(format!("pptx skipped (python-pptx unavailable): {err}"));
            }
        }
    }

    Ok(written)
}
