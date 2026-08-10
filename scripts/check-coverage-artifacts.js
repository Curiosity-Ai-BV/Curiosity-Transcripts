#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const { fileURLToPath } = require("node:url");

const repoRoot = path.resolve(__dirname, "..");
const coverageRoot = path.join(repoRoot, "release-artifacts", "coverage");
const scriptLabel = "scripts/check-coverage-artifacts.js";
const daLinePattern = /^DA:(\d+),(\d+)(?:,[^,\s]+)?$/;
const fnLinePattern = /^FN:(\d+),(.+)$/;
const fndaLinePattern = /^FNDA:(\d+),(.+)$/;

const desktopComponentSeamRequiredPaths = [
  {
    expected: "apps/desktop/src/desktopRecordingControls.tsx",
    alternatives: ["src/desktopRecordingControls.tsx"],
    requiredFunctions: ["RecordingControls"],
  },
  {
    expected: "apps/desktop/src/desktopMeetingDetailHeader.tsx",
    alternatives: ["src/desktopMeetingDetailHeader.tsx"],
    requiredFunctions: ["MeetingDetailHeader"],
  },
  {
    expected: "apps/desktop/src/desktopMeetingPrivacyRow.tsx",
    alternatives: ["src/desktopMeetingPrivacyRow.tsx"],
    requiredFunctions: ["MeetingPrivacyRow"],
  },
  {
    expected: "apps/desktop/src/desktopMeetingSummarySection.tsx",
    alternatives: ["src/desktopMeetingSummarySection.tsx"],
    requiredFunctions: ["MeetingSummarySection"],
  },
  {
    expected: "apps/desktop/src/desktopMeetingDetailActions.tsx",
    alternatives: ["src/desktopMeetingDetailActions.tsx"],
    requiredFunctions: ["MeetingDetailActions"],
  },
  {
    expected: "apps/desktop/src/desktopMeetingTranscriptSection.tsx",
    alternatives: ["src/desktopMeetingTranscriptSection.tsx"],
    requiredFunctions: ["MeetingTranscriptSection"],
  },
  {
    expected: "apps/desktop/src/desktopCommandOutcomes.tsx",
    alternatives: ["src/desktopCommandOutcomes.tsx"],
    requiredFunctions: ["DesktopCommandOutcomes"],
  },
  {
    expected: "apps/desktop/src/desktopSettingsEngineStack.tsx",
    alternatives: ["src/desktopSettingsEngineStack.tsx"],
    requiredFunctions: ["DesktopSettingsEngineStack"],
  },
  {
    expected: "apps/desktop/src/desktopModelReadiness.tsx",
    alternatives: ["src/desktopModelReadiness.tsx"],
    requiredFunctions: ["DesktopModelReadiness"],
  },
  {
    expected: "apps/desktop/src/desktopModelSetupOptions.tsx",
    alternatives: ["src/desktopModelSetupOptions.tsx"],
    requiredFunctions: ["DesktopModelSetupOptions"],
  },
  {
    expected: "apps/desktop/src/desktopCalendarContext.tsx",
    alternatives: ["src/desktopCalendarContext.tsx"],
    requiredFunctions: ["DesktopCalendarContext"],
  },
  {
    expected: "apps/desktop/src/desktopSettingsFeedback.tsx",
    alternatives: ["src/desktopSettingsFeedback.tsx"],
    requiredFunctions: ["DesktopSettingsFeedback"],
  },
  {
    expected: "apps/desktop/src/desktopSettingsForm.tsx",
    alternatives: ["src/desktopSettingsForm.tsx"],
    requiredFunctions: ["DesktopSettingsForm"],
  },
  {
    expected: "apps/desktop/src/desktopTopbar.tsx",
    alternatives: ["src/desktopTopbar.tsx"],
    requiredFunctions: ["DesktopTopbar"],
  },
];

