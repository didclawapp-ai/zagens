//! Cross-format smoke tests for office read/write (CI-safe where possible).

#[cfg(test)]
mod tests {
    use crate::tools::office_common::{classify_office_generation_error, load_office_payload_file};
    use crate::tools::office_read::ReadOfficeTool;
    use crate::tools::office_write::WriteOfficeTool;
    use crate::tools::spec::{ToolContext, ToolSpec};
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    fn office_python_ready() -> bool {
        crate::office_env::office_environment_status()
            .get("ready")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    #[test]
    fn classify_office_errors_tags_deps_and_timeout() {
        let deps = classify_office_generation_error("fail", "ModuleNotFoundError: No module named 'docx'");
        assert!(deps.contains("[OFFICE_DEPS]"), "{deps}");
        let timeout = classify_office_generation_error("生成超时", "");
        assert!(timeout.contains("[OFFICE_TIMEOUT]"), "{timeout}");
    }

    #[tokio::test]
    async fn write_office_xlsx_default_deliverables_and_cache() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tool = WriteOfficeTool;
        let result = tool
            .execute(
                json!({
                    "format": "xlsx",
                    "title": "smoke-test",
                    "sheets": [{
                        "name": "Data",
                        "rows": [["A", "B"], [1, 2]]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success, "{}", result.content);
        assert!(result.content.contains("deliverables/"));
        let out = dir.path().join("deliverables/smoke-test.xlsx");
        assert!(out.is_file(), "xlsx missing");
        let bytes = fs::read(&out).expect("read");
        assert!(bytes.starts_with(b"PK"));
        load_office_payload_file(dir.path(), &out).expect("payload");
    }

    #[tokio::test]
    async fn write_office_docx_rust_fallback_smoke() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tool = WriteOfficeTool;
        let result = tool
            .execute(
                json!({
                    "format": "docx",
                    "title": "smoke-doc",
                    "blocks": [
                        { "type": "heading", "level": 1, "text": "Smoke" },
                        {
                            "type": "table",
                            "headers": ["ColA", "ColB"],
                            "rows": [["1", "2"]]
                        }
                    ]
                }),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success, "{}", result.content);
        let out = dir.path().join("deliverables/smoke-doc.docx");
        assert!(out.is_file());
        assert!(fs::read(&out).expect("read").starts_with(b"PK"));
        load_office_payload_file(dir.path(), &out).expect("payload cache");
    }

    #[tokio::test]
    async fn read_office_docx_table_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let write = WriteOfficeTool;
        write
            .execute(
                json!({
                    "format": "docx",
                    "title": "read-smoke",
                    "blocks": [
                        {
                            "type": "table",
                            "headers": ["指标", "数值"],
                            "rows": [["收入", "100"]]
                        }
                    ]
                }),
                &ctx,
            )
            .await
            .expect("write");

        let read = ReadOfficeTool;
        let result = read
            .execute(json!({ "path": "deliverables/read-smoke.docx" }), &ctx)
            .await
            .expect("read");

        assert!(result.success, "{}", result.content);
        assert!(
            result.content.contains("指标") && result.content.contains("收入"),
            "table text: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn write_office_xlsx_payload_incremental_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tool = WriteOfficeTool;
        let first = tool
            .execute(
                json!({
                    "format": "xlsx",
                    "title": "rt-inc",
                    "sheets": [{
                        "name": "Data",
                        "rows": [["Col", "Val"], ["A", 1]]
                    }]
                }),
                &ctx,
            )
            .await
            .expect("first");
        assert!(first.success, "{}", first.content);
        let path = "deliverables/rt-inc.xlsx";
        let payload = load_office_payload_file(dir.path(), &dir.path().join(path)).expect("load");
        let mut sheets = payload["sheets"].as_array().cloned().expect("sheets");
        if let Some(sheet) = sheets.get_mut(0) {
            sheet["rows"] = json!([["Col", "Val"], ["A", 2]]);
        }
        let second = tool
            .execute(
                json!({
                    "format": "xlsx",
                    "path": path,
                    "sheets": sheets
                }),
                &ctx,
            )
            .await
            .expect("second");
        assert!(second.success, "{}", second.content);
        let read = ReadOfficeTool;
        let out = read
            .execute(json!({ "path": path, "limit": 10 }), &ctx)
            .await
            .expect("read");
        assert!(out.content.contains('2'), "updated cell: {}", out.content);
    }

    #[tokio::test]
    async fn write_office_pptx_smoke_when_python_ready() {
        if !office_python_ready() {
            eprintln!("skip write_office_pptx_smoke: office python not ready");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = WriteOfficeTool
            .execute(
                json!({
                    "format": "pptx",
                    "title": "smoke-ppt",
                    "slides": [{ "title": "Cover", "bullets": ["Point A"] }]
                }),
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success, "{}", result.content);
        let out = dir.path().join("deliverables/smoke-ppt.pptx");
        assert!(fs::read(&out).expect("read").starts_with(b"PK"));
    }

    #[tokio::test]
    async fn write_office_pdf_smoke_when_python_ready() {
        if !office_python_ready() {
            eprintln!("skip write_office_pdf_smoke: office python not ready");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = WriteOfficeTool
            .execute(
                json!({
                    "format": "pdf",
                    "title": "smoke-pdf",
                    "blocks": [
                        { "type": "heading", "level": 1, "text": "Report" },
                        { "type": "paragraph", "text": "Body" }
                    ]
                }),
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success, "{}", result.content);
        let out = dir.path().join("deliverables/smoke-pdf.pdf");
        let bytes = fs::read(&out).expect("read");
        assert!(bytes.starts_with(b"%PDF"), "magic: {:?}", &bytes[..8.min(bytes.len())]);
    }
}
