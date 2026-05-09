//! File metadata inspection tool (`file_info`).
//!
//! Returns structured metadata about a file: size, modification time,
//! binary/text classification, PDF detection, line count (for small files).

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};

use super::file::sniff_encoding_label;
use super::spec::{ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, required_str};

const LINE_COUNT_SIZE_LIMIT: u64 = 10 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8192;
const BINARY_PRINTABLE_RATIO: f64 = 0.7;

pub struct FileInfoTool;

#[async_trait]
impl ToolSpec for FileInfoTool {
    fn name(&self) -> &'static str {
        "file_info"
    }

    fn description(&self) -> &'static str {
        "Get file metadata: size, mtime (RFC3339 UTC), text/binary sniff, PDF detection, line count for files ≤10MB, encoding_guess from a head sample."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to workspace or absolute)"
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Sandboxable]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let file_path = context.resolve_path(path_str)?;

        let metadata = fs::metadata(&file_path).map_err(|e| {
            let kind = e.kind();
            if kind == std::io::ErrorKind::NotFound {
                ToolError::execution_failed(format!(
                    "[NOT_FOUND] 文件 {} 不存在: {e}",
                    file_path.display()
                ))
            } else if kind == std::io::ErrorKind::PermissionDenied {
                ToolError::execution_failed(format!(
                    "[PERMISSION] 没有权限读取 {}: {e}",
                    file_path.display()
                ))
            } else {
                ToolError::execution_failed(format!("Failed to stat {}: {e}", file_path.display()))
            }
        })?;

        let size_bytes = metadata.len();
        let mtime = metadata
            .modified()
            .ok()
            .map(|st| DateTime::<Utc>::from(st).to_rfc3339_opts(SecondsFormat::Secs, true));

        let is_pdf_flag = is_pdf(&file_path).unwrap_or(false);

        let head_sample = match read_sniff_sample(&file_path, BINARY_SNIFF_BYTES) {
            Ok(b) => b,
            Err(_) => Vec::new(),
        };

        let (is_binary, binary_reason) = detect_binary_sniff(&head_sample);

        let is_text = !is_pdf_flag && !is_binary;

        let line_count: Option<usize> = if is_text && size_bytes <= LINE_COUNT_SIZE_LIMIT {
            count_lines(&file_path).ok()
        } else {
            None
        };

        let encoding_guess: Value = if is_pdf_flag || is_binary {
            Value::Null
        } else {
            match sniff_encoding_label(&head_sample) {
                Some(s) => serde_json::Value::String(s),
                None => Value::Null,
            }
        };

        let result = json!({
            "path": file_path.to_string_lossy(),
            "size_bytes": size_bytes,
            "mtime": mtime,
            "is_text": is_text,
            "is_binary": is_binary,
            "binary_reason": binary_reason,
            "is_pdf": is_pdf_flag,
            "line_count": line_count,
            "encoding_guess": encoding_guess,
        });

        ToolResult::json(&result).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

fn is_pdf(path: &Path) -> Result<bool, ToolError> {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    {
        return Ok(true);
    }
    let mut buf = [0u8; 4];
    match fs::File::open(path) {
        Ok(mut f) => {
            use std::io::Read;
            match f.read_exact(&mut buf) {
                Ok(()) => {}
                Err(_) => return Ok(false),
            }
        }
        Err(_) => return Ok(false),
    };
    Ok(&buf == b"%PDF")
}

fn read_sniff_sample(path: &Path, limit: usize) -> Result<Vec<u8>, ToolError> {
    use std::io::Read;

    let mut buf = vec![0u8; limit];
    let n = fs::File::open(path)
        .map_err(|e| ToolError::execution_failed(e.to_string()))?
        .take(limit as u64)
        .read(&mut buf)
        .map_err(|e| ToolError::execution_failed(e.to_string()))?;
    buf.truncate(n);
    Ok(buf)
}

fn detect_binary_sniff(head: &[u8]) -> (bool, Option<String>) {
    if head.is_empty() {
        return (false, None);
    }

    let printable = head.iter().filter(|&&b| is_printable_byte(b)).count();
    let ratio = printable as f64 / head.len() as f64;

    if ratio < BINARY_PRINTABLE_RATIO {
        let pct = ((1.0 - ratio) * 100.0).round() as u32;
        (
            true,
            Some(format!(
                "{pct}% non-printable bytes in first {} bytes",
                head.len()
            )),
        )
    } else {
        (false, None)
    }
}

fn is_printable_byte(b: u8) -> bool {
    matches!(b, b'\n' | b'\r' | b'\t' | b' ' | 0x20..=0x7E | 0x80..=0xFF)
}

fn count_lines(path: &Path) -> Result<usize, ToolError> {
    let file = fs::File::open(path)
        .map_err(|e| ToolError::execution_failed(format!("Failed to open for line count: {e}")))?;
    let reader = BufReader::new(file);
    let count = reader.lines().map_while(|r| r.ok()).count();
    Ok(count)
}