const artifacts = [
  {
    label: "frontend LCOV",
    file: path.join(coverageRoot, "frontend", "lcov.info"),
    requiredPaths: [
      {
        expected: "apps/desktop/src/App.tsx",
        alternatives: ["src/App.tsx"],
        requiredFunctions: [
          "deleteSelectedMeeting",
          "retryFailedDelete",
          "generateSelectedSummary",
          "testWhisperModelPath",
          "testOllamaConnection",
          "saveRawAudioRetentionPolicy",
        ],
      },
      {
        expected: "apps/desktop/src/commandAdapter.ts",
        alternatives: ["src/commandAdapter.ts"],
        requiredFunctions: [
          "snapshotCommand",
          "mapDeleteState",
          "mapRawAudioRetention",
          "mapLocalProcessingState",
          "mapAnalysisDisclosure",
          "mapCommandJobState",
        ],
      },
      {
        expected: "apps/desktop/src/desktopContract.ts",
        alternatives: ["src/desktopContract.ts"],
        requiredFunctions: [
          "assertDesktopSnapshotContract",
          "assertWhisperModelPathTestContract",
          "assertOllamaConnectionTestContract",
          "assertMeetingSearchResultsContract",
          "validateDeleteCommandState",
          "validateAnalysisDisclosureState",
          "validateCommandJobView",
        ],
      },
      ...desktopComponentSeamRequiredPaths,
    ],
  },
  {
    label: "Rust workspace LCOV",
    file: path.join(coverageRoot, "rust", "workspace.lcov"),
    requiredPaths: [
      {
        expected: "crates/store/src/lib.rs",
        alternatives: [],
        requiredLineSpans: [
          {
            label: "store delete meeting entrypoint",
            startAnchor:
              "    pub fn delete_meeting(&self, meeting_id: &str) -> StoreResult<DeleteReport> {",
            endAnchor: "    pub fn finalize_pending_delete_intents(",
          },
          {
            label: "store pending delete finalization",
            startAnchor: "    pub fn finalize_pending_delete_intents(",
            endAnchor: "    fn finalize_deleted_meeting_cleanup(",
          },
          {
            label: "store deleted meeting cleanup",
            startAnchor: "    fn finalize_deleted_meeting_cleanup(",
            endAnchor: "    pub fn meeting_deleted(&self, meeting_id: &str) -> StoreResult<bool> {",
          },
          {
            label: "store private row delete residual check",
            startAnchor:
              "    fn private_rows_remain_for_delete(&self, meeting_id: &str) -> StoreResult<bool> {",
            endAnchor:
              "    fn delete_private_meeting_rows(&self, meeting_id: &str) -> StoreResult<()> {",
          },
          {
            label: "store private meeting row deletion",
            startAnchor:
              "    fn delete_private_meeting_rows(&self, meeting_id: &str) -> StoreResult<()> {",
            endAnchor: "    fn private_manifest_exists(&self, meeting_id: &str) -> StoreResult<bool> {",
            positiveFunctionNameSubstring: "delete_private_meeting_rows",
          },
        ],
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
        requiredLineSpans: [
          {
            label: "desktop export command state",
            startAnchor: "fn export_meeting_command_state_for_app_root(",
            endAnchor: "#[cfg(test)]\nfn delete_meeting_for_app_root(",
          },
          {
            label: "desktop delete command state",
            startAnchor: "fn delete_meeting_command_state_for_app_root(",
            endAnchor: "fn generate_summary_for_app_root_with_cancellation(",
          },
          {
            label: "desktop raw audio retention policy view",
            startAnchor: "fn raw_audio_retention_policy_view(",
            endAnchor: "#[derive(Clone)]\nstruct LocalOllamaTextClient<T> {",
          },
        ],
      },
      {
        expected: "apps/desktop/src-tauri/src/command_outcomes.rs",
        alternatives: ["src/command_outcomes.rs"],
        requiredLineSpans: [
          {
            label: "desktop exported command state DTO",
            startAnchor:
              "impl ExportCommandState {\n    pub(crate) fn exported(",
            endAnchor:
              "    pub(crate) fn failed(meeting_id: &str, format: ExportFormat, message: String) -> Self {",
          },
          {
            label: "desktop deleted command state DTO",
            startAnchor:
              "impl DeleteCommandState {\n    pub(crate) fn deleted(",
            endAnchor:
              "    pub(crate) fn failed(meeting_id: &str, message: String) -> Self {",
          },
        ],
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
        coveredLines: new Set(),
        functions: new Set(),
        functionLines: new Map(),
        functionHits: new Map(),
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
        currentRecord.coveredLines.add(Number(match[1]));
      }
      continue;
    }

    if (currentRecord && line.startsWith("FN:")) {
      const match = line.match(fnLinePattern);
      if (match) {
        const lineNumber = Number(match[1]);
        const functionName = match[2];
        currentRecord.functions.add(functionName);
        currentRecord.functionLines.set(functionName, lineNumber);
      }
      continue;
    }

    if (currentRecord && line.startsWith("FNDA:")) {
      const match = line.match(fndaLinePattern);
      if (match) {
        currentRecord.functionHits.set(
          match[2],
          (currentRecord.functionHits.get(match[2]) ?? 0) + Number(match[1]),
        );
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

function lineNumberForIndex(sourceText, index) {
  return sourceText.slice(0, index).split(/\r?\n/).length;
}

function resolveSourceText(sourcePath, sourceTextByPath) {
  if (sourceTextByPath?.has(sourcePath)) {
    return sourceTextByPath.get(sourcePath);
  }

  const sourceFile = path.join(repoRoot, sourcePath);
  if (fs.existsSync(sourceFile)) {
    return fs.readFileSync(sourceFile, "utf8");
  }

  return null;
}

function resolveRequiredSpan(sourceText, requiredSpan) {
  const startIndex = sourceText.indexOf(requiredSpan.startAnchor);
  if (startIndex === -1) {
    return {
      error: `Missing source anchor for Rust coverage seam ${requiredSpan.label}: ${requiredSpan.startAnchor}`,
    };
  }

  const endSearchIndex = startIndex + requiredSpan.startAnchor.length;
  const endIndex = sourceText.indexOf(requiredSpan.endAnchor, endSearchIndex);
  if (endIndex === -1) {
    return {
      error: `Missing source anchor for Rust coverage seam ${requiredSpan.label}: ${requiredSpan.endAnchor}`,
    };
  }

  return {
    startLine: lineNumberForIndex(sourceText, startIndex),
    endLine: lineNumberForIndex(sourceText, endIndex) - 1,
  };
}

function validateRequiredFunctions(requiredPath, matchedRecords) {
  const errors = [];

  for (const functionName of requiredPath.requiredFunctions ?? []) {
    const hasFunctionRecord = matchedRecords.some((record) => record.functions.has(functionName));
    if (!hasFunctionRecord) {
      errors.push(`Missing function coverage record ${functionName} in ${requiredPath.expected}`);
      continue;
    }

    const hasPairedPositiveHit = matchedRecords.some(
      (record) => record.functions.has(functionName) && (record.functionHits.get(functionName) ?? 0) > 0,
    );
    if (!hasPairedPositiveHit) {
      errors.push(`Function coverage record ${functionName} has no positive FNDA hits in ${requiredPath.expected}`);
    }
  }

  return errors;
}

function hasPositiveFunctionSubstringHit(records, functionNameSubstring, span) {
  return records.some((record) => {
    for (const functionName of record.functions) {
      if (!functionName.includes(functionNameSubstring)) {
        continue;
      }
      const declarationLine = record.functionLines.get(functionName);
      if (!Number.isInteger(declarationLine)) {
        continue;
      }
      if (declarationLine < span.startLine || declarationLine > span.endLine) {
        continue;
      }
      if ((record.functionHits.get(functionName) ?? 0) > 0) {
        return true;
      }
    }
    return false;
  });
}

function validateRequiredLineSpans(requiredPath, matchedRecords, sourceTextByPath) {
  const errors = [];

  for (const requiredSpan of requiredPath.requiredLineSpans ?? []) {
    const sourceText = resolveSourceText(
      normalizeRelativePath(requiredPath.expected),
      sourceTextByPath,
    );
    if (sourceText === null) {
      errors.push(`Missing required source file ${requiredPath.expected} for Rust coverage seam ${requiredSpan.label}`);
      continue;
    }

    const resolvedSpan = resolveRequiredSpan(sourceText, requiredSpan);
    if (resolvedSpan.error) {
      errors.push(resolvedSpan.error);
      continue;
    }

    const covered = matchedRecords.some((record) => {
      for (const line of record.coveredLines) {
        if (line >= resolvedSpan.startLine && line <= resolvedSpan.endLine) {
          return true;
        }
      }
      return false;
    });

    if (!covered && requiredSpan.positiveFunctionNameSubstring) {
      const hasFunctionHit = hasPositiveFunctionSubstringHit(
        matchedRecords,
        requiredSpan.positiveFunctionNameSubstring,
        resolvedSpan,
      );
      if (hasFunctionHit) {
        continue;
      }
    }

    if (!covered) {
      const functionEvidence = requiredSpan.positiveFunctionNameSubstring
        ? ` and no positive FNDA hit for a function containing ${requiredSpan.positiveFunctionNameSubstring}`
        : "";
      errors.push(
        `Rust coverage seam ${requiredSpan.label} has no covered DA lines in ` +
          `${requiredPath.expected}:${resolvedSpan.startLine}-${resolvedSpan.endLine}${functionEvidence}`,
      );
    }
  }

  return errors;
}

function validateRequiredCoverage(artifact, records, sourceTextByPath) {
  const errors = [];

  for (const requiredPath of artifact.requiredPaths) {
    const match = sourceMatches(records, requiredPath.expected, requiredPath.alternatives);
    if (!match.found) {
      errors.push(`Missing coverage source path ${requiredPath.expected}`);
    } else if (!match.hasCoveredLine) {
      errors.push(
        `Coverage source path ${requiredPath.expected} has no covered line hits; ` +
          "expected at least one DA line with a positive hit count",
      );
    }
    if (match.found) {
      const candidates = new Set(
        [requiredPath.expected, ...requiredPath.alternatives].map(normalizeRelativePath),
      );
      const matchedRecords = records.filter((record) => candidates.has(record.source));
      errors.push(...validateRequiredFunctions(requiredPath, matchedRecords));
      errors.push(...validateRequiredLineSpans(requiredPath, matchedRecords, sourceTextByPath));
    }
  }

  return errors;
}

function validateLcovText(artifact, text, sourceTextByPath) {
  if (text.trim().length === 0) {
    return [`${artifact.label} artifact is empty`];
  }

  const records = parseLcovRecords(text);
  if (records.length === 0) {
    return [`${artifact.label} artifact has no LCOV SF source records`];
  }

  return validateRequiredCoverage(artifact, records, sourceTextByPath);
}

function runSelfTests() {
  const frontendArtifact = {
    label: "self-test frontend LCOV",
    requiredPaths: [
      {
        expected: "apps/desktop/src/App.tsx",
        alternatives: ["src/App.tsx"],
        requiredFunctions: ["deleteSelectedMeeting", "generateSelectedSummary"],
      },
      {
        expected: "apps/desktop/src/commandAdapter.ts",
        alternatives: ["src/commandAdapter.ts"],
        requiredFunctions: ["snapshotCommand", "mapDeleteState"],
      },
      {
        expected: "apps/desktop/src/desktopContract.ts",
        alternatives: ["src/desktopContract.ts"],
        requiredFunctions: ["assertDesktopSnapshotContract", "assertMeetingSearchResultsContract"],
      },
    ],
  };
  const frontendComponentArtifact = {
    label: "self-test frontend component LCOV",
    requiredPaths: desktopComponentSeamRequiredPaths,
  };
  const tauriArtifact = {
    label: "self-test Tauri LCOV",
    requiredPaths: [
      {
        expected: "apps/desktop/src-tauri/src/main.rs",
        alternatives: ["src/main.rs"],
        requiredLineSpans: [
          {
            label: "export command state",
            startAnchor: "fn export_meeting_command_state_for_app_root(",
            endAnchor: "fn delete_meeting_command_state_for_app_root(",
          },
        ],
      },
    ],
  };
  const storeArtifact = {
    label: "self-test store LCOV",
    requiredPaths: [
      {
        expected: "crates/store/src/lib.rs",
        alternatives: [],
        requiredLineSpans: [
          {
            label: "store private meeting row deletion",
            startAnchor: "    fn delete_private_meeting_rows(",
            endAnchor: "    fn private_manifest_exists(",
            positiveFunctionNameSubstring: "delete_private_meeting_rows",
          },
        ],
      },
    ],
  };

  function expectRejected(name, artifact, text, expectedMessage, sourceTextByPath) {
    const errors = validateLcovText(artifact, text, sourceTextByPath);
    if (!errors.some((error) => error.includes(expectedMessage))) {
      fail(scriptLabel, `Self-test did not reject ${name}`);
    }
  }

  function expectAccepted(name, artifact, text, sourceTextByPath) {
    const errors = validateLcovText(artifact, text, sourceTextByPath);
    if (errors.length > 0) {
      fail(scriptLabel, `Self-test rejected ${name}: ${errors.join("; ")}`);
    }
  }

  function lcov(records) {
    return records
      .flatMap((record) => [
        `SF:${record.source}`,
        ...(record.functions ?? []).map(([line, name]) => `FN:${line},${name}`),
        ...(record.functionHits ?? []).map(([hits, name]) => `FNDA:${hits},${name}`),
        ...(record.hits ?? []).map(([line, hits]) => `DA:${line},${hits}`),
        "end_of_record",
      ])
      .join("\n");
  }

  function positiveRecordsForRequiredPaths(requiredPaths, lineBase = 100) {
    return requiredPaths.map((requiredPath, index) => {
      const line = lineBase + index * 10;
      const requiredFunctions = requiredPath.requiredFunctions ?? [];

      return {
        source: requiredPath.expected,
        functions: requiredFunctions.map((functionName, functionIndex) => [
          line + functionIndex,
          functionName,
        ]),
        functionHits: requiredFunctions.map((functionName) => [1, functionName]),
        hits: [[line, 1]],
      };
    });
  }

  expectRejected(
    "missing required source",
    frontendArtifact,
    lcov([
      { source: "apps/desktop/src/Other.tsx", hits: [[1, 1]] },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
      { source: "apps/desktop/src/desktopContract.ts", hits: [[1, 1]] },
    ]),
    "Missing coverage source path apps/desktop/src/App.tsx",
  );
  expectRejected(
    "source with no DA records",
    frontendArtifact,
    lcov([
      { source: "apps/desktop/src/App.tsx" },
      { source: "apps/desktop/src/commandAdapter.ts", hits: [[1, 1]] },
      { source: "apps/desktop/src/desktopContract.ts", hits: [[1, 1]] },
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
      { source: "apps/desktop/src/desktopContract.ts", hits: [[1, 1]] },
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
      { source: "apps/desktop/src/desktopContract.ts", hits: [[1, 1]] },
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
      { source: "apps/desktop/src/desktopContract.ts", hits: [[1, 1]] },
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
      "SF:apps/desktop/src/desktopContract.ts",
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
      "SF:apps/desktop/src/desktopContract.ts",
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
      "SF:apps/desktop/src/desktopContract.ts",
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
        functions: [
          [10, "deleteSelectedMeeting"],
          [20, "generateSelectedSummary"],
        ],
        functionHits: [
          [1, "deleteSelectedMeeting"],
          [1, "generateSelectedSummary"],
        ],
        hits: [
          [1, 0],
          [2, 1],
        ],
      },
      {
        source: "apps/desktop/src/commandAdapter.ts",
        functions: [
          [10, "snapshotCommand"],
          [20, "mapDeleteState"],
        ],
        functionHits: [
          [1, "snapshotCommand"],
          [1, "mapDeleteState"],
        ],
        hits: [[7, 1]],
      },
      {
        source: "apps/desktop/src/desktopContract.ts",
        functions: [
          [10, "assertDesktopSnapshotContract"],
          [20, "assertMeetingSearchResultsContract"],
        ],
        functionHits: [
          [1, "assertDesktopSnapshotContract"],
          [1, "assertMeetingSearchResultsContract"],
        ],
        hits: [[8, 1]],
      },
    ]),
  );
  expectRejected(
    "missing frontend seam intent function",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        functions: [[10, "deleteSelectedMeeting"]],
        functionHits: [[1, "deleteSelectedMeeting"]],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/commandAdapter.ts",
        functions: [
          [10, "snapshotCommand"],
          [20, "mapDeleteState"],
        ],
        functionHits: [
          [1, "snapshotCommand"],
          [1, "mapDeleteState"],
        ],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/desktopContract.ts",
        functions: [
          [10, "assertDesktopSnapshotContract"],
          [20, "assertMeetingSearchResultsContract"],
        ],
        functionHits: [
          [1, "assertDesktopSnapshotContract"],
          [1, "assertMeetingSearchResultsContract"],
        ],
        hits: [[10, 1]],
      },
    ]),
    "Missing function coverage record generateSelectedSummary",
  );
  expectRejected(
    "zero-hit frontend seam intent function",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        functions: [
          [10, "deleteSelectedMeeting"],
          [20, "generateSelectedSummary"],
        ],
        functionHits: [
          [1, "deleteSelectedMeeting"],
          [0, "generateSelectedSummary"],
        ],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/commandAdapter.ts",
        functions: [
          [10, "snapshotCommand"],
          [20, "mapDeleteState"],
        ],
        functionHits: [
          [1, "snapshotCommand"],
          [1, "mapDeleteState"],
        ],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/desktopContract.ts",
        functions: [
          [10, "assertDesktopSnapshotContract"],
          [20, "assertMeetingSearchResultsContract"],
        ],
        functionHits: [
          [1, "assertDesktopSnapshotContract"],
          [1, "assertMeetingSearchResultsContract"],
        ],
        hits: [[10, 1]],
      },
    ]),
    "Function coverage record generateSelectedSummary has no positive FNDA hits",
  );
  expectRejected(
    "split frontend function declaration and hit across duplicate source records",
    frontendArtifact,
    lcov([
      {
        source: "apps/desktop/src/App.tsx",
        functions: [[10, "deleteSelectedMeeting"]],
        functionHits: [[1, "deleteSelectedMeeting"]],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/App.tsx",
        functions: [[20, "generateSelectedSummary"]],
        hits: [[20, 1]],
      },
      {
        source: "apps/desktop/src/App.tsx",
        functionHits: [[1, "generateSelectedSummary"]],
        hits: [[30, 1]],
      },
      {
        source: "apps/desktop/src/commandAdapter.ts",
        functions: [
          [10, "snapshotCommand"],
          [20, "mapDeleteState"],
        ],
        functionHits: [
          [1, "snapshotCommand"],
          [1, "mapDeleteState"],
        ],
        hits: [[10, 1]],
      },
      {
        source: "apps/desktop/src/desktopContract.ts",
        functions: [
          [10, "assertDesktopSnapshotContract"],
          [20, "assertMeetingSearchResultsContract"],
        ],
        functionHits: [
          [1, "assertDesktopSnapshotContract"],
          [1, "assertMeetingSearchResultsContract"],
        ],
        hits: [[10, 1]],
      },
    ]),
    "Function coverage record generateSelectedSummary has no positive FNDA hits",
  );
  expectAccepted(
    "frontend alternative source paths with positive hits",
    frontendArtifact,
    lcov([
      {
        source: "src/App.tsx",
        functions: [
          [10, "deleteSelectedMeeting"],
          [20, "generateSelectedSummary"],
        ],
        functionHits: [
          [1, "deleteSelectedMeeting"],
          [1, "generateSelectedSummary"],
        ],
        hits: [[1, 1]],
      },
      {
        source: "src/commandAdapter.ts",
        functions: [
          [10, "snapshotCommand"],
          [20, "mapDeleteState"],
        ],
        functionHits: [
          [1, "snapshotCommand"],
          [1, "mapDeleteState"],
        ],
        hits: [[1, 1]],
      },
      {
        source: "src/desktopContract.ts",
        functions: [
          [10, "assertDesktopSnapshotContract"],
          [20, "assertMeetingSearchResultsContract"],
        ],
        functionHits: [
          [1, "assertDesktopSnapshotContract"],
          [1, "assertMeetingSearchResultsContract"],
        ],
        hits: [[1, 1]],
      },
    ]),
  );
  expectRejected(
    "missing extracted component source",
    frontendComponentArtifact,
    lcov(
      positiveRecordsForRequiredPaths(
        desktopComponentSeamRequiredPaths.slice(1),
      ),
    ),
    "Missing coverage source path apps/desktop/src/desktopRecordingControls.tsx",
  );
  expectRejected(
    "missing command outcomes component source",
    frontendComponentArtifact,
    lcov(
      positiveRecordsForRequiredPaths(
        desktopComponentSeamRequiredPaths.filter(
          (requiredPath) =>
            requiredPath.expected !== "apps/desktop/src/desktopCommandOutcomes.tsx",
        ),
      ),
    ),
    "Missing coverage source path apps/desktop/src/desktopCommandOutcomes.tsx",
  );
  expectRejected(
    "missing topbar component source",
    frontendComponentArtifact,
    lcov(
      positiveRecordsForRequiredPaths(
        desktopComponentSeamRequiredPaths.filter(
          (requiredPath) =>
            requiredPath.expected !== "apps/desktop/src/desktopTopbar.tsx",
        ),
      ),
    ),
    "Missing coverage source path apps/desktop/src/desktopTopbar.tsx",
  );
  expectRejected(
    "missing extracted component function evidence",
    frontendComponentArtifact,
    lcov([
      {
        source: "apps/desktop/src/desktopRecordingControls.tsx",
        hits: [[100, 1]],
      },
      ...positiveRecordsForRequiredPaths(
        desktopComponentSeamRequiredPaths.slice(1),
        110,
      ),
    ]),
    "Missing function coverage record RecordingControls",
  );
  expectRejected(
    "zero-hit extracted component function evidence",
    frontendComponentArtifact,
    lcov([
      {
        source: "apps/desktop/src/desktopRecordingControls.tsx",
        functions: [[100, "RecordingControls"]],
        functionHits: [[0, "RecordingControls"]],
        hits: [[100, 1]],
      },
      ...positiveRecordsForRequiredPaths(
        desktopComponentSeamRequiredPaths.slice(1),
        110,
      ),
    ]),
    "Function coverage record RecordingControls has no positive FNDA hits",
  );
  expectAccepted(
    "extracted component sources with positive function hits",
    frontendComponentArtifact,
    lcov(positiveRecordsForRequiredPaths(desktopComponentSeamRequiredPaths)),
  );
  expectAccepted(
    "Tauri alternative source path with a positive hit",
    tauriArtifact,
    lcov([{ source: "src/main.rs", hits: [[1, 1]] }]),
    new Map([
      [
        "apps/desktop/src-tauri/src/main.rs",
        [
          "fn export_meeting_command_state_for_app_root(",
          "  Ok(export_state)",
          "fn delete_meeting_command_state_for_app_root(",
        ].join("\n"),
      ],
    ]),
  );
  expectRejected(
    "missing Rust seam anchor",
    tauriArtifact,
    lcov([{ source: "apps/desktop/src-tauri/src/main.rs", hits: [[1, 1]] }]),
    "Missing source anchor for Rust coverage seam export command state",
    new Map([["apps/desktop/src-tauri/src/main.rs", "fn unrelated() {}"]]),
  );
  expectRejected(
    "zero-hit Rust seam span",
    tauriArtifact,
    lcov([{ source: "apps/desktop/src-tauri/src/main.rs", hits: [[99, 1]] }]),
    "Rust coverage seam export command state has no covered DA lines",
    new Map([
      [
        "apps/desktop/src-tauri/src/main.rs",
        [
          "fn export_meeting_command_state_for_app_root(",
          "  Ok(export_state)",
          "fn delete_meeting_command_state_for_app_root(",
        ].join("\n"),
      ],
    ]),
  );
  expectRejected(
    "covered Rust line after seam span",
    storeArtifact,
    lcov([{ source: "crates/store/src/lib.rs", hits: [[6, 1]] }]),
    "Rust coverage seam store private meeting row deletion has no covered DA lines",
    new Map([
      [
        "crates/store/src/lib.rs",
        [
          "    fn delete_private_meeting_rows(",
          "        Ok(())",
          "    }",
          "",
          "    fn private_manifest_exists(",
          "        Ok(true)",
        ].join("\n"),
      ],
    ]),
  );
  expectRejected(
    "covered Rust later helper function does not satisfy earlier seam",
    storeArtifact,
    lcov([
      {
        source: "crates/store/src/lib.rs",
        functions: [[5, "mangled_private_manifest_exists"]],
        functionHits: [[3, "mangled_private_manifest_exists"]],
        hits: [[6, 1]],
      },
    ]),
    "Rust coverage seam store private meeting row deletion has no covered DA lines",
    new Map([
      [
        "crates/store/src/lib.rs",
        [
          "    fn delete_private_meeting_rows(",
          "        Ok(())",
          "    }",
          "",
          "    fn private_manifest_exists(",
          "        Ok(true)",
        ].join("\n"),
      ],
    ]),
  );
  expectRejected(
    "Rust substring function declared after seam span",
    storeArtifact,
    lcov([
      {
        source: "crates/store/src/lib.rs",
        functions: [[7, "unrelated_delete_private_meeting_rows_test"]],
        functionHits: [[2, "unrelated_delete_private_meeting_rows_test"]],
        hits: [[7, 1]],
      },
    ]),
    "Rust coverage seam store private meeting row deletion has no covered DA lines",
    new Map([
      [
        "crates/store/src/lib.rs",
        [
          "    fn delete_private_meeting_rows(",
          "        Ok(())",
          "    }",
          "",
          "    fn private_manifest_exists(",
          "        Ok(true)",
          "    fn unrelated_delete_private_meeting_rows_test(",
        ].join("\n"),
      ],
    ]),
  );
  expectAccepted(
    "Rust seam function substring FNDA fallback",
    storeArtifact,
    lcov([
      {
        source: "crates/store/src/lib.rs",
        functions: [[1, "mangled_delete_private_meeting_rows"]],
        functionHits: [[2, "mangled_delete_private_meeting_rows"]],
        hits: [[6, 1]],
      },
    ]),
    new Map([
      [
        "crates/store/src/lib.rs",
        [
          "    fn delete_private_meeting_rows(",
          "        Ok(())",
          "    }",
          "",
          "    fn private_manifest_exists(",
          "        Ok(true)",
        ].join("\n"),
      ],
    ]),
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
      "Checks required LCOV source records, named frontend FNDA hits, and anchored Rust DA span hits.",
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

console.log("Coverage artifacts include the expected critical seam-intent evidence.");
