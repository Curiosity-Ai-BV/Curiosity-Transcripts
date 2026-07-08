#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const { fileURLToPath } = require("node:url");

const repoRoot = path.resolve(__dirname, "..");
const coverageRoot = path.join(repoRoot, "release-artifacts", "coverage");

const artifacts = [
  {
    label: "frontend LCOV",
    file: path.join(coverageRoot, "frontend", "lcov.info"),
    requiredPaths: [
      {
        expected: "apps/desktop/src/App.tsx",
        alternatives: ["src/App.tsx"],
      },
      {
        expected: "apps/desktop/src/commandAdapter.ts",
        alternatives: ["src/commandAdapter.ts"],
      },
    ],
  },
  {
    label: "Rust workspace LCOV",
    file: path.join(coverageRoot, "rust", "workspace.lcov"),
    requiredPaths: [
      {
        expected: "crates/store/src/lib.rs",
        alternatives: [],
      },
    ],
  },
  {
    label: "desktop Tauri Rust LCOV",
    file: path.join(coverageRoot, "rust", "desktop-tauri.lcov"),
    requiredPaths: [
      {
        expected: "apps/desktop/src-tauri/src/main.rs",
        alternatives: ["src/main.rs"],
      },
    ],
  },
];

function toForwardSlashes(value) {
  return value.replace(/\\/g, "/");
}

function normalizeRelativePath(value) {
  return toForwardSlashes(value).replace(/^\.\//, "").replace(/\/+/g, "/");
}

function normalizeSourcePath(sourcePath) {
  let value = sourcePath.trim();

  if (value.startsWith("file://")) {
    value = fileURLToPath(value);
  }

  value = toForwardSlashes(path.normalize(value));

  if (path.isAbsolute(value)) {
    const relative = path.relative(repoRoot, value);
    if (!relative.startsWith("..") && !path.isAbsolute(relative)) {
      return normalizeRelativePath(relative);
    }
  }

  const rootPrefix = `${toForwardSlashes(repoRoot)}/`;
  if (value.startsWith(rootPrefix)) {
    return normalizeRelativePath(value.slice(rootPrefix.length));
  }

  return normalizeRelativePath(value);
}

function readLcovSources(artifact) {
  if (!fs.existsSync(artifact.file)) {
    throw new Error(`Missing ${artifact.label} artifact at ${path.relative(repoRoot, artifact.file)}`);
  }

  const text = fs.readFileSync(artifact.file, "utf8");
  if (text.trim().length === 0) {
    throw new Error(`${artifact.label} artifact is empty at ${path.relative(repoRoot, artifact.file)}`);
  }

  const sources = new Set();
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("SF:")) {
      sources.add(normalizeSourcePath(line.slice(3)));
    }
  }

  if (sources.size === 0) {
    throw new Error(`${artifact.label} artifact has no LCOV SF source records`);
  }

  return sources;
}

function sourceMatches(sources, expected, alternatives) {
  const candidates = new Set([expected, ...alternatives].map(normalizeRelativePath));

  for (const source of sources) {
    if (candidates.has(source)) {
      return true;
    }
  }

  return false;
}

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

for (const artifact of artifacts) {
  const label = path.relative(repoRoot, artifact.file);
  let sources;

  try {
    sources = readLcovSources(artifact);
  } catch (error) {
    fail(label, error.message);
    continue;
  }

  for (const requiredPath of artifact.requiredPaths) {
    if (!sourceMatches(sources, requiredPath.expected, requiredPath.alternatives)) {
      fail(label, `Missing coverage source path ${requiredPath.expected}`);
    }
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Coverage artifacts include the expected critical source paths.");
