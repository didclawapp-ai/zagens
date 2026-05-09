// ---------------------------------------------------------------------------
// CsvRenderer — lightweight CSV / TSV table preview.
//
// Handles:
//  - Comma, tab, and semicolon delimiters (auto-detected)
//  - Quoted fields (double-quote escaping)
//  - Truncation at 1000 rows with a warning
//  - Horizontal scroll for wide tables
// ---------------------------------------------------------------------------

import { useMemo } from 'react';
import type { RendererProps } from '../types';

// ---- parser ----------------------------------------------------------------

interface ParsedCsv {
  headers: string[];
  rows: string[][];
  truncated: boolean;
}

function detectDelimiter(firstLine: string): string {
  const tabs = (firstLine.match(/\t/g) ?? []).length;
  const commas = (firstLine.match(/,/g) ?? []).length;
  const semis = (firstLine.match(/;/g) ?? []).length;
  if (tabs >= commas && tabs >= semis && tabs > 0) return '\t';
  if (semis > commas && semis > 0) return ';';
  return ',';
}

function splitRow(line: string, delim: string): string[] {
  const fields: string[] = [];
  let current = '';
  let inQuote = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQuote) {
      if (ch === '"') {
        if (i + 1 < line.length && line[i + 1] === '"') {
          current += '"';
          i++;
        } else {
          inQuote = false;
        }
      } else {
        current += ch;
      }
    } else if (ch === '"') {
      inQuote = true;
    } else if (ch === delim) {
      fields.push(current.trim());
      current = '';
    } else {
      current += ch;
    }
  }
  fields.push(current.trim());
  return fields;
}

const MAX_ROWS = 1000;

function parseCsv(text: string): ParsedCsv {
  const lines = text.split(/\r?\n/).filter((l) => l.trim() !== '');
  if (lines.length === 0) return { headers: [], rows: [], truncated: false };

  const delim = detectDelimiter(lines[0]);
  const headers = splitRow(lines[0], delim);
  const body = lines.slice(1);
  const truncated = body.length > MAX_ROWS;
  const rows = (truncated ? body.slice(0, MAX_ROWS) : body).map((line) =>
    splitRow(line, delim),
  );

  return { headers, rows, truncated };
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

// ---- component -------------------------------------------------------------

export function CsvRenderer({ state }: RendererProps) {
  const { content, fileName, size } = state;

  const parsed = useMemo<ParsedCsv>(() => {
    if (!content) return { headers: [], rows: [], truncated: false };
    return parseCsv(content);
  }, [content]);

  if (!content || parsed.headers.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-t-text-muted">
        {content ? '无法解析 CSV 数据' : '空文件'}
      </div>
    );
  }

  const colCount = parsed.headers.length;

  return (
    <div className="h-full overflow-auto p-5">
      <div className="overflow-x-auto rounded-lg border border-card-border">
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="bg-canvas-alt/50">
              {parsed.headers.map((h, i) => (
                <th
                  key={i}
                  className="border border-divider px-3 py-2 text-left font-semibold text-t-text whitespace-nowrap"
                >
                  {escapeHtml(h || `列${i + 1}`)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {parsed.rows.map((row, ri) => (
              <tr key={ri} className="even:bg-canvas-alt/20">
                {Array.from({ length: colCount }, (_, ci) => (
                  <td
                    key={ci}
                    className="border border-divider px-3 py-1.5 text-t-text whitespace-nowrap"
                  >
                    {ci < row.length ? escapeHtml(row[ci]) : ''}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {(parsed.truncated || size != null) && (
        <p className="mt-2 text-xs text-t-text-muted">
          {parsed.truncated
            ? `仅显示前 ${MAX_ROWS} 行。`
            : `${parsed.rows.length} 行 × ${colCount} 列`}
          {fileName ? `（${fileName}）` : ''}
          {size != null ? ` · ${(size / 1024).toFixed(1)} KB` : ''}
        </p>
      )}
    </div>
  );
}
