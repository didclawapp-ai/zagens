import { beforeEach, describe, expect, it } from 'vitest';
import {
  extractPathsFromEditToolOutput,
  isBrowserPreviewRelevantPath,
  resetPostEditPreviewHintCooldownForTests,
  shouldShowPostEditPreviewHint,
} from './postEditPreviewHint';

beforeEach(() => {
  resetPostEditPreviewHintCooldownForTests();
});

describe('isBrowserPreviewRelevantPath', () => {
  it('accepts web entrypoints and preview config', () => {
    expect(isBrowserPreviewRelevantPath('client/index.html')).toBe(true);
    expect(isBrowserPreviewRelevantPath('styles/app.css')).toBe(true);
    expect(isBrowserPreviewRelevantPath('.zagens/preview.json')).toBe(true);
  });

  it('rejects source-only paths', () => {
    expect(isBrowserPreviewRelevantPath('src/main.ts')).toBe(false);
    expect(isBrowserPreviewRelevantPath('package.json')).toBe(false);
  });
});

describe('extractPathsFromEditToolOutput', () => {
  it('parses write summaries and evidence facts', () => {
    const output = `- fact: path=client/index.html
Created 120 bytes (8 lines) to client/index.html`;
    expect(extractPathsFromEditToolOutput(output)).toEqual(['client/index.html']);
  });
});

describe('shouldShowPostEditPreviewHint', () => {
  it('shows only for preview-relevant write_file paths', () => {
    expect(
      shouldShowPostEditPreviewHint(
        'write_file',
        '- fact: path=client/index.html\nCreated 1 bytes (1 lines) to client/index.html',
      ),
    ).toBe(true);
    resetPostEditPreviewHintCooldownForTests();
    expect(
      shouldShowPostEditPreviewHint(
        'write_file',
        '- fact: path=package.json\nCreated 1 bytes (1 lines) to package.json',
      ),
    ).toBe(false);
  });

  it('does not show when path cannot be parsed', () => {
    expect(shouldShowPostEditPreviewHint('write_file', 'ok')).toBe(false);
  });

  it('ignores non-edit tools', () => {
    expect(
      shouldShowPostEditPreviewHint(
        'read_file',
        'Created 1 bytes (1 lines) to client/index.html',
      ),
    ).toBe(false);
  });

  it('cools down after one offer during a write burst', () => {
    const html = 'Created 1 bytes (1 lines) to client/index.html';
    expect(shouldShowPostEditPreviewHint('write_file', html, { now: 1_000 })).toBe(
      true,
    );
    expect(
      shouldShowPostEditPreviewHint('write_file', html, { now: 1_000 + 60_000 }),
    ).toBe(false);
    expect(
      shouldShowPostEditPreviewHint('write_file', html, {
        now: 1_000 + 5 * 60 * 1000,
      }),
    ).toBe(true);
  });
});
