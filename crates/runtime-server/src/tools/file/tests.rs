use super::*;

use crate::tools::spec::{ApprovalRequirement, ToolContext, ToolSpec};
use serde_json::json;
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
        // New file → "Created …" summary; the unified diff above the summary
        // primes the TUI's diff-aware renderer (#505).
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

    /// #157 — When the model uses `replacement` instead of `replace`,
    /// the error should name the provided fields so the model can
    /// self-correct without a second round-trip.
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
        // The error must name both the missing field AND the provided ones.
        assert!(
            err.contains("missing required field 'replace'"),
            "error must name the missing field: {err}"
        );
        assert!(
            err.contains("Input provided:") || err.contains("provided:"),
            "error must list the fields the model did supply: {err}"
        );
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
