#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const { fileURLToPath } = require("node:url");

const repoRoot = path.resolve(__dirname, "..");
const coverageRoot = path.join(repoRoot, "release-artifacts", "coverage");
const scriptLabel = "scripts/check-coverage-artifacts.js";
const daLinePattern = /^DA:(\d+),(\d+)(?:,[^,\s]+)?$/;

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

function parseLcovRecords(text) {
  const records = [];
  let currentRecord = null;

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();

    if (line.startsWith("SF:")) {
      currentRecord = {
        source: normalizeSourcePath(line.slice(3)),
        hasCoveredLine: false,
      };
      records.push(currentRecord);
      continue;
    }

    if (line === "end_of_record") {
      currentRecord = null;
      continue;
    }

    if (currentRecord && line.startsWith("DA:")) {
      const match = line.match(daLinePattern);
      if (match && Number(match[2]) > 0) {
        currentRecord.hasCoveredLine = true;
      }
    }
  }

  return records;
}

function readLcovSources(artifact) {
  if (!fs.existsSync(artifact.file)) {
    throw new Error(`Missing ${artifact.label} artifact at ${path.relative(repoRoot, artifact.file)}`);
  }

  const text = fs.readFileSync(artifact.file, "utf8");
  if (text.trim().length === 0) {
    throw new Error(`${artifact.label} artifact is empty at ${path.relative(repoRoot, artifact.file)}`);
  }

  const records = parseLcovRecords(text);

  if (records.length === 0) {
    throw new Error(`${artifact.label} artifact has no LCOV SF source records`);
  }

  return records;
}

function sourceMatches(records, expected, alternatives) {
  const candidates = new Set([expected, ...alternatives].map(normalizeRelativePath));
  const matchedRecords = records.filter((record) => candidates.has(record.source));

  return {
    found: matchedRecords.length > 0,
    hasCoveredLine: matchedRecords.length > 0 && matchedRecords.every((record) => record.hasCoveredLine),
  };
}

function validateRequiredCoverage(artifact, records) {
  const errors = [];

  for (const requiredPath of artifact.requiredPaths) {
    const match = sourceMatches(records, requiredPath.expected, requiredPath.alternatives);
    if (!match.found) {
      errors.push(`Missing coverage source path ${requiredPath.expected}`);
    } else if (!match.hasCoveredLine) {
      errors.push(
        `Coverage source path ${requiredPath.expected} has no covered line hits; expected at least one DA line with a positive hit count`,
      );
    }
  }

  return errors;
}

function validateLcovText(artifact, text) {
  if (text.trim().length === 0) {
    return [`${artifact.label} artifact is empty`];
  }

  const records = parseLcovRecords(text);
  if (records.length === 0) {
    return [`${artifact.label} artifact has no LCOV SF source records`];
  }

  return validateRequiredCoverage(artifact, records);
}

