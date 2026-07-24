import { describe, expect, it } from 'vitest';
import {
  extractPathsFromEditToolOutput,
  extractUnifiedDiff,
  resolveEditToolPath,
  statsFromEditToolOutput,
} from './diffEntries';
import { summarizeSessionFileChanges } from './sessionFileChanges';

describe('resolveEditToolPath', () => {
  it('reads path from write summary when input is missing', () => {
    expect(
      resolveEditToolPath({
        name: 'write_file',
        input: '',
        output: 'Created 561 bytes (20 lines) to server/main.go\n[diff omitted — large file]',
      }),
    ).toBe('server/main.go');
  });

  it('rejects outputs with no resolvable path', () => {
    expect(
      resolveEditToolPath({
        name: 'write_file',
        input: '',
        output: '(no textual changes)',
      }),
    ).toBeNull();
  });
});

describe('extractUnifiedDiff', () => {
  it('ignores large-file preview blocks', () => {
    const output =
      'Created 10 bytes (1 lines) to app.ts\n[diff omitted — large file]\n+preview line\n+preview line';
    expect(extractUnifiedDiff(output)).toBeNull();
  });
});

describe('statsFromEditToolOutput', () => {
  it('uses line count from summary when diff is omitted', () => {
    expect(
      statsFromEditToolOutput(
        'Created 561 bytes (20 lines) to server/main.go\n[diff omitted — large file]\n+foo',
      ),
    ).toEqual({ added: 20, removed: 0 });
  });
});

describe('summarizeSessionFileChanges', () => {
  it('dedupes by path and keeps latest stats', () => {
    const rows = summarizeSessionFileChanges([
      {
        id: 'a1',
        tools: [
          {
            id: 't1',
            name: 'write_file',
            input: '{"path":"client/index.html"}',
            output: '--- a/client/index.html\n+++ b/client/index.html\n@@\n+line1\n',
            status: 'done',
          },
          {
            id: 't2',
            name: 'write_file',
            input: '{"path":"client/index.html"}',
            output: '--- a/client/index.html\n+++ b/client/index.html\n@@\n+line1\n+line2\n',
            status: 'done',
          },
        ],
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.path).toBe('client/index.html');
    expect(rows[0]?.added).toBe(2);
  });

  it('includes large completed writes without unified diff', () => {
    const rows = summarizeSessionFileChanges([
      {
        id: 'a1',
        tools: [
          {
            id: 't1',
            name: 'write_file',
            input: '',
            output:
              'Created 1200 bytes (45 lines) to client/src/app.ts\n[diff omitted — large file; showing head preview]\n+preview',
            status: 'done',
          },
          {
            id: 't2',
            name: 'write_file',
            input: '',
            output: 'Created 80 bytes (3 lines) to package.json',
            status: 'done',
          },
        ],
      },
    ]);
    expect(rows).toHaveLength(2);
    expect(rows.map((r) => r.path)).toEqual(['client/src/app.ts', 'package.json']);
    expect(rows[0]?.added).toBe(45);
    expect(rows[1]?.added).toBe(3);
  });

  it('includes running edits before diff output arrives', () => {
    const rows = summarizeSessionFileChanges([
      {
        id: 'a1',
        tools: [
          {
            id: 't1',
            name: 'write_file',
            input: '{"path":"server/main.go"}',
            output: '',
            status: 'running',
          },
        ],
      },
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0]?.fileName).toBe('main.go');
    expect(rows[0]?.status).toBe('running');
  });

  it('parses evidence fact paths from output', () => {
    expect(
      extractPathsFromEditToolOutput(
        '- fact: path=client/index.html\nCreated 1 bytes (1 lines) to client/index.html',
      ),
    ).toEqual(['client/index.html']);
  });
});
