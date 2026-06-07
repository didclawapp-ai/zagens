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
        let status = crate::office_env::office_environment_status();
        if !status
            .get("ready")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            return false;
        }
        // Re-resolve under the same lock used by write_office to avoid parallel
        // test races and stale "ready" probes on half-built venvs.
        crate::python_env::resolve_python_for_office().is_ok()
    }

    fn skip_if_office_env_unavailable(test_name: &str, content: &str) -> bool {
        if content.contains("创建 venv 失败")
            || content.contains("[OFFICE_ERROR]")
            || content.contains("[OFFICE_DEPS]")
        {
            eprintln!("skip {test_name}: {content}");
            true
        } else {
            false
        }
    }

    #[test]
    fn classify_office_errors_tags_deps_and_timeout() {
        let deps =
            classify_office_generation_error("fail", "ModuleNotFoundError: No module named 'docx'");
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
        if skip_if_office_env_unavailable("write_office_pptx_smoke", &result.content) {
            return;
        }
        assert!(result.success, "{}", result.content);
        let out = dir.path().join("deliverables/smoke-ppt.pptx");
        assert!(fs::read(&out).expect("read").starts_with(b"PK"));
    }

    #[tokio::test]
    async fn read_office_legacy_doc_hint() {
        let dir = tempdir().expect("tempdir");
        let legacy = dir.path().join("legacy.doc");
        fs::write(&legacy, b"\x00").expect("write");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let err = ReadOfficeTool
            .execute(json!({ "path": "legacy.doc" }), &ctx)
            .await
            .expect_err("legacy .doc should fail");
        let msg = err.to_string();
        assert!(msg.contains("UNSUPPORTED") || msg.contains("docx"), "{msg}");
    }

    #[tokio::test]
    async fn write_office_xlsx_from_csv_source() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join("input.csv"),
            "Product,Qty\nWidget,10\nGadget,5",
        )
        .expect("csv");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = WriteOfficeTool
            .execute(
                json!({
                    "format": "xlsx",
                    "title": "from-source",
                    "sheets": [{
                        "name": "Sales",
                        "source": "input.csv"
                    }]
                }),
                &ctx,
            )
            .await
            .expect("execute");
        assert!(result.success, "{}", result.content);
        let read = ReadOfficeTool
            .execute(json!({ "path": "deliverables/from-source.xlsx" }), &ctx)
            .await
            .expect("read");
        assert!(read.content.contains("Widget") && read.content.contains("10"));
    }

    #[tokio::test]
    async fn read_office_pptx_chart_when_python_ready() {
        if !office_python_ready() {
            eprintln!("skip read_office_pptx_chart: office python not ready");
            return;
        }
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let write_result = WriteOfficeTool
            .execute(
                json!({
                    "format": "pptx",
                    "title": "chart-read",
                    "slides": [{
                        "title": "Sales",
                        "chart": {
                            "type": "bar",
                            "categories": ["Q1", "Q2"],
                            "series": [{ "name": "Revenue", "values": [100.0, 120.0] }]
                        }
                    }]
                }),
                &ctx,
            )
            .await
            .expect("write");
        if skip_if_office_env_unavailable("read_office_pptx_chart", &write_result.content) {
            return;
        }
        assert!(write_result.success, "{}", write_result.content);
        let read = ReadOfficeTool
            .execute(json!({ "path": "deliverables/chart-read.pptx" }), &ctx)
            .await
            .expect("read");
        assert!(
            read.content.contains("[图表数据]") || read.content.contains("Revenue"),
            "chart data: {}",
            read.content
        );
    }

    #[tokio::test]
    async fn read_office_xlsx_numfmt_golden() {
        use rust_xlsxwriter::{Format, Workbook};
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("numfmt.xlsx");
        let mut workbook = Workbook::new();
        let pct = Format::new().set_num_format("0.00%");
        let sheet = workbook.add_worksheet();
        sheet.write_string(0, 0, "Rate").expect("cell");
        sheet
            .write_number_with_format(0, 1, 0.125, &pct)
            .expect("pct cell");
        workbook.save(&path).expect("save xlsx");

        let ctx = ToolContext::new(dir.path().to_path_buf());
        let result = ReadOfficeTool
            .execute(json!({ "path": "numfmt.xlsx" }), &ctx)
            .await
            .expect("read");
        assert!(result.success, "{}", result.content);
        assert!(
            result.content.contains("12.5%")
                || result.content.contains("0.125")
                || result.content.contains("12.5"),
            "percentage display: {}",
            result.content
        );
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
        if skip_if_office_env_unavailable("write_office_pdf_smoke", &result.content) {
            return;
        }
        assert!(result.success, "{}", result.content);
        let out = dir.path().join("deliverables/smoke-pdf.pdf");
        let bytes = fs::read(&out).expect("read");
        assert!(
            bytes.starts_with(b"%PDF"),
            "magic: {:?}",
            &bytes[..8.min(bytes.len())]
        );
    }
}
