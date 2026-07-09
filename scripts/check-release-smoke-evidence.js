#!/usr/bin/env node
const fs = require("node:fs");
const net = require("node:net");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "..");
const scriptLabel = "scripts/check-release-smoke-evidence.js";
const defaultEvidencePath = path.join(repoRoot, "docs", "release-candidate-smoke-evidence.template.json");

const expectedManualItemIds = [
  "clean-user-install",
  "macos-permissions",
  "model-setup",
  "offline-after-setup",
  "recording",
  "import-wav",
  "transcription",
  "durable-recovery",
  "correction",
  "summary",
  "privacy-data-state",
  "at-rest-disclosure",
  "export-json-markdown-srt",
  "contract-fixture",
  "automated-release-artifacts",
  "delete-cleanup",
  "uninstall-private-data",
];

const automatedReleaseArtifactItemId = "automated-release-artifacts";
const atRestDisclosureItemId = "at-rest-disclosure";
const completeAtRestDisclosureEvidenceText =
  "Release notes state app-level encryption-at-rest is not implemented in v1; app-private data relies on OS/user-account file protections; app delete controls app-private meeting data; user-owned source files and exported files can remain outside app deletion control.";
const atRestDisclosureRequirements = [
  {
    label: "app-level encryption-at-rest is not implemented in v1",
    patternGroups: [
      [/\bv1\b/],
      [/\b(?:app|application) level\b/],
      [/\bencryption at rest\b/],
      [
        /\b(?:app|application) level encryption at rest (?:is )?(?:not implemented|not yet implemented)\b/,
        /\b(?:does not implement|does not include) (?:app|application) level encryption at rest\b/,
        /\b(?:app|application) level encryption at rest .*?\b(?:does not exist|is unavailable)\b/,
      ],
    ],
  },
  {
    label: "app-private data relies on OS/user-account file protections",
    patternGroups: [
      [/\bapp private\b/],
      [/\b(?:data|storage)\b/],
      [/\b(?:os|operating system)\b/],
      [/\buser account\b/],
      [/\bfile protections?\b/],
      [/\b(?:relies|rely|relying|relied|protected by|backed by|uses)\b/],
    ],
  },
  {
    label: "app delete controls app-private meeting data",
    patternGroups: [
      [/\bapp private\b/],
      [/\bmeeting data\b/],
      [/\b(?:app delete|app deletion|delete controls?|deletion controls?|delete boundary|app control)\b/],
    ],
  },
  {
    label: "user-owned source files and exported files can remain outside app deletion control",
    patternGroups: [
      [/\buser owned\b/],
      [/\bsource files?\b/],
      [/\b(?:exported files?|export files?|exports?)\b/],
      [/\boutside\b/],
      [/\b(?:app delete|app deletion|delete control|deletion control|delete boundary|app control)\b/],
    ],
  },
];
const requiredAutomatedReleaseArtifactReferences = [
  "release-artifacts/supply-chain",
  "release-artifacts/coverage",
  "release-artifacts/contracts/desktop-command-view-contract.receipt.json",
];

const manualStatusValues = new Set(["pending", "passed", "failed", "skipped"]);
const overallResultValues = new Set(["pending", "passed", "failed", "incomplete"]);
const completionValues = new Set(["passed", "completed"]);
const semverCorePattern =
  "(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)(?:-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?";
const semverPattern = new RegExp(`^${semverCorePattern}$`);
const semverTagPattern = new RegExp(`^v${semverCorePattern}$`);
const gitShaPattern = /^[0-9a-f]{7,40}$/i;
const sha256Pattern = /^[0-9a-f]{64}$/i;
const iso8601TimestampWithTimezonePattern =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
const macosVersionPattern =
  /^(?:macOS(?:\s+[A-Za-z][A-Za-z0-9-]*){0,2}\s+)?\d{2,}(?:\.\d+){1,2}(?:\s*\([0-9A-Za-z]+\))?$/i;
const firstReleaseArchitectureValues = new Set(["arm64", "aarch64"]);
const weakEvidenceTextValues = new Set([
  "-",
  "done",
  "n/a",
  "na",
  "none",
  "ok",
  "okay",
  "pass",
  "passed",
  "pending",
  "tbd",
  "template",
  "todo",
  "unknown",
]);
const hostedOrCloudOllamaModelValues = new Set(
  [
    "deepseek-v3.2:cloud",
    "ollama-cloud-deepseek-v3-2",
    "DeepSeek V3.2 Cloud",
    "hosted-deepseek-v3-2-speciale",
    "DeepSeek-V3.2-Speciale",
    "DeepSeek V3.2 Speciale",
  ].map(normalizeOllamaModelName),
);
const criticalResultFields = [
  ["build", "signing", "status"],
  ["build", "signing", "codesignVerified"],
  ["build", "notarization", "status"],
  ["build", "notarization", "stapled"],
  ["build", "notarization", "gatekeeperVerified"],
  ["machine", "cleanUserAccount", "createdForSmoke"],
  ["machine", "cleanUserAccount", "developmentCheckoutAbsent"],
  ["modelSetup", "whisper", "status"],
  ["modelSetup", "ollama", "status"],
];

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function createFixture() {
  return {
    schemaVersion: "1.0",
    kind: "release-candidate-smoke-evidence",
    scope: "arm64 macOS DMG manual smoke evidence",
    isTemplate: true,
    overallResult: "pending",
    build: {
      version: "TEMPLATE_VERSION",
      gitRef: "TEMPLATE_GIT_REF",
      gitSha: "TEMPLATE_GIT_SHA",
      buildTimestamp: "TEMPLATE_BUILD_TIMESTAMP_ISO_8601",
      releaseArtifact: {
        path: "TEMPLATE_DMG_PATH",
        name: "Curiosity-Transcripts-TEMPLATE_VERSION-macos-aarch64.dmg",
        sha256: "TEMPLATE_DMG_SHA256",
      },
      signing: {
        status: "pending",
        developerIdApplication: "TEMPLATE_DEVELOPER_ID_APPLICATION",
        developerIdInstaller: "TEMPLATE_DEVELOPER_ID_INSTALLER_OR_NOT_APPLICABLE",
        codesignVerified: "pending",
      },
      notarization: {
        status: "pending",
        requestId: "TEMPLATE_NOTARIZATION_REQUEST_ID_OR_PENDING",
        stapled: "pending",
        gatekeeperVerified: "pending",
      },
    },
    machine: {
      machineLabel: "TEMPLATE_MACHINE_LABEL",
      architecture: "TEMPLATE_ARCHITECTURE",
      macosVersion: "TEMPLATE_MACOS_VERSION",
      cleanUserAccount: {
        accountLabel: "TEMPLATE_CLEAN_USER_ACCOUNT_LABEL",
        createdForSmoke: "pending",
        developmentCheckoutAbsent: "pending",
      },
    },
    modelSetup: {
      whisper: {
        modelPath: "TEMPLATE_WHISPER_MODEL_PATH_OR_SKIP_REASON",
        status: "pending",
        sha256: "TEMPLATE_WHISPER_MODEL_SHA256_OR_PENDING",
        fileSizeBytes: "TEMPLATE_WHISPER_MODEL_SIZE_OR_PENDING",
        modifiedAt: "TEMPLATE_WHISPER_MODEL_MODIFIED_AT_OR_PENDING",
        pathTestEvidence: "TEMPLATE_WHISPER_PATH_TEST_EVIDENCE_OR_PENDING",
        skipReason: "TEMPLATE_SKIP_REASON_IF_WHISPER_ABSENT",
      },
      ollama: {
        baseUrl: "TEMPLATE_OLLAMA_BASE_URL_OR_SKIP_REASON",
        model: "TEMPLATE_OLLAMA_MODEL_OR_SKIP_REASON",
        status: "pending",
        tagsEvidence: "TEMPLATE_OLLAMA_TAGS_EVIDENCE_OR_PENDING",
        skipReason: "TEMPLATE_SKIP_REASON_IF_OLLAMA_ABSENT",
      },
    },
    manualItems: expectedManualItemIds.map((id) => ({
      id,
      label: `Template label for ${id}`,
      status: "pending",
      evidence: {
        observations: [`TEMPLATE_EVIDENCE_FOR_${id}`],
        artifacts: [`TEMPLATE_ARTIFACT_FOR_${id}`],
        skipReason: "TEMPLATE_SKIP_REASON_IF_SKIPPED",
      },
      notes: "TEMPLATE_NOTES",
    })),
  };
}

