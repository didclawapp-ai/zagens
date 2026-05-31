use super::*;

use crate::tools::spec::{ApprovalRequirement, ToolContext, ToolSpec};
use serde_json::{Value, json};
use std::fs;
use std::process::{Command, Stdio};
use tempfile::tempdir;

use super::read::{is_pdf, parse_pages_arg};

#[tokio::test]
async fn test_read_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a test file
        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write");

        let tool = ReadFileTool;
        let result = tool
            .execute(json!({"path": "test.txt"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert_eq!(result.content, "hello world");

        let md = result.metadata.as_ref().expect("metadata");
        assert_eq!(md["encoding_detected_via"], "streaming-line");
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = ReadFileTool;
        let result = tool.execute(json!({"path": "nonexistent.txt"}), &ctx).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_read_file_missing_path() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = ReadFileTool;
        let result = tool.execute(json!({}), &ctx).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Failed to validate input: missing required field 'path'")
        );
    }

    #[test]
    fn pdf_detected_by_extension() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("paper.PDF");
        fs::write(&path, b"not really a pdf, but extension says yes").unwrap();
        assert!(is_pdf(&path).unwrap());
    }

    #[test]
    fn pdf_detected_by_magic_bytes_without_extension() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("blob");
        fs::write(&path, b"%PDF-1.7\nrest of bytes").unwrap();
        assert!(is_pdf(&path).unwrap());
    }

    #[test]
    fn non_pdf_not_detected() {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("notes.txt");
        fs::write(&path, "hello").unwrap();
        assert!(!is_pdf(&path).unwrap());
    }

    #[test]
    fn pages_arg_parses_single_and_range() {
        assert_eq!(parse_pages_arg("5"), Some((5, 5)));
        assert_eq!(parse_pages_arg("1-10"), Some((1, 10)));
        assert_eq!(parse_pages_arg(" 3 - 7 "), Some((3, 7)));
        assert_eq!(parse_pages_arg("0"), None);
        assert_eq!(parse_pages_arg("10-3"), None);
        assert_eq!(parse_pages_arg(""), None);
        assert_eq!(parse_pages_arg("abc"), None);
    }

    #[tokio::test]
    async fn read_file_offset_alias_matches_start_line_precedence() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("lines.txt");
        fs::write(&test_file, "a\nb\nc").expect("write");

        let by_offset = ReadFileTool
            .execute(json!({"path": "lines.txt", "offset": 2}), &ctx)
            .await
            .expect("execute");

        assert_eq!(by_offset.content, "b\nc");

        let by_start_line = ReadFileTool
            .execute(json!({"path": "lines.txt", "start_line": 2}), &ctx)
            .await
            .expect("execute");

        assert_eq!(by_start_line.content, "b\nc");

        let start_line_wins = ReadFileTool
            .execute(
                json!({"path": "lines.txt", "start_line": 1, "offset": 3}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(
            start_line_wins.content.starts_with('a'),
            "{}",
            start_line_wins.content
        );
    }

    #[tokio::test]
    async fn read_file_exact_window_to_eof_without_trunc_notice_when_total_unknown() {
        // Large files skip total_lines_known — ensure we don't emit a truncation
        // footer when we read exactly to EOF (within the small-file line-count budget).

        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let lines: Vec<String> = (1..=800).map(|i| format!("line{i}")).collect();
        let content = lines.join("\n");
        assert!(content.len() < FILE_SIZE_LINE_COUNT_LIMIT as usize);
        let test_file = tmp.path().join("exact.txt");
        fs::write(&test_file, &content).expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "exact.txt", "limit": 800}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(!result.content.contains("接续"), "{}", result.content);
        let metadata = result.metadata.expect("metadata");
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_returns_binary_unavailable_when_pdftotext_missing_and_pdf_extract_fails() {
        // When pdftotext is missing and pdf-extract cannot parse the PDF,
        // a structured binary_unavailable response is returned.
        if Command::new("pdftotext")
            .arg("-v")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
        {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        let path = tmp.path().join("doc.pdf");
        fs::write(&path, b"%PDF-1.7\n%%EOF").unwrap();
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let result = ReadFileTool
            .execute(json!({"path": "doc.pdf"}), &ctx)
            .await
            .expect("structured response, not error");
        assert!(result.success);
        assert!(result.content.contains("binary_unavailable"));
    }

    #[tokio::test]
    async fn test_write_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = WriteFileTool;
        let result = tool
            .execute(
                json!({"path": "output.txt", "content": "test content"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        // Small new file → "Created …" summary + real unified diff (kept so the
        // web-ui DiffCard / 「本轮变更」panel render it; only LARGE writes switch
        // to the summary+preview path — see 3.1/3.2).
        assert!(result.content.contains("Created"), "{}", result.content);
        assert!(result.content.contains("--- a/"), "{}", result.content);
        assert!(
            result.content.contains("+test content"),
            "{}",
            result.content
        );

        // Verify file was written
        let written = fs::read_to_string(tmp.path().join("output.txt")).expect("read");
        assert_eq!(written, "test content");
    }

    #[tokio::test]
    async fn test_write_file_large_uses_preview_not_diff() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Exceed DIFF_MAX_INPUT_BYTES so the diff is intentionally skipped.
        let big = "x\n".repeat(DIFF_MAX_INPUT_BYTES);
        let tool = WriteFileTool;
        let result = tool
            .execute(json!({"path": "big.txt", "content": big}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(
            result.content.contains("diff omitted"),
            "{}",
            &result.content[..result.content.len().min(300)]
        );
        assert!(
            result.content.contains("preview (head)"),
            "{}",
            &result.content[..result.content.len().min(300)]
        );
        // Must NOT start with a unified-diff header (web-ui would misrender it).
        assert!(
            !result.content.contains("--- a/"),
            "{}",
            &result.content[..result.content.len().min(300)]
        );
    }

    #[tokio::test]
    async fn test_write_file_preserves_crlf() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let path = tmp.path().join("crlf.txt");
        fs::write(&path, "line1\r\nline2\r\n").expect("seed");

        let tool = WriteFileTool;
        tool.execute(
            json!({"path": "crlf.txt", "content": "newA\nnewB\n"}),
            &ctx,
        )
        .await
        .expect("execute");

        let written = fs::read(&path).expect("read");
        // LF input must be re-normalized to the file's original CRLF (2.1).
        assert_eq!(written, b"newA\r\nnewB\r\n");
    }

    #[tokio::test]
    async fn test_write_file_skips_identical_content() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let path = tmp.path().join("same.txt");
        fs::write(&path, "unchanged\n").expect("seed");

        let tool = WriteFileTool;
        let result = tool
            .execute(
                json!({"path": "same.txt", "content": "unchanged\n"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(
            result.content.contains("no changes"),
            "{}",
            result.content
        );
    }

    #[tokio::test]
    async fn test_write_file_too_large() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = WriteFileTool;
        let huge = "x".repeat(MAX_WRITE_SIZE + 1);
        let err = tool
            .execute(json!({"path": "big.txt", "content": huge}), &ctx)
            .await
            .expect_err("should reject oversized content");
        assert!(format!("{err:?}").contains("TOO_LARGE"), "{err:?}");
    }

    #[tokio::test]
    async fn test_write_file_creates_dirs() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let tool = WriteFileTool;
        let result = tool
            .execute(
                json!({"path": "subdir/nested/file.txt", "content": "nested content"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);

        // Verify nested file was created
        let written = fs::read_to_string(tmp.path().join("subdir/nested/file.txt")).expect("read");
        assert_eq!(written, "nested content");
    }

    #[tokio::test]
    async fn test_edit_file_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a file to edit
        let test_file = tmp.path().join("edit_me.txt");
        fs::write(&test_file, "hello world hello").expect("write");

        let tool = EditFileTool;
        let result = tool
            .execute(
                json!({"path": "edit_me.txt", "search": "hello", "replace": "hi", "replace_mode": "all"}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("2 occurrence(s)"));
        // Inline diff (#505) — the unified diff lands above the summary
        // line so the TUI's diff-aware renderer kicks in.
        assert!(result.content.contains("--- a/"), "{}", result.content);
        assert!(
            result.content.contains("-hello world hello"),
            "{}",
            result.content
        );
        assert!(
            result.content.contains("+hi world hi"),
            "{}",
            result.content
        );

        // Verify edit was applied
        let edited = fs::read_to_string(&test_file).expect("read");
        assert_eq!(edited, "hi world hi");
    }

    /// Tool surface audit T2 — empty `search` + replace_mode:"all" used to
    /// insert at every UTF-8 boundary and corrupt the whole file. Must error
    /// up front and leave the file byte-for-byte unchanged.
    #[tokio::test]
    async fn test_edit_file_empty_search_rejected() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("keep_me.txt");
        let original = "line one\nline two\nline three";
        fs::write(&test_file, original).expect("write");

        let tool = EditFileTool;
        for search in ["", "   ", "\n\t "] {
            let result = tool
                .execute(
                    json!({"path": "keep_me.txt", "search": search, "replace": "X", "replace_mode": "all"}),
                    &ctx,
                )
                .await;
            assert!(result.is_err(), "empty search {search:?} must error");
            assert!(result.unwrap_err().to_string().contains("search 不能为空"));
        }

        let after = fs::read_to_string(&test_file).expect("read");
        assert_eq!(after, original, "file must be untouched by rejected edits");
    }

    #[tokio::test]
    async fn test_edit_file_not_found() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a file without the search string
        let test_file = tmp.path().join("no_match.txt");
        fs::write(&test_file, "foo bar baz").expect("write");

        let tool = EditFileTool;
        let result = tool
            .execute(
                json!({"path": "no_match.txt", "search": "hello", "replace": "hi"}),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    /// #157 — When the model uses a wrong alias (`replacement`/`new_str`/…)
    /// instead of `replace`, the error names the right field directly so the
    /// model self-corrects without a second guess-and-retry round-trip.
    #[tokio::test]
    async fn test_edit_file_wrong_param_name_shows_provided_fields() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write");

        let tool = EditFileTool;
        // Model uses `replacement` instead of `replace`.
        let result = tool
            .execute(
                json!({"path": "test.txt", "search": "hello", "replacement": "hi"}),
                &ctx,
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // The hint must name the correct field AND the wrong alias used.
        assert!(
            err.contains("'replace'"),
            "error must name the correct field: {err}"
        );
        assert!(
            err.contains("replacement"),
            "error must name the wrong alias the model used: {err}"
        );
    }

    /// The same targeted hint fires for the `new_str` habit (carried over from
    /// other editing tools).
    #[tokio::test]
    async fn test_edit_file_new_str_alias_hint() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let test_file = tmp.path().join("test.txt");
        fs::write(&test_file, "hello world").expect("write");

        let tool = EditFileTool;
        let result = tool
            .execute(
                json!({"path": "test.txt", "search": "hello", "new_str": "hi"}),
                &ctx,
            )
            .await;

        let err = result.unwrap_err().to_string();
        assert!(err.contains("'replace'") && err.contains("new_str"), "{err}");
    }

    #[tokio::test]
    async fn test_list_dir_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create some files and directories
        fs::write(tmp.path().join("file1.txt"), "").expect("write");
        fs::write(tmp.path().join("file2.txt"), "").expect("write");
        fs::create_dir(tmp.path().join("subdir")).expect("mkdir");

        let tool = ListDirTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");

        assert!(result.success);
        assert!(result.content.contains("file1.txt"));
        assert!(result.content.contains("file2.txt"));
        assert!(result.content.contains("subdir"));
        assert!(result.content.contains("\"is_dir\": true"));
    }

    #[tokio::test]
    async fn test_list_dir_sorts_dirs_first_then_name() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());
        fs::write(tmp.path().join("b.txt"), "").expect("write");
        fs::write(tmp.path().join("a.txt"), "").expect("write");
        fs::create_dir(tmp.path().join("zdir")).expect("mkdir");

        let tool = ListDirTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");
        let parsed: serde_json::Value = serde_json::from_str(&result.content).expect("json");
        let entries = parsed["entries"].as_array().expect("entries array");

        // Directory comes first regardless of name, then files sorted by name.
        assert_eq!(entries[0]["name"], "zdir");
        assert_eq!(entries[0]["is_dir"], true);
        assert_eq!(entries[1]["name"], "a.txt");
        assert_eq!(entries[2]["name"], "b.txt");
        assert_eq!(parsed["truncated"], false);
        assert_eq!(parsed["total"], 3);
    }

    #[tokio::test]
    async fn test_list_dir_with_path() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        // Create a subdirectory with files
        let subdir = tmp.path().join("mydir");
        fs::create_dir(&subdir).expect("mkdir");
        fs::write(subdir.join("nested.txt"), "").expect("write");

        let tool = ListDirTool;
        let result = tool
            .execute(json!({"path": "mydir"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("nested.txt"));
    }

    #[tokio::test]
    async fn test_list_dir_offset_paginates() {
        let tmp = tempdir().expect("tempdir");
        for i in 0..5 {
            std::fs::write(tmp.path().join(format!("file_{i}.txt")), "x\n").expect("write");
        }

        let ctx = ToolContext::new(tmp.path().to_path_buf());
        let tool = ListDirTool;
        let result = tool
            .execute(json!({"limit": 2, "offset": 2}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        let parsed: Value = serde_json::from_str(&result.content).expect("json");
        assert_eq!(parsed["total"].as_u64().unwrap(), 5);
        assert_eq!(parsed["offset"].as_u64().unwrap(), 2);
        assert_eq!(parsed["returned"].as_u64().unwrap(), 2);
        assert!(parsed["truncated"].as_bool().unwrap());
    }

    #[test]
    fn test_read_file_tool_properties() {
        let tool = ReadFileTool;
        assert_eq!(tool.name(), "read_file");
        assert!(tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn test_write_file_tool_properties() {
        let tool = WriteFileTool;
        assert_eq!(tool.name(), "write_file");
        assert!(!tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Suggest);
    }

    #[test]
    fn test_edit_file_tool_properties() {
        let tool = EditFileTool;
        assert_eq!(tool.name(), "edit_file");
        assert!(!tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Suggest);
    }

    #[test]
    fn test_list_dir_tool_properties() {
        let tool = ListDirTool;
        assert_eq!(tool.name(), "list_dir");
        assert!(tool.is_read_only());
        assert!(tool.is_sandboxable());
        assert_eq!(tool.approval_requirement(), ApprovalRequirement::Auto);
    }

    #[test]
    fn test_parallel_support_flags() {
        let read_tool = ReadFileTool;
        let list_tool = ListDirTool;
        let write_tool = WriteFileTool;

        assert!(read_tool.supports_parallel());
        assert!(list_tool.supports_parallel());
        assert!(!write_tool.supports_parallel());
    }

    #[test]
    fn test_input_schemas() {
        // Verify all tools have valid JSON schemas
        let read_schema = ReadFileTool.input_schema();
        assert!(read_schema.get("type").is_some());
        let props = read_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("read schema should have properties");
        assert!(props.contains_key("path"));
        assert!(props.contains_key("start_line"));
        assert!(props.contains_key("offset"));
        assert!(props.contains_key("limit"));
        assert!(props.contains_key("pages"));

        let write_schema = WriteFileTool.input_schema();
        let required = write_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("write schema should include required array");
        assert!(required.iter().any(|v| v.as_str() == Some("path")));
        assert!(required.iter().any(|v| v.as_str() == Some("content")));

        let edit_schema = EditFileTool.input_schema();
        let required = edit_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("edit schema should include required array");
        assert_eq!(required.len(), 1); // only 'path' is required (operation defaults to search_replace)

        let list_schema = ListDirTool.input_schema();
        let required = list_schema
            .get("required")
            .and_then(|value| value.as_array())
            .expect("list schema should include required array");
        assert!(required.is_empty()); // path is optional
    }

    #[tokio::test]
    async fn read_file_start_line_skips_leading_lines() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("multiline.txt");
        fs::write(&test_file, "line1\nline2\nline3\nline4\nline5").expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "multiline.txt", "start_line": 3}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert_eq!(result.content, "line3\nline4\nline5");

        let metadata = result.metadata.expect("should have metadata");
        assert_eq!(metadata["lines_read"], 3);
        assert_eq!(metadata["total_lines"], 5);
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_limit_truncates_with_notice() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let lines: Vec<String> = (1..=50).map(|i| format!("line{i}")).collect();
        let content = lines.join("\n");
        let test_file = tmp.path().join("many_lines.txt");
        fs::write(&test_file, &content).expect("write");

        let result = ReadFileTool
            .execute(
                json!({"path": "many_lines.txt", "start_line": 1, "limit": 10}),
                &ctx,
            )
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.contains("line1\nline2"));
        assert!(result.content.contains("line10"));
        assert!(!result.content.contains("line11"));
        assert!(result.content.contains("..."));
        assert!(result.content.contains("共 50 行"));

        let metadata = result.metadata.expect("should have metadata");
        assert_eq!(metadata["lines_read"], 10);
        assert_eq!(metadata["total_lines"], 50);
        assert!(metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_metadata_includes_path_and_size() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("meta_test.txt");
        let body = "hello metadata test";
        fs::write(&test_file, body).expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "meta_test.txt"}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        let metadata = result.metadata.expect("should have metadata");
        assert!(metadata["path"].as_str().unwrap().contains("meta_test.txt"));
        assert!(metadata["size_bytes"].as_u64().unwrap() > 0);
        assert_eq!(metadata["lines_read"], 1);
        assert!(!metadata["truncated"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn read_file_start_line_past_end_returns_empty_with_metadata() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let test_file = tmp.path().join("short.txt");
        fs::write(&test_file, "only two\nlines here").expect("write");

        let result = ReadFileTool
            .execute(json!({"path": "short.txt", "start_line": 10}), &ctx)
            .await
            .expect("execute");

        assert!(result.success);
        assert!(result.content.is_empty());

        let metadata = result.metadata.expect("should have metadata");
    assert_eq!(metadata["lines_read"], 0);
    assert_eq!(metadata["total_lines"], 2);
}

// --- Tool surface audit C8 — encoding preservation on write/edit/fim ---

#[test]
fn encode_text_roundtrips_gb18030() {
    let text = "中文内容 abc";
    let bytes = encode_text(text, "gb18030", false);
    // GB18030 of multi-byte Chinese is not valid UTF-8.
    assert!(std::str::from_utf8(&bytes).is_err());
    let (decoded, label, _via) = detect_and_decode(&bytes);
    assert_eq!(decoded, text);
    assert_eq!(label, "gb18030");
}

#[test]
fn encode_text_roundtrips_utf16le_with_bom() {
    let text = "héllo 世界";
    let bytes = encode_text(text, "utf-16le", true);
    assert_eq!(&bytes[..2], &[0xFF, 0xFE], "UTF-16LE BOM must be restored");
    let (decoded, label, via) = detect_and_decode(&bytes);
    assert_eq!(decoded, text);
    assert_eq!(label, "utf-16le");
    assert_eq!(via, "bom");
}

#[test]
fn encode_text_roundtrips_utf16be_with_bom() {
    let text = "abc 你好";
    let bytes = encode_text(text, "utf-16be", true);
    assert_eq!(&bytes[..2], &[0xFE, 0xFF], "UTF-16BE BOM must be restored");
    let (decoded, _label, _via) = detect_and_decode(&bytes);
    assert_eq!(decoded, text);
}

#[test]
fn encode_text_unknown_label_falls_back_to_utf8() {
    let text = "plain";
    let bytes = encode_text(text, "iso-2022-jp", false);
    assert_eq!(bytes, text.as_bytes());
}

#[tokio::test]
async fn test_write_file_preserves_gb18030_encoding() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path().to_path_buf());
    let path = tmp.path().join("gb.txt");

    // Seed a GB18030-encoded file.
    let seed_bytes = encoding_rs::GB18030.encode("原始中文").0.into_owned();
    fs::write(&path, &seed_bytes).expect("seed");

    let result = WriteFileTool
        .execute(json!({"path": "gb.txt", "content": "新的中文内容"}), &ctx)
        .await
        .expect("execute");
    assert!(result.success);

    let written = fs::read(&path).expect("read");
    assert!(
        std::str::from_utf8(&written).is_err(),
        "GB18030 file must NOT be silently transcoded to UTF-8"
    );
    let (decoded, label, _via) = detect_and_decode(&written);
    assert_eq!(decoded, "新的中文内容");
    assert_eq!(label, "gb18030");
}

#[tokio::test]
async fn test_edit_file_preserves_gb18030_encoding() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path().to_path_buf());
    let path = tmp.path().join("edit_gb.txt");

    let seed_bytes = encoding_rs::GB18030.encode("你好世界").0.into_owned();
    fs::write(&path, &seed_bytes).expect("seed");

    let result = EditFileTool
        .execute(
            json!({"path": "edit_gb.txt", "search": "世界", "replace": "中国"}),
            &ctx,
        )
        .await
        .expect("execute");
    assert!(result.success);

    let written = fs::read(&path).expect("read");
    assert!(
        std::str::from_utf8(&written).is_err(),
        "edit must keep GB18030 bytes, not UTF-8"
    );
    let (decoded, label, _via) = detect_and_decode(&written);
    assert_eq!(decoded, "你好中国");
    assert_eq!(label, "gb18030");
}

#[tokio::test]
async fn test_edit_file_preserves_utf16le_bom() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path().to_path_buf());
    let path = tmp.path().join("u16.txt");

    // UTF-16LE with BOM.
    let mut seed = vec![0xFF, 0xFEu8];
    for u in "alpha beta".encode_utf16() {
        seed.extend_from_slice(&u.to_le_bytes());
    }
    fs::write(&path, &seed).expect("seed");

    let result = EditFileTool
        .execute(
            json!({"path": "u16.txt", "search": "beta", "replace": "gamma"}),
            &ctx,
        )
        .await
        .expect("execute");
    assert!(result.success);

    let written = fs::read(&path).expect("read");
    assert_eq!(&written[..2], &[0xFF, 0xFE], "UTF-16LE BOM must survive edit");
    let (decoded, label, _via) = detect_and_decode(&written);
    assert_eq!(decoded, "alpha gamma");
    assert_eq!(label, "utf-16le");
}