function runSelfTests() {
  const frontendArtifact = {
    label: "self-test frontend LCOV",
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
  };
  const tauriArtifact = {
    label: "self-test Tauri LCOV",
    requiredPaths: [
      {
        expected: "apps/desktop/src-tauri/src/main.rs",
        alternatives: ["src/main.rs"],
      },
    ],
  };

  function expectRejected(name, artifact, text, expectedMessage) {
    const errors = validateLcovText(artifact, text);
    if (!errors.some((error) => error.includes(expectedMessage))) {
      fail(scriptLabel, `Self-test did not reject ${name}`);
    }
  }

  function expectAccepted(name, artifact, text) {
    const errors = validateLcovText(artifact, text);
    if (errors.length > 0) {
      fail(scriptLabel, `Self-test rejected ${name}: ${errors.join("; ")}`);
    }
  }

  function lcov(records) {
    return records
      .flatMap((record) => [
        `SF:${record.source}`,
        ...(record.hits ?? []).map(([line, hits]) => `DA:${line},${hits}`),
        "end_of_record",
      ])
      .join("\n");
  }

  expectRejected(
    "missing required source",
    frontendArtifact,
    lcov([
      { source: "apps/desktop/src/Other.tsx", hits: [[1, 1]] },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
    "Missing coverage source path apps/desktop/src/App.tsx",
  );
  expectRejected(
    "source with no DA records",
    frontendArtifact,
    lcov([
      { source: "apps/desktop/src/App.tsx" },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
    "has no covered line hits",
  );
  expectRejected(
    "source with all-zero DA records",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        hits: [
          [1, 0],
          [2, 0],
        ],
      },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
    "has no covered line hits",
  );
  expectRejected(
    "positive DA hit in a different source record",
    frontendArtifact,
    lcov([
      { source: "apps/desktop/src/App.tsx" },
      { source: "apps/desktop/src/Other.tsx", hits: [[1, 1]] },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
    "has no covered line hits",
  );
  expectRejected(
    "duplicate required source with one all-zero record",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        hits: [[1, 0]],
      },
      {
        source: "apps/desktop/src/App.tsx",
        hits: [[2, 1]],
      },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
    "has no covered line hits",
  );
  expectRejected(
    "required source with malformed DA line number",
    frontendArtifact,
    [
      "SF:apps/desktop/src/App.tsx",
      "DA:not-a-line,1",
      "end_of_record",
      "SF:apps/desktop/src/commandAdapter.ts",
      "DA:1,1",
      "end_of_record",
    ].join("\n"),
    "has no covered line hits",
  );
  expectRejected(
    "required source with missing DA line number",
    frontendArtifact,
    [
      "SF:apps/desktop/src/App.tsx",
      "DA:,1",
      "end_of_record",
      "SF:apps/desktop/src/commandAdapter.ts",
      "DA:1,1",
      "end_of_record",
    ].join("\n"),
    "has no covered line hits",
  );
  expectRejected(
    "required source with fractional DA hit count",
    frontendArtifact,
    [
      "SF:apps/desktop/src/App.tsx",
      "DA:1,0.5",
      "end_of_record",
      "SF:apps/desktop/src/commandAdapter.ts",
      "DA:1,1",
      "end_of_record",
    ].join("\n"),
    "has no covered line hits",
  );
  expectAccepted(
    "required frontend sources with positive hits",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        hits: [
          [1, 0],
          [2, 1],
        ],
      },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[7, 1]] },
    ]),
  );
  expectAccepted(
    "frontend alternative source paths with positive hits",
    frontendArtifact,
    lcov([
      { source: "src/App.tsx", hits: [[1, 1]] },
      { source: "src/commandAdapter.ts", hits: [[1, 1]] },
    ]),
  );
  expectAccepted(
    "Tauri alternative source path with a positive hit",
    tauriArtifact,
    lcov([{ source: "src/main.rs", hits: [[1, 1]] }]),
  );
}

function parseArgs(argv) {
  const options = {
    selfTest: false,
    help: false,
  };

  for (const arg of argv) {
    if (arg === "--self-test") {
      options.selfTest = true;
    } else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else {
      fail(scriptLabel, `Unexpected argument: ${arg}`);
    }
  }

  return options;
}

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  console.log(
    [
      "Usage: node scripts/check-coverage-artifacts.js",
      "       node scripts/check-coverage-artifacts.js --self-test",
      "",
      "Checks required LCOV source records and requires each one to include at least one DA line with a positive hit count.",
    ].join("\n"),
  );
  process.exit(ok ? 0 : 1);
}

if (options.selfTest) {
  runSelfTests();
  if (!ok) {
    process.exit(1);
  }
  console.log("Coverage artifact checker self-tests passed.");
  process.exit(0);
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

  for (const error of validateRequiredCoverage(artifact, sources)) {
    fail(label, error);
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Coverage artifacts include the expected critical source paths with covered line hits.");