function createStrictPassedFixture() {
  const evidence = createFixture();
  evidence.isTemplate = false;
  evidence.overallResult = "passed";
  evidence.build = {
    version: "1.2.3",
    gitRef: "refs/tags/v1.2.3",
    gitSha: "0123456789abcdef0123456789abcdef01234567",
    buildTimestamp: "2026-07-08T12:00:00Z",
    releaseArtifact: {
      path: "/tmp/Curiosity-Transcripts-1.2.3-macos-aarch64.dmg",
      name: "Curiosity-Transcripts-1.2.3-macos-aarch64.dmg",
      sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    },
    signing: {
      status: "passed",
      developerIdApplication: "Developer ID Application: Curiosity AI BV (TEAMID1234)",
      developerIdInstaller: "not applicable for DMG",
      codesignVerified: "passed",
    },
    notarization: {
      status: "completed",
      requestId: "notary-request-id-123",
      stapled: "passed",
      gatekeeperVerified: "passed",
    },
  };
  evidence.machine = {
    machineLabel: "release-smoke-macbook-pro",
    architecture: "arm64",
    macosVersion: "15.5",
    cleanUserAccount: {
      accountLabel: "release-smoke-user",
      createdForSmoke: "passed",
      developmentCheckoutAbsent: "passed",
    },
  };
  evidence.modelSetup = {
    whisper: {
      modelPath: "/Users/release-smoke/Models/ggml-base.en.bin",
      status: "passed",
      sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      fileSizeBytes: 147964211,
      modifiedAt: "2026-07-08T10:00:00Z",
      pathTestEvidence: "Settings Test path showed readable file size and matching SHA-256.",
      skipReason: "",
    },
    ollama: {
      baseUrl: "http://127.0.0.1:11434",
      model: "llama3.2:3b",
      status: "passed",
      tagsEvidence: "/api/tags reported llama3.2:3b before offline smoke.",
      skipReason: "",
    },
  };
  evidence.manualItems = evidence.manualItems.map((item) => {
    if (item.id === automatedReleaseArtifactItemId) {
      return {
        id: item.id,
        label: item.label.replace("Template label for ", ""),
        status: "passed",
        evidence: {
          observations: ["Confirmed CI-produced release artifact references before manual publish."],
          artifacts: [...requiredAutomatedReleaseArtifactReferences],
          skipReason: "",
        },
        notes: "Automated release artifacts are attached to the draft release evidence.",
      };
    }

    if (item.id === atRestDisclosureItemId) {
      return {
        id: item.id,
        label: item.label.replace("Template label for ", ""),
        status: "passed",
        evidence: {
          observations: [completeAtRestDisclosureEvidenceText],
          artifacts: ["release-smoke/at-rest-disclosure-release-notes.txt"],
          skipReason: "",
        },
        notes: completeAtRestDisclosureEvidenceText,
      };
    }

    return {
      id: item.id,
      label: item.label.replace("Template label for ", ""),
      status: "passed",
      evidence: {
        observations: [`Observed ${item.id} pass during clean macOS DMG smoke.`],
        artifacts: [`release-smoke/${item.id}.txt`],
        skipReason: "",
      },
      notes: `${item.id} passed with recorded evidence.`,
    };
  });
  return evidence;
}

