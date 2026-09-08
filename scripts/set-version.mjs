#!/usr/bin/env node
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[0-9A-Za-z.-]+)?$/;

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));

function fail(message) {
  process.stderr.write(`set-version: ${message}\n`);
  process.exit(1);
}

const [version, ...extra] = process.argv.slice(2);

if (!version || extra.length > 0) {
  fail("usage: node scripts/set-version.mjs <MAJOR.MINOR.PATCH[-identifier]>");
}

if (!SEMVER.test(version)) {
  fail(
    `invalid version "${version}": expected MAJOR.MINOR.PATCH with an optional -identifier suffix, without a leading "v"`,
  );
}

function replaceJsonVersion(source, path) {
  const pattern = /^(\s*"version"\s*:\s*")[^"]*(")/m;
  if (!pattern.test(source)) {
    fail(`no top-level "version" field found in ${path}`);
  }
  return source.replace(pattern, `$1${version}$2`);
}

function replaceWorkspaceVersion(source, path) {
  const section = /^\[workspace\.package\]$/m;
  const header = source.match(section);
  if (!header) {
    fail(`no [workspace.package] section found in ${path}`);
  }
  const start = header.index + header[0].length;
  const rest = source.slice(start);
  const nextSection = rest.search(/^\[/m);
  const end = nextSection === -1 ? source.length : start + nextSection;
  const block = source.slice(start, end);
  const pattern = /^(version\s*=\s*")[^"]*(")/m;
  if (!pattern.test(block)) {
    fail(`no version key inside [workspace.package] in ${path}`);
  }
  return (
    source.slice(0, start) +
    block.replace(pattern, `$1${version}$2`) +
    source.slice(end)
  );
}

const targets = [
  { path: join("app", "package.json"), rewrite: replaceJsonVersion },
  {
    path: join("app", "src-tauri", "tauri.conf.json"),
    rewrite: replaceJsonVersion,
  },
  { path: join("app", "Cargo.toml"), rewrite: replaceWorkspaceVersion },
];

const pending = targets.map(({ path, rewrite }) => {
  const absolutePath = join(repoRoot, path);
  let source;
  try {
    source = readFileSync(absolutePath, "utf8");
  } catch (error) {
    fail(`cannot read ${path}: ${error.message}`);
  }
  return { path, absolutePath, source, next: rewrite(source, path) };
});

for (const { path, absolutePath, source, next } of pending) {
  if (source === next) continue;
  writeFileSync(absolutePath, next);
  process.stdout.write(`${path}\n`);
}
