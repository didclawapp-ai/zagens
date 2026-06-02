#!/usr/bin/env node
/**
 * Sync download URLs and Tauri updater manifest from a GitHub Release.
 *
 * Usage:
 *   node scripts/sync-download-manifest.mjs
 *   GITHUB_REPO=owner/repo RELEASE_TAG=zagens-v0.6.0-preview.1 node scripts/sync-download-manifest.mjs
 *
 * Requires `gh` CLI when fetching live release assets (optional — falls back to URL templates).
 */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const websiteRoot = join(__dirname, '..');
const releasePath = join(websiteRoot, 'src', 'data', 'release.json');
const latestJsonPath = join(websiteRoot, 'public', 'download', 'latest.json');

function semverFromTag(tag) {
  const m = tag.match(/v?(0\.\d+\.\d+(?:-[\w.]+)?)/i);
  return m ? m[1] : tag.replace(/^zagens-/i, '');
}

function assetUrl(repo, tag, filename) {
  return `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(filename)}`;
}

function hasGh() {
  try {
    execFileSync(process.platform === 'win32' ? 'where' : 'which', ['gh'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function fetchReleaseJson(repo, tag) {
  const out = execFileSync('gh', ['release', 'view', tag, '--repo', repo, '--json', 'assets,publishedAt,url'], {
    encoding: 'utf8',
  });
  return JSON.parse(out);
}

async function sha256Remote(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetch failed ${url}: ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  return createHash('sha256').update(buf).digest('hex');
}

async function sha256File(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    createReadStream(path)
      .on('error', reject)
      .on('data', (c) => hash.update(c))
      .on('end', () => resolve(hash.digest('hex')));
  });
}

function pickAsset(assets, pattern) {
  return assets.find((a) => pattern.test(a.name));
}

async function main() {
  const existing = JSON.parse(await readFile(releasePath, 'utf8'));
  const repo = process.env.GITHUB_REPO ?? existing.githubRepo ?? 'zagens-desktop/zagens';
  const tag = process.env.RELEASE_TAG ?? existing.githubReleaseTag ?? 'zagens-v0.6.0-preview.1';
  const version = semverFromTag(tag);

  const zipName =
    process.env.ZIP_FILENAME ?? existing.platforms?.['windows-x64']?.zip?.filename ??
    `Zagens_${version}_x64-setup.exe.zip`;
  const exeName =
    process.env.EXE_FILENAME ?? existing.platforms?.['windows-x64']?.exe?.filename ??
    `Zagens_${version}_x64-setup.exe`;

  let publishedAt = existing.publishedAt ?? new Date().toISOString().slice(0, 10);
  let zipUrl = assetUrl(repo, tag, zipName);
  let exeUrl = assetUrl(repo, tag, exeName);
  let zipSha = existing.platforms?.['windows-x64']?.zip?.sha256 ?? '';
  let exeSha = existing.platforms?.['windows-x64']?.exe?.sha256 ?? '';

  if (hasGh()) {
    try {
      const rel = fetchReleaseJson(repo, tag);
      publishedAt = rel.publishedAt?.slice(0, 10) ?? publishedAt;
      const zipAsset = pickAsset(rel.assets, /\.zip$/i);
      const exeAsset = pickAsset(rel.assets, /setup\.exe$/i);
      if (zipAsset) {
        zipUrl = zipAsset.url;
        zipSha = zipSha || (await sha256Remote(zipAsset.url));
      }
      if (exeAsset) {
        exeUrl = exeAsset.url;
        exeSha = exeSha || (await sha256Remote(exeAsset.url));
      }
      console.log(`[sync] GitHub release ${tag} (${rel.assets.length} assets)`);
    } catch (err) {
      console.warn(`[sync] gh release view failed — using template URLs: ${err.message}`);
    }
  } else {
    console.warn('[sync] gh CLI not found — writing template URLs only');
  }

  const release = {
    version,
    publishedAt,
    githubRepo: repo,
    githubReleaseTag: tag,
    platforms: {
      'windows-x64': {
        zip: { filename: zipName, url: zipUrl, sha256: zipSha },
        exe: { filename: exeName, url: exeUrl, sha256: exeSha },
      },
    },
    notes: `Synced ${new Date().toISOString()}`,
  };

  await writeFile(releasePath, `${JSON.stringify(release, null, 2)}\n`);

  const latest = {
    version,
    notes: `Zagens ${version} preview`,
    pub_date: new Date().toISOString(),
    platforms: {
      'windows-x86_64': {
        signature: '',
        url: zipUrl || exeUrl,
      },
    },
  };

  await writeFile(latestJsonPath, `${JSON.stringify(latest, null, 2)}\n`);
  console.log(`[sync] wrote ${releasePath}`);
  console.log(`[sync] wrote ${latestJsonPath}`);
  if (!latest.platforms['windows-x86_64'].signature) {
    console.warn('[sync] latest.json signature empty — Tauri updater disabled until pubkey is configured');
  }
}

main().catch((err) => {
  console.error(`[sync] failed: ${err.message}`);
  process.exit(1);
});