function createAllowIncompleteDraftFixture() {
  const evidence = createStrictPassedFixture();
  evidence.overallResult = "incomplete";
  evidence.build.signing.status = "pending";
  evidence.build.signing.codesignVerified = "pending";
  evidence.build.notarization.status = "pending";
  evidence.build.notarization.stapled = "pending";
  evidence.build.notarization.gatekeeperVerified = "pending";
  evidence.machine.cleanUserAccount.createdForSmoke = "pending";
  evidence.machine.cleanUserAccount.developmentCheckoutAbsent = "pending";
  evidence.modelSetup.whisper.status = "pending";
  evidence.modelSetup.ollama.status = "pending";
  evidence.manualItems = evidence.manualItems.map((item) => ({
    ...item,
    status: "pending",
    evidence: {
      observations: [`Draft observation for ${item.id}.`],
      artifacts: [`draft-smoke/${item.id}.txt`],
      skipReason: "",
    },
    notes: `${item.id} is still pending in draft evidence.`,
  }));
  return evidence;
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function nonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function isPlaceholderText(value) {
  if (typeof value !== "string") {
    return false;
  }

  const trimmed = value.trim();
  return /^TEMPLATE$/i.test(trimmed) || /\bTEMPLATE_[A-Z0-9_]+\b/i.test(trimmed) || /^PENDING:/i.test(trimmed);
}

function meaningfulString(value) {
  if (!nonEmptyString(value) || isPlaceholderText(value)) {
    return false;
  }

  return !weakEvidenceTextValues.has(value.trim().toLowerCase());
}

function normalizeOllamaModelName(modelName) {
  return modelName
    .trim()
    .split("")
    .filter((char) => !/\s/.test(char))
    .join("")
    .toLowerCase();
}

function isLocalOllamaModelTag(value) {
  const normalized = normalizeOllamaModelName(value);
  return normalized.length > 0 && !normalized.endsWith(":cloud") && !hostedOrCloudOllamaModelValues.has(normalized);
}

function tagsEvidenceNamesOllamaModel(tagsEvidence, model) {
  const normalizedEvidence = normalizeOllamaModelName(tagsEvidence);
  const normalizedModel = normalizeOllamaModelName(model);
  return normalizedModel.length > 0 && normalizedEvidence.includes(normalizedModel);
}

function hasExplicitUrlUserinfo(value) {
  const authorityStart = value.indexOf("://");
  if (authorityStart === -1) {
    return false;
  }

  const authority = value.slice(authorityStart + 3).split(/[/?#]/, 1)[0];
  return authority.includes("@");
}

function hasExplicitUrlQueryOrFragment(value) {
  return value.includes("?") || value.includes("#");
}

function isLoopbackHostname(hostname) {
  if (hostname === "localhost") {
    return true;
  }

  const host = hostname.startsWith("[") && hostname.endsWith("]") ? hostname.slice(1, -1) : hostname;
  const ipVersion = net.isIP(host);
  if (ipVersion === 4) {
    return host.split(".")[0] === "127";
  }
  if (ipVersion === 6) {
    return host === "::1" || host === "0:0:0:0:0:0:0:1";
  }
  return false;
}

function isLocalOllamaBaseUrl(value) {
  if (!/^https?:\/\//i.test(value)) {
    return false;
  }

  let url;
  try {
    url = new URL(value);
  } catch {
    return false;
  }

  return (
    (url.protocol === "http:" || url.protocol === "https:") &&
    !hasExplicitUrlUserinfo(value) &&
    !hasExplicitUrlQueryOrFragment(value) &&
    url.username === "" &&
    url.password === "" &&
    url.search === "" &&
    url.hash === "" &&
    isLoopbackHostname(url.hostname.toLowerCase())
  );
}

function fieldLabel(pathParts) {
  return pathParts.join(".");
}

function valueAt(object, pathParts) {
  return pathParts.reduce((current, part) => current?.[part], object);
}

function hasPath(object, pathParts) {
  const key = pathParts[pathParts.length - 1];
  const parent = pathParts.slice(0, -1).reduce((current, part) => {
    if (!isPlainObject(current)) {
      return undefined;
    }
    return current[part];
  }, object);

  return isPlainObject(parent) && hasOwn(parent, key);
}

function requirePresent(errors, object, pathParts) {
  const key = pathParts[pathParts.length - 1];
  const parentPath = pathParts.slice(0, -1);
  const parent = parentPath.reduce((current, part) => {
    if (!isPlainObject(current)) {
      return undefined;
    }
    return current[part];
  }, object);

  if (!isPlainObject(parent) || !hasOwn(parent, key)) {
    errors.push(`Missing required field ${fieldLabel(pathParts)}`);
    return false;
  }

  return true;
}

function requireObject(errors, object, pathParts) {
  if (!requirePresent(errors, object, pathParts)) {
    return null;
  }

  const value = pathParts.reduce((current, part) => current?.[part], object);
  if (!isPlainObject(value)) {
    errors.push(`${fieldLabel(pathParts)} must be an object`);
    return null;
  }

  return value;
}

function requireNonEmptyString(errors, object, pathParts) {
  if (!requirePresent(errors, object, pathParts)) {
    return;
  }

  const value = valueAt(object, pathParts);
  if (!nonEmptyString(value)) {
    errors.push(`${fieldLabel(pathParts)} must be a non-empty string`);
  }
}

function requireFilledStringFormat(errors, object, pathParts, predicate, expectation) {
  if (!hasPath(object, pathParts)) {
    return "";
  }

  const label = fieldLabel(pathParts);
  const value = valueAt(object, pathParts);
  if (!nonEmptyString(value)) {
    errors.push(`${label} must be ${expectation}`);
    return "";
  }

  const trimmed = value.trim();
  if (!predicate(trimmed)) {
    errors.push(`${label} must be ${expectation}`);
  }
  return trimmed;
}

function hasMeaningfulEvidence(value) {
  if (meaningfulString(value)) {
    return true;
  }
  if (Array.isArray(value)) {
    return value.some((item) => hasMeaningfulEvidence(item));
  }
  if (isPlainObject(value)) {
    return Object.values(value).some((item) => hasMeaningfulEvidence(item));
  }
  return false;
}

function hasManualItemEvidence(item) {
  if (!isPlainObject(item.evidence)) {
    return false;
  }

  return hasMeaningfulEvidence(item.evidence.observations) || hasMeaningfulEvidence(item.evidence.artifacts);
}

function collectMeaningfulStrings(value, strings = []) {
  if (meaningfulString(value)) {
    strings.push(value.trim());
    return strings;
  }
  if (Array.isArray(value)) {
    value.forEach((item) => collectMeaningfulStrings(item, strings));
    return strings;
  }
  if (isPlainObject(value)) {
    Object.values(value).forEach((item) => collectMeaningfulStrings(item, strings));
  }
  return strings;
}

function validateAutomatedReleaseArtifactItem(item, prefix, errors) {
  const artifactReferences = new Set(collectMeaningfulStrings(item.evidence?.artifacts));

  for (const requiredReference of requiredAutomatedReleaseArtifactReferences) {
    if (!artifactReferences.has(requiredReference)) {
      errors.push(`${prefix} requires automated release artifact reference ${requiredReference}`);
    }
  }
}

function normalizeAtRestDisclosureText(value) {
  return value
    .toLowerCase()
    .replace(/[_/\\-]+/g, " ")
    .replace(/[^a-z0-9\s]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function matchesAllPatternGroups(value, patternGroups) {
  return patternGroups.every((patterns) => patterns.some((pattern) => pattern.test(value)));
}

function validateAtRestDisclosureItem(item, prefix, errors) {
  const disclosureText = normalizeAtRestDisclosureText(
    collectMeaningfulStrings([item.notes, item.evidence?.observations]).join("\n"),
  );

  for (const requirement of atRestDisclosureRequirements) {
    if (!matchesAllPatternGroups(disclosureText, requirement.patternGroups)) {
      errors.push(`${prefix} at-rest disclosure requires evidence that ${requirement.label}`);
    }
  }
}

function skipReasonFor(item) {
  if (nonEmptyString(item.skipReason)) {
    return item.skipReason;
  }
  if (isPlainObject(item.evidence) && nonEmptyString(item.evidence.skipReason)) {
    return item.evidence.skipReason;
  }
  return "";
}

function normalizeGitTag(value) {
  return value.startsWith("refs/tags/") ? value.slice("refs/tags/".length) : value;
}

function isArm64DmgArtifact(value) {
  const basename = path.basename(value).toLowerCase();
  return basename.endsWith(".dmg") && (basename.includes("aarch64") || basename.includes("arm64"));
}

function isIso8601TimestampWithTimezone(value) {
  const match = value.match(iso8601TimestampWithTimezonePattern);
  if (!match || Number.isNaN(Date.parse(value))) {
    return false;
  }

  const [, yearText, monthText, dayText, hourText, minuteText, secondText] = match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const second = Number(secondText);
  if (month < 1 || month > 12 || hour > 23 || minute > 59 || second > 59) {
    return false;
  }

  const daysInMonth = new Date(Date.UTC(year, month, 0)).getUTCDate();
  return day >= 1 && day <= daysInMonth;
}

function validateFilledBuildMetadata(evidence, errors) {
  const version = requireFilledStringFormat(
    errors,
    evidence,
    ["build", "version"],
    (value) => semverPattern.test(value),
    "a SemVer-like version",
  );
  const gitRef = requireFilledStringFormat(
    errors,
    evidence,
    ["build", "gitRef"],
    (value) => semverTagPattern.test(normalizeGitTag(value)),
    "a vMAJOR.MINOR.PATCH tag",
  );
  const normalizedTag = normalizeGitTag(gitRef);
  if (semverPattern.test(version) && semverTagPattern.test(normalizedTag) && normalizedTag !== `v${version}`) {
    errors.push(`build.gitRef must match build.version as v${version}`);
  }

  requireFilledStringFormat(
    errors,
    evidence,
    ["build", "gitSha"],
    (value) => gitShaPattern.test(value),
    "a plausible git SHA (7-40 hex characters)",
  );
  requireFilledStringFormat(
    errors,
    evidence,
    ["build", "buildTimestamp"],
    isIso8601TimestampWithTimezone,
    "an ISO-8601 timestamp with timezone",
  );
  const artifactPath = requireFilledStringFormat(
    errors,
    evidence,
    ["build", "releaseArtifact", "path"],
    isArm64DmgArtifact,
    "an arm64/aarch64 .dmg artifact path",
  );
  const artifactName = requireFilledStringFormat(
    errors,
    evidence,
    ["build", "releaseArtifact", "name"],
    isArm64DmgArtifact,
    "an arm64/aarch64 .dmg artifact name",
  );
  requireFilledStringFormat(
    errors,
    evidence,
    ["build", "releaseArtifact", "sha256"],
    (value) => sha256Pattern.test(value),
    "a 64-hex SHA-256 checksum",
  );

  if (semverPattern.test(version)) {
    const expectedArtifactName = `Curiosity-Transcripts-${version}-macos-aarch64.dmg`;
    if (artifactPath && path.basename(artifactPath) !== expectedArtifactName) {
      errors.push(`build.releaseArtifact.path must end with ${expectedArtifactName}`);
    }
    if (artifactName && artifactName !== expectedArtifactName) {
      errors.push(`build.releaseArtifact.name must be ${expectedArtifactName}`);
    }
  }
  if (artifactPath && artifactName && path.basename(artifactPath) !== artifactName) {
    errors.push("build.releaseArtifact.path basename must match build.releaseArtifact.name");
  }
}

function validateFilledMachineMetadata(evidence, errors) {
  requireFilledStringFormat(
    errors,
    evidence,
    ["machine", "architecture"],
    (value) => firstReleaseArchitectureValues.has(value.toLowerCase()),
    "arm64 or aarch64 for the first release target",
  );
  requireFilledStringFormat(
    errors,
    evidence,
    ["machine", "macosVersion"],
    (value) => macosVersionPattern.test(value),
    "a concrete macOS version such as 15.5",
  );
  requireFilledStringFormat(
    errors,
    evidence,
    ["machine", "machineLabel"],
    meaningfulString,
    "a non-placeholder smoke-test Mac label or model",
  );
}

function validateFilledModelSetup(evidence, errors) {
  if (isCompleted(valueAt(evidence, ["modelSetup", "whisper", "status"]))) {
    requireFilledStringFormat(
      errors,
      evidence,
      ["modelSetup", "whisper", "sha256"],
      (value) => sha256Pattern.test(value),
      "a 64-hex SHA-256 checksum",
    );

    const whisperFileSizeLabel = "modelSetup.whisper.fileSizeBytes";
    const whisperFileSizeBytes = valueAt(evidence, ["modelSetup", "whisper", "fileSizeBytes"]);
    if (!Number.isInteger(whisperFileSizeBytes) || whisperFileSizeBytes <= 0) {
      errors.push(`${whisperFileSizeLabel} must be a positive integer number`);
    }

    requireFilledStringFormat(
      errors,
      evidence,
      ["modelSetup", "whisper", "pathTestEvidence"],
      meaningfulString,
      "meaningful non-placeholder path-test evidence",
    );
  }

  if (isCompleted(valueAt(evidence, ["modelSetup", "ollama", "status"]))) {
    requireFilledStringFormat(
      errors,
      evidence,
      ["modelSetup", "ollama", "baseUrl"],
      isLocalOllamaBaseUrl,
      "a local loopback http(s) URL without credentials, query, or fragment",
    );
    const ollamaModel = requireFilledStringFormat(
      errors,
      evidence,
      ["modelSetup", "ollama", "model"],
      isLocalOllamaModelTag,
      "a local Ollama model tag, not a hosted/cloud preset",
    );
    const tagsEvidence = requireFilledStringFormat(
      errors,
      evidence,
      ["modelSetup", "ollama", "tagsEvidence"],
      meaningfulString,
      "meaningful non-placeholder /api/tags evidence",
    );
    if (tagsEvidence && ollamaModel && !tagsEvidenceNamesOllamaModel(tagsEvidence, ollamaModel)) {
      errors.push("modelSetup.ollama.tagsEvidence must name modelSetup.ollama.model");
    }
  }
}

function collectTemplatePlaceholders(errors, value, options, pathParts = []) {
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (isPlaceholderText(value) || (!options.allowIncomplete && normalized === "pending")) {
      errors.push(`Non-template evidence must replace placeholder value at ${fieldLabel(pathParts)}`);
    }
    return;
  }

  if (Array.isArray(value)) {
    value.forEach((item, index) => collectTemplatePlaceholders(errors, item, options, [...pathParts, index]));
    return;
  }

  if (!isPlainObject(value)) {
    return;
  }

  for (const [key, child] of Object.entries(value)) {
    collectTemplatePlaceholders(errors, child, options, [...pathParts, key]);
  }
}

function validateTopLevel(evidence, errors) {
  if (!isPlainObject(evidence)) {
    errors.push("Evidence root must be a JSON object");
    return;
  }

  for (const field of [
    "schemaVersion",
    "kind",
    "scope",
    "isTemplate",
    "overallResult",
    "build",
    "machine",
    "modelSetup",
    "manualItems",
  ]) {
    requirePresent(errors, evidence, [field]);
  }

  if (evidence.schemaVersion !== "1.0") {
    errors.push('schemaVersion must be "1.0"');
  }
  if (evidence.kind !== "release-candidate-smoke-evidence") {
    errors.push('kind must be "release-candidate-smoke-evidence"');
  }
  if (!nonEmptyString(evidence.scope)) {
    errors.push("scope must be a non-empty string");
  }
  if (typeof evidence.isTemplate !== "boolean") {
    errors.push("isTemplate must be a boolean");
  }
  if (!overallResultValues.has(evidence.overallResult)) {
    errors.push(`overallResult must be one of ${Array.from(overallResultValues).join(", ")}`);
  }
  if (evidence.isTemplate === true && evidence.overallResult !== "pending") {
    errors.push("Template evidence overallResult must remain pending");
  }
}

function isCompleted(value) {
  if (value === true) {
    return true;
  }
  if (typeof value !== "string") {
    return false;
  }
  return completionValues.has(value.trim().toLowerCase());
}

function validateCriticalResultFields(evidence, errors, options) {
  for (const pathParts of criticalResultFields) {
    if (!requirePresent(errors, evidence, pathParts)) {
      continue;
    }

    const label = fieldLabel(pathParts);
    const value = valueAt(evidence, pathParts);

    if (evidence.isTemplate === true && value !== "pending") {
      errors.push(`Template evidence ${label} must remain pending`);
      continue;
    }

    if (evidence.isTemplate === false && !options.allowIncomplete && !isCompleted(value)) {
      errors.push(`Strict non-template evidence requires ${label} to be passed or completed`);
    }
  }
}

function validateBuild(evidence, errors) {
  if (!requireObject(errors, evidence, ["build"])) {
    return;
  }

  for (const pathParts of [
    ["build", "version"],
    ["build", "gitRef"],
    ["build", "gitSha"],
    ["build", "buildTimestamp"],
    ["build", "releaseArtifact", "path"],
    ["build", "releaseArtifact", "name"],
    ["build", "releaseArtifact", "sha256"],
    ["build", "signing", "status"],
    ["build", "signing", "developerIdApplication"],
    ["build", "signing", "developerIdInstaller"],
    ["build", "signing", "codesignVerified"],
    ["build", "notarization", "status"],
    ["build", "notarization", "requestId"],
    ["build", "notarization", "stapled"],
    ["build", "notarization", "gatekeeperVerified"],
  ]) {
    requireNonEmptyString(errors, evidence, pathParts);
  }

  if (evidence.isTemplate === false) {
    validateFilledBuildMetadata(evidence, errors);
  }
}

function validateMachine(evidence, errors) {
  if (!requireObject(errors, evidence, ["machine"])) {
    return;
  }

  for (const pathParts of [
    ["machine", "machineLabel"],
    ["machine", "architecture"],
    ["machine", "macosVersion"],
    ["machine", "cleanUserAccount", "accountLabel"],
    ["machine", "cleanUserAccount", "createdForSmoke"],
    ["machine", "cleanUserAccount", "developmentCheckoutAbsent"],
  ]) {
    requireNonEmptyString(errors, evidence, pathParts);
  }

  if (evidence.isTemplate === false) {
    validateFilledMachineMetadata(evidence, errors);
  }
}

function validateModelSetup(evidence, errors) {
  if (!requireObject(errors, evidence, ["modelSetup"])) {
    return;
  }

  const whisper = requireObject(errors, evidence, ["modelSetup", "whisper"]);
  if (whisper) {
    for (const field of ["modelPath", "status", "sha256", "fileSizeBytes", "modifiedAt", "pathTestEvidence", "skipReason"]) {
      requirePresent(errors, evidence, ["modelSetup", "whisper", field]);
    }
    requireNonEmptyString(errors, evidence, ["modelSetup", "whisper", "status"]);
    if (!nonEmptyString(whisper.modelPath) && !nonEmptyString(whisper.skipReason)) {
      errors.push("modelSetup.whisper requires modelPath or explicit skipReason");
    }
  }

  const ollama = requireObject(errors, evidence, ["modelSetup", "ollama"]);
  if (ollama) {
    for (const field of ["baseUrl", "model", "status", "tagsEvidence", "skipReason"]) {
      requirePresent(errors, evidence, ["modelSetup", "ollama", field]);
    }
    requireNonEmptyString(errors, evidence, ["modelSetup", "ollama", "status"]);
    if ((!nonEmptyString(ollama.baseUrl) || !nonEmptyString(ollama.model)) && !nonEmptyString(ollama.skipReason)) {
      errors.push("modelSetup.ollama requires baseUrl/model or explicit skipReason");
    }
  }

  if (evidence.isTemplate === false) {
    validateFilledModelSetup(evidence, errors);
  }
}

function validateManualItems(evidence, errors, options) {
  if (!Array.isArray(evidence.manualItems)) {
    errors.push("manualItems must be an array");
    return;
  }

  const actualIds = evidence.manualItems.map((item) => item?.id);
  if (JSON.stringify(actualIds) !== JSON.stringify(expectedManualItemIds)) {
    errors.push(
      `manualItems must exactly match expected ordered ids: expected ${expectedManualItemIds.join(", ")}; found ${actualIds.join(", ")}`,
    );
  }

  for (const [index, item] of evidence.manualItems.entries()) {
    const prefix = `manualItems[${index}]`;
    if (!isPlainObject(item)) {
      errors.push(`${prefix} must be an object`);
      continue;
    }

    for (const field of ["id", "label", "status", "evidence", "notes"]) {
      if (!hasOwn(item, field)) {
        errors.push(`${prefix} missing required field ${field}`);
      }
    }

    if (!nonEmptyString(item.id)) {
      errors.push(`${prefix}.id must be a non-empty string`);
    }
    if (!nonEmptyString(item.label)) {
      errors.push(`${prefix}.label must be a non-empty string`);
    }
    if (!manualStatusValues.has(item.status)) {
      errors.push(`${prefix}.status must be one of ${Array.from(manualStatusValues).join(", ")}`);
    }

    if (evidence.isTemplate && item.status !== "pending") {
      errors.push(`${prefix} template item must remain pending; found ${item.status}`);
    }

    if (!evidence.isTemplate && !options.allowIncomplete && item.status !== "passed") {
      errors.push(`${prefix} status ${item.status} fails strict non-template validation`);
    }

    if (item.status === "skipped" && !nonEmptyString(skipReasonFor(item))) {
      errors.push(`${prefix} skipped item requires a non-empty skip reason`);
    }

    if (item.status === "failed" && (!nonEmptyString(item.notes) || !hasMeaningfulEvidence(item.evidence))) {
      errors.push(`${prefix} failed item requires non-empty notes and evidence`);
    }

    if (item.status === "passed" && (!meaningfulString(item.notes) || !hasManualItemEvidence(item))) {
      errors.push(`${prefix} passed item requires meaningful notes and observation or artifact evidence`);
    }

    if (!evidence.isTemplate && !options.allowIncomplete && item.id === automatedReleaseArtifactItemId) {
      validateAutomatedReleaseArtifactItem(item, prefix, errors);
    }

    if (
      !evidence.isTemplate &&
      !options.allowIncomplete &&
      item.id === atRestDisclosureItemId &&
      item.status === "passed"
    ) {
      validateAtRestDisclosureItem(item, prefix, errors);
    }
  }

  const incompleteItems = evidence.manualItems.filter((item) => item?.status !== "passed");
  if (evidence.overallResult === "passed" && incompleteItems.length > 0) {
    errors.push(
      `overallResult cannot be passed while manualItems include ${incompleteItems
        .map((item) => `${item.id}:${item.status}`)
        .join(", ")}`,
    );
  }

  if (!evidence.isTemplate && !options.allowIncomplete && evidence.overallResult !== "passed") {
    errors.push("Non-template evidence must set overallResult to passed after all manual items pass");
  }
}

function validateEvidence(evidence, options = {}) {
  const errors = [];

  validateTopLevel(evidence, errors);
  if (!isPlainObject(evidence)) {
    return errors;
  }

  validateBuild(evidence, errors);
  validateMachine(evidence, errors);
  validateModelSetup(evidence, errors);
  validateManualItems(evidence, errors, options);
  validateCriticalResultFields(evidence, errors, options);

  if (evidence.isTemplate === false) {
    collectTemplatePlaceholders(errors, evidence, options);
  }

  return errors;
}

function expectRejected(name, errors, expectedText) {
  if (errors.length === 0) {
    fail(scriptLabel, `Self-test did not reject: ${name}`);
    return;
  }
  if (expectedText && !errors.some((error) => error.includes(expectedText))) {
    fail(scriptLabel, `Self-test rejected ${name}, but not for ${expectedText}`);
  }
}

function expectAccepted(name, errors) {
  if (errors.length > 0) {
    fail(scriptLabel, `Self-test rejected accepted case ${name}: ${errors.join("; ")}`);
  }
}

function expectFileIncludes(file, expected, description) {
  const filePath = path.join(repoRoot, file);
  let text = "";
  try {
    text = fs.readFileSync(filePath, "utf8");
  } catch (error) {
    fail(scriptLabel, `Self-test could not read ${file}: ${error.message}`);
    return;
  }

  if (!text.includes(expected)) {
    fail(scriptLabel, `Self-test expected ${description} in ${file}`);
  }
}

function runSelfTests() {
  expectAccepted("valid template fixture", validateEvidence(createFixture()));

  const missingMetadata = createFixture();
  delete missingMetadata.build.gitSha;
  expectRejected("missing build metadata", validateEvidence(missingMetadata), "build.gitSha");

  const templatePassed = createFixture();
  templatePassed.manualItems[0].status = "passed";
  expectRejected("template item marked passed", validateEvidence(templatePassed), "template");

  const nonTemplateIncomplete = createFixture();
  nonTemplateIncomplete.isTemplate = false;
  nonTemplateIncomplete.overallResult = "passed";
  nonTemplateIncomplete.manualItems = nonTemplateIncomplete.manualItems.map((item) => ({
    ...item,
    status: "passed",
  }));
  nonTemplateIncomplete.manualItems[2].status = "skipped";
  nonTemplateIncomplete.manualItems[2].evidence.skipReason = "Whisper model unavailable";
  expectRejected(
    "non-template skipped item cannot be treated as passed",
    validateEvidence(nonTemplateIncomplete),
    "overallResult",
  );

  const skippedWithoutReason = createFixture();
  skippedWithoutReason.isTemplate = false;
  skippedWithoutReason.overallResult = "incomplete";
  skippedWithoutReason.manualItems = skippedWithoutReason.manualItems.map((item) => ({
    ...item,
    status: "passed",
  }));
  skippedWithoutReason.manualItems[4].status = "skipped";
  skippedWithoutReason.manualItems[4].evidence.skipReason = "";
  expectRejected(
    "skipped item without reason",
    validateEvidence(skippedWithoutReason, { allowIncomplete: true }),
    "skip reason",
  );

  const staleIds = createFixture();
  staleIds.manualItems[0].id = "old-clean-install";
  expectRejected("stale manual item ids", validateEvidence(staleIds), "manualItems");

  const missingIds = createFixture();
  missingIds.manualItems.pop();
  expectRejected("missing manual item ids", validateEvidence(missingIds), "manualItems");

  const strictPendingMetadata = createStrictPassedFixture();
  strictPendingMetadata.build.signing.status = "pending";
  strictPendingMetadata.build.notarization.status = "pending";
  strictPendingMetadata.machine.cleanUserAccount.createdForSmoke = "pending";
  strictPendingMetadata.modelSetup.whisper.status = "pending";
  expectRejected(
    "strict non-template rejects pending non-manual metadata",
    validateEvidence(strictPendingMetadata),
    "build.signing.status",
  );

  const templateOverallPassed = createFixture();
  templateOverallPassed.overallResult = "passed";
  expectRejected("template overallResult passed", validateEvidence(templateOverallPassed), "overallResult");

  const templateWhisperPassed = createFixture();
  templateWhisperPassed.modelSetup.whisper.status = "passed";
  expectRejected("template Whisper status passed", validateEvidence(templateWhisperPassed), "modelSetup.whisper.status");

  const templateSigningPassed = createFixture();
  templateSigningPassed.build.signing.status = "passed";
  expectRejected("template signing status passed", validateEvidence(templateSigningPassed), "build.signing.status");

  expectAccepted("strict filled evidence", validateEvidence(createStrictPassedFixture()));

  const lowercaseTemplateMetadata = createStrictPassedFixture();
  lowercaseTemplateMetadata.build.signing.developerIdApplication = "template_developer_id_application";
  expectRejected(
    "filled evidence lowercase template metadata",
    validateEvidence(lowercaseTemplateMetadata),
    "build.signing.developerIdApplication",
  );

  const lowercasePendingMetadata = createStrictPassedFixture();
  lowercasePendingMetadata.build.signing.developerIdApplication = "pending: add real Developer ID identity";
  expectRejected(
    "filled evidence lowercase pending metadata",
    validateEvidence(lowercasePendingMetadata),
    "build.signing.developerIdApplication",
  );

  const embeddedTemplateMetadata = createStrictPassedFixture();
  embeddedTemplateMetadata.build.signing.developerIdApplication =
    "Developer ID Application: TEMPLATE_DEVELOPER_ID_APPLICATION";
  expectRejected(
    "filled evidence embedded template metadata",
    validateEvidence(embeddedTemplateMetadata),
    "build.signing.developerIdApplication",
  );

  const embeddedLowercaseTemplateMetadata = createStrictPassedFixture();
  embeddedLowercaseTemplateMetadata.build.signing.developerIdApplication =
    "Developer ID Application: template_developer_id_application";
  expectRejected(
    "filled evidence embedded lowercase template metadata",
    validateEvidence(embeddedLowercaseTemplateMetadata),
    "build.signing.developerIdApplication",
  );

  const badBuildVersion = createStrictPassedFixture();
  badBuildVersion.build.version = "release-1";
  expectRejected("filled evidence bad build version", validateEvidence(badBuildVersion), "build.version");

  const badGitRef = createStrictPassedFixture();
  badGitRef.build.gitRef = "main";
  expectRejected("filled evidence bad git tag", validateEvidence(badGitRef), "build.gitRef");

  const mismatchedGitRef = createStrictPassedFixture();
  mismatchedGitRef.build.gitRef = "refs/tags/v1.2.4";
  expectRejected("filled evidence mismatched git tag", validateEvidence(mismatchedGitRef), "build.gitRef");

  const badGitSha = createStrictPassedFixture();
  badGitSha.build.gitSha = "not-a-sha";
  expectRejected("filled evidence bad git SHA", validateEvidence(badGitSha), "build.gitSha");

  const badBuildTimestamp = createStrictPassedFixture();
  badBuildTimestamp.build.buildTimestamp = "soon";
  expectRejected("filled evidence bad build timestamp", validateEvidence(badBuildTimestamp), "build.buildTimestamp");

  const impossibleBuildTimestamp = createStrictPassedFixture();
  impossibleBuildTimestamp.build.buildTimestamp = "2026-02-31T12:00:00Z";
  expectRejected(
    "filled evidence impossible build timestamp date",
    validateEvidence(impossibleBuildTimestamp),
    "build.buildTimestamp",
  );

  const badArtifactPath = createStrictPassedFixture();
  badArtifactPath.build.releaseArtifact.path = "/tmp/Curiosity-Transcripts.zip";
  expectRejected("filled evidence non-DMG artifact path", validateEvidence(badArtifactPath), "build.releaseArtifact.path");

  const badArtifactName = createStrictPassedFixture();
  badArtifactName.build.releaseArtifact.name = "Curiosity-Transcripts-1.2.3-macos-x64.dmg";
  expectRejected("filled evidence wrong artifact name", validateEvidence(badArtifactName), "build.releaseArtifact.name");

  const mismatchedArtifactVersion = createStrictPassedFixture();
  mismatchedArtifactVersion.build.releaseArtifact.path = "/tmp/Curiosity-Transcripts-9.9.9-macos-aarch64.dmg";
  mismatchedArtifactVersion.build.releaseArtifact.name = "Curiosity-Transcripts-9.9.9-macos-aarch64.dmg";
  expectRejected(
    "filled evidence mismatched artifact version",
    validateEvidence(mismatchedArtifactVersion),
    "build.releaseArtifact.path",
  );

  const badArtifactChecksum = createStrictPassedFixture();
  badArtifactChecksum.build.releaseArtifact.sha256 = "abc123";
  expectRejected(
    "filled evidence bad artifact checksum",
    validateEvidence(badArtifactChecksum),
    "build.releaseArtifact.sha256",
  );

  const badMachineArchitecture = createStrictPassedFixture();
  badMachineArchitecture.machine.architecture = "x64";
  expectRejected(
    "filled evidence wrong machine architecture",
    validateEvidence(badMachineArchitecture),
    "machine.architecture",
  );

  const badMacosVersion = createStrictPassedFixture();
  badMacosVersion.machine.macosVersion = "latest";
  expectRejected("filled evidence bad macOS version", validateEvidence(badMacosVersion), "machine.macosVersion");

  const marketingMacosVersion = createStrictPassedFixture();
  marketingMacosVersion.machine.macosVersion = "macOS Sequoia 15.5 (24F74)";
  expectAccepted("filled evidence macOS marketing version", validateEvidence(marketingMacosVersion));

  const weakMachineLabel = createStrictPassedFixture();
  weakMachineLabel.machine.machineLabel = "unknown";
  expectRejected("filled evidence weak machine label", validateEvidence(weakMachineLabel), "machine.machineLabel");

  const templateMachineLabel = createStrictPassedFixture();
  templateMachineLabel.machine.machineLabel = "TEMPLATE";
  expectRejected(
    "filled evidence template machine label",
    validateEvidence(templateMachineLabel),
    "machine.machineLabel",
  );

  const badWhisperChecksum = createStrictPassedFixture();
  badWhisperChecksum.modelSetup.whisper.sha256 = "not-a-sha256";
  expectRejected(
    "filled evidence bad Whisper checksum",
    validateEvidence(badWhisperChecksum),
    "modelSetup.whisper.sha256",
  );

  const zeroByteWhisperModel = createStrictPassedFixture();
  zeroByteWhisperModel.modelSetup.whisper.fileSizeBytes = 0;
  expectRejected(
    "filled evidence zero-byte Whisper model",
    validateEvidence(zeroByteWhisperModel),
    "modelSetup.whisper.fileSizeBytes",
  );

  const stringWhisperModelSize = createStrictPassedFixture();
  stringWhisperModelSize.modelSetup.whisper.fileSizeBytes = "147";
  expectRejected(
    "filled evidence string Whisper model size",
    validateEvidence(stringWhisperModelSize),
    "modelSetup.whisper.fileSizeBytes",
  );

  const weakWhisperPathTestEvidence = createStrictPassedFixture();
  weakWhisperPathTestEvidence.modelSetup.whisper.pathTestEvidence = "ok";
  expectRejected(
    "filled evidence weak Whisper path test evidence",
    validateEvidence(weakWhisperPathTestEvidence),
    "modelSetup.whisper.pathTestEvidence",
  );

  const placeholderWhisperPathTestEvidence = createStrictPassedFixture();
  placeholderWhisperPathTestEvidence.modelSetup.whisper.pathTestEvidence =
    "TEMPLATE_WHISPER_PATH_TEST_EVIDENCE_OR_PENDING";
  expectRejected(
    "filled evidence placeholder Whisper path test evidence",
    validateEvidence(placeholderWhisperPathTestEvidence),
    "modelSetup.whisper.pathTestEvidence",
  );

  for (const loopbackBaseUrl of [
    "http://localhost:11434",
    "https://localhost:11434",
    "http://127.0.0.1:11434",
    "http://[::1]:11434",
  ]) {
    const loopbackOllamaBaseUrl = createStrictPassedFixture();
    loopbackOllamaBaseUrl.modelSetup.ollama.baseUrl = loopbackBaseUrl;
    expectAccepted(
      `filled evidence loopback Ollama URL ${loopbackBaseUrl}`,
      validateEvidence(loopbackOllamaBaseUrl),
    );
  }

  const remoteOllamaBaseUrl = createStrictPassedFixture();
  remoteOllamaBaseUrl.modelSetup.ollama.baseUrl = "https://ollama.example.com";
  expectRejected(
    "filled evidence remote Ollama base URL",
    validateEvidence(remoteOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const unsupportedSchemeOllamaBaseUrl = createStrictPassedFixture();
  unsupportedSchemeOllamaBaseUrl.modelSetup.ollama.baseUrl = "ftp://localhost:11434";
  expectRejected(
    "filled evidence unsupported-scheme Ollama base URL",
    validateEvidence(unsupportedSchemeOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  for (const malformedBaseUrl of ["http:@localhost:11434", "http:/@localhost:11434", "http:localhost:11434"]) {
    const malformedOllamaBaseUrl = createStrictPassedFixture();
    malformedOllamaBaseUrl.modelSetup.ollama.baseUrl = malformedBaseUrl;
    expectRejected(
      `filled evidence malformed Ollama base URL ${malformedBaseUrl}`,
      validateEvidence(malformedOllamaBaseUrl),
      "modelSetup.ollama.baseUrl",
    );
  }

  const credentialOllamaBaseUrl = createStrictPassedFixture();
  credentialOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://user:pass@127.0.0.1:11434";
  expectRejected(
    "filled evidence credential-bearing Ollama base URL",
    validateEvidence(credentialOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const emptyUserinfoOllamaBaseUrl = createStrictPassedFixture();
  emptyUserinfoOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://@127.0.0.1:11434";
  expectRejected(
    "filled evidence empty-userinfo Ollama base URL",
    validateEvidence(emptyUserinfoOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const queryOllamaBaseUrl = createStrictPassedFixture();
  queryOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://127.0.0.1:11434?token=secret";
  expectRejected(
    "filled evidence query-bearing Ollama base URL",
    validateEvidence(queryOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const emptyQueryOllamaBaseUrl = createStrictPassedFixture();
  emptyQueryOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://127.0.0.1:11434?";
  expectRejected(
    "filled evidence empty-query-marker Ollama base URL",
    validateEvidence(emptyQueryOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const fragmentOllamaBaseUrl = createStrictPassedFixture();
  fragmentOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://127.0.0.1:11434/#token";
  expectRejected(
    "filled evidence fragment-bearing Ollama base URL",
    validateEvidence(fragmentOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const emptyFragmentOllamaBaseUrl = createStrictPassedFixture();
  emptyFragmentOllamaBaseUrl.modelSetup.ollama.baseUrl = "http://127.0.0.1:11434#";
  expectRejected(
    "filled evidence empty-fragment-marker Ollama base URL",
    validateEvidence(emptyFragmentOllamaBaseUrl),
    "modelSetup.ollama.baseUrl",
  );

  const hostedOllamaModelTag = createStrictPassedFixture();
  hostedOllamaModelTag.modelSetup.ollama.model = "deepseek-v3.2:cloud";
  expectRejected(
    "filled evidence hosted Ollama model tag",
    validateEvidence(hostedOllamaModelTag),
    "modelSetup.ollama.model",
  );

  const hostedPresetOllamaModel = createStrictPassedFixture();
  hostedPresetOllamaModel.modelSetup.ollama.model = "DeepSeek V3.2 Speciale";
  expectRejected(
    "filled evidence hosted preset Ollama model",
    validateEvidence(hostedPresetOllamaModel),
    "modelSetup.ollama.model",
  );

  const emptyOllamaModel = createStrictPassedFixture();
  emptyOllamaModel.modelSetup.ollama.model = " ";
  emptyOllamaModel.modelSetup.ollama.skipReason = "filled evidence cannot claim passed setup without a local model";
  expectRejected(
    "filled evidence empty Ollama model",
    validateEvidence(emptyOllamaModel),
    "modelSetup.ollama.model",
  );

  const weakOllamaTagsEvidence = createStrictPassedFixture();
  weakOllamaTagsEvidence.modelSetup.ollama.tagsEvidence = "ok";
  expectRejected(
    "filled evidence weak Ollama tags evidence",
    validateEvidence(weakOllamaTagsEvidence),
    "modelSetup.ollama.tagsEvidence",
  );

  const genericOllamaTagsEvidence = createStrictPassedFixture();
  genericOllamaTagsEvidence.modelSetup.ollama.tagsEvidence =
    "/api/tags returned installed local models before offline smoke.";
  expectRejected(
    "filled evidence generic Ollama tags evidence without model name",
    validateEvidence(genericOllamaTagsEvidence),
    "modelSetup.ollama.model",
  );

  const passedItemWithoutEvidence = createStrictPassedFixture();
  passedItemWithoutEvidence.manualItems[0].evidence = {
    observations: [],
    artifacts: [],
    skipReason: "",
  };
  expectRejected(
    "passed manual item without evidence",
    validateEvidence(passedItemWithoutEvidence),
    "manualItems[0] passed item",
  );

  const passedItemWithoutNotes = createStrictPassedFixture();
  passedItemWithoutNotes.manualItems[0].notes = "";
  expectRejected(
    "passed manual item without notes",
    validateEvidence(passedItemWithoutNotes),
    "manualItems[0] passed item",
  );

  const passedItemWithWeakEvidence = createStrictPassedFixture();
  passedItemWithWeakEvidence.manualItems[0].evidence = {
    observations: ["ok"],
    artifacts: [],
    skipReason: "not applicable",
  };
  passedItemWithWeakEvidence.manualItems[0].notes = "ok";
  expectRejected(
    "passed manual item with weak evidence",
    validateEvidence(passedItemWithWeakEvidence),
    "manualItems[0] passed item",
  );

  const passedItemWithBareTemplateEvidence = createStrictPassedFixture();
  passedItemWithBareTemplateEvidence.manualItems[0].evidence = {
    observations: ["TEMPLATE"],
    artifacts: [],
    skipReason: "",
  };
  passedItemWithBareTemplateEvidence.manualItems[0].notes = "TEMPLATE";
  expectRejected(
    "passed manual item with bare template evidence",
    validateEvidence(passedItemWithBareTemplateEvidence),
    "manualItems[0] passed item",
  );

  const passedItemWithLowercasePendingEvidence = createStrictPassedFixture();
  passedItemWithLowercasePendingEvidence.manualItems[0].evidence = {
    observations: ["pending: capture clean install result"],
    artifacts: [],
    skipReason: "",
  };
  passedItemWithLowercasePendingEvidence.manualItems[0].notes = "pending: attach install screenshot";
  expectRejected(
    "passed manual item with lowercase pending evidence",
    validateEvidence(passedItemWithLowercasePendingEvidence),
    "manualItems[0] passed item",
  );

  const atRestDisclosureItemIndex = createStrictPassedFixture().manualItems.findIndex(
    (item) => item.id === atRestDisclosureItemId,
  );
  const setAtRestDisclosureEvidence = (evidence, disclosureText) => {
    evidence.manualItems[atRestDisclosureItemIndex].evidence = {
      observations: [disclosureText],
      artifacts: ["release-smoke/at-rest-disclosure-release-notes.txt"],
      skipReason: "",
    };
    evidence.manualItems[atRestDisclosureItemIndex].notes = disclosureText;
  };
  if (atRestDisclosureItemIndex === -1) {
    fail(scriptLabel, "Self-test fixture missing at-rest-disclosure manual item");
  } else {
    const completeAtRestDisclosure = createStrictPassedFixture();
    setAtRestDisclosureEvidence(completeAtRestDisclosure, completeAtRestDisclosureEvidenceText);
    expectAccepted(
      "passed at-rest disclosure with all boundary categories",
      validateEvidence(completeAtRestDisclosure),
    );

    const pathOnlyAtRestDisclosure = createStrictPassedFixture();
    pathOnlyAtRestDisclosure.manualItems[atRestDisclosureItemIndex].evidence = {
      observations: ["Release notes attached."],
      artifacts: [
        "release-smoke/v1-app-level-encryption-at-rest-not-implemented-app-private-data-os-user-account-file-protections-app-delete-controls-app-private-meeting-data-user-owned-source-files-exported-files-outside-app-delete-control.txt",
      ],
      skipReason: "",
    };
    pathOnlyAtRestDisclosure.manualItems[atRestDisclosureItemIndex].notes = "Release notes attached.";
    expectRejected(
      "passed at-rest disclosure ignores semantic claims in artifact paths",
      validateEvidence(pathOnlyAtRestDisclosure),
      "app-level encryption-at-rest is not implemented in v1",
    );

    const affirmativeEncryptionAtRestDisclosure = createStrictPassedFixture();
    setAtRestDisclosureEvidence(
      affirmativeEncryptionAtRestDisclosure,
      "v1 provides app-level encryption-at-rest without setup. App-private data relies on OS/user-account file protections. App delete controls app-private meeting data. User-owned source files and exported files can remain outside app deletion control.",
    );
    expectRejected(
      "passed at-rest disclosure rejects affirmative encryption-at-rest claim",
      validateEvidence(affirmativeEncryptionAtRestDisclosure),
      "app-level encryption-at-rest is not implemented in v1",
    );

    for (const missingCategory of [
      {
        name: "app-level encryption-at-rest is not implemented in v1",
        disclosureText:
          "App-private data relies on OS/user-account file protections. App delete controls app-private meeting data. User-owned source files and exported files can remain outside app deletion control.",
      },
      {
        name: "app-private data relies on OS/user-account file protections",
        disclosureText:
          "App-level encryption-at-rest is not implemented in v1. App delete controls app-private meeting data. User-owned source files and exported files can remain outside app deletion control.",
      },
      {
        name: "app delete controls app-private meeting data",
        disclosureText:
          "App-level encryption-at-rest is not implemented in v1. App-private data relies on OS/user-account file protections. User-owned source files and exported files can remain outside app deletion control.",
      },
      {
        name: "user-owned source files and exported files can remain outside app deletion control",
        disclosureText:
          "App-level encryption-at-rest is not implemented in v1. App-private data relies on OS/user-account file protections. App delete controls app-private meeting data.",
      },
    ]) {
      const missingAtRestDisclosureCategory = createStrictPassedFixture();
      setAtRestDisclosureEvidence(missingAtRestDisclosureCategory, missingCategory.disclosureText);
      expectRejected(
        `passed at-rest disclosure missing ${missingCategory.name}`,
        validateEvidence(missingAtRestDisclosureCategory),
        missingCategory.name,
      );
    }
  }

  const requiredArtifactItemIndex = createStrictPassedFixture().manualItems.findIndex(
    (item) => item.id === "automated-release-artifacts",
  );
  if (requiredArtifactItemIndex === -1) {
    fail(scriptLabel, "Self-test fixture missing automated-release-artifacts manual item");
  } else {
    for (const missingArtifact of [
      "release-artifacts/supply-chain",
      "release-artifacts/coverage",
      "release-artifacts/contracts/desktop-command-view-contract.receipt.json",
    ]) {
      const missingRequiredArtifact = createStrictPassedFixture();
      missingRequiredArtifact.manualItems[requiredArtifactItemIndex].evidence.artifacts =
        missingRequiredArtifact.manualItems[requiredArtifactItemIndex].evidence.artifacts.filter(
          (artifact) => artifact !== missingArtifact,
        );
      expectRejected(
        `automated release artifact item missing ${missingArtifact}`,
        validateEvidence(missingRequiredArtifact),
        missingArtifact,
      );
    }

    const suffixFalsePositive = createStrictPassedFixture();
    suffixFalsePositive.manualItems[requiredArtifactItemIndex].evidence.artifacts =
      suffixFalsePositive.manualItems[requiredArtifactItemIndex].evidence.artifacts.map((artifact) =>
        artifact === "release-artifacts/coverage" ? "release-artifacts/coverage-old" : artifact,
      );
    expectRejected(
      "automated release artifact item rejects suffix false positive",
      validateEvidence(suffixFalsePositive),
      "release-artifacts/coverage",
    );

    const proseFalsePositive = createStrictPassedFixture();
    proseFalsePositive.manualItems[requiredArtifactItemIndex].evidence.artifacts =
      proseFalsePositive.manualItems[requiredArtifactItemIndex].evidence.artifacts.filter(
        (artifact) => artifact !== "release-artifacts/supply-chain",
      );
    proseFalsePositive.manualItems[requiredArtifactItemIndex].notes =
      "Artifact missing release-artifacts/supply-chain from the draft evidence.";
    proseFalsePositive.manualItems[requiredArtifactItemIndex].evidence.observations = [
      "Do not accept prose-only references to release-artifacts/supply-chain.",
    ];
    expectRejected(
      "automated release artifact item rejects prose false positive",
      validateEvidence(proseFalsePositive),
      "release-artifacts/supply-chain",
    );
  }

  expectAccepted(
    "allow-incomplete draft evidence with pending statuses",
    validateEvidence(createAllowIncompleteDraftFixture(), { allowIncomplete: true }),
  );

  const allowIncompleteDraftWithPendingWhisperEvidence = createAllowIncompleteDraftFixture();
  allowIncompleteDraftWithPendingWhisperEvidence.modelSetup.whisper.sha256 = "pending";
  allowIncompleteDraftWithPendingWhisperEvidence.modelSetup.whisper.fileSizeBytes = "pending";
  allowIncompleteDraftWithPendingWhisperEvidence.modelSetup.whisper.pathTestEvidence = "pending";
  expectAccepted(
    "allow-incomplete draft evidence with pending Whisper metadata",
    validateEvidence(allowIncompleteDraftWithPendingWhisperEvidence, { allowIncomplete: true }),
  );

  expectFileIncludes(
    "docs/release-candidate-checklist.md",
    "node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json",
    "path-based filled evidence validation command",
  );
  expectFileIncludes(
    "docs/production-readiness-roadmap.md",
    "node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json",
    "path-based filled evidence validation command",
  );
}

function parseArgs(argv) {
  const options = {
    selfTest: false,
    allowIncomplete: false,
    evidencePath: defaultEvidencePath,
  };

  for (const arg of argv) {
    if (arg === "--self-test") {
      options.selfTest = true;
    } else if (arg === "--allow-incomplete") {
      options.allowIncomplete = true;
    } else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else if (!options.pathProvided) {
      options.evidencePath = path.resolve(arg);
      options.pathProvided = true;
    } else {
      fail(scriptLabel, `Unexpected argument: ${arg}`);
    }
  }

  return options;
}

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  console.log(
    [
      "Usage: node scripts/check-release-smoke-evidence.js [--allow-incomplete] [path/to/evidence.json]",
      "       node scripts/check-release-smoke-evidence.js --self-test",
      "",
      "Default path: docs/release-candidate-smoke-evidence.template.json",
      "--allow-incomplete permits draft non-template evidence with pending, failed, or skipped items.",
      "A passed overall result is still rejected unless every manual item passed.",
    ].join("\n"),
  );
  process.exit(ok ? 0 : 1);
}

if (options.selfTest) {
  runSelfTests();
  if (!ok) {
    process.exit(1);
  }
  console.log("Release smoke evidence self-tests passed.");
  process.exit(0);
}

function labelFor(filePath) {
  const relative = path.relative(repoRoot, filePath);
  if (!relative.startsWith("..") && !path.isAbsolute(relative)) {
    return relative;
  }
  return filePath;
}

let evidence;
const evidenceLabel = labelFor(options.evidencePath);

try {
  evidence = JSON.parse(fs.readFileSync(options.evidencePath, "utf8"));
} catch (error) {
  fail(evidenceLabel, `Unable to read or parse release smoke evidence: ${error.message}`);
}

if (evidence) {
  for (const error of validateEvidence(evidence, { allowIncomplete: options.allowIncomplete })) {
    fail(evidenceLabel, error);
  }
}

if (!ok) {
  process.exit(1);
}

console.log(`Release smoke evidence metadata is valid: ${evidenceLabel}`);
