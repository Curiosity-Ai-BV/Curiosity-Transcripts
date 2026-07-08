#!/usr/bin/env node
const fs = require("node:fs");
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
  "delete-cleanup",
  "uninstall-private-data",
];

const manualStatusValues = new Set(["pending", "passed", "failed", "skipped"]);
const overallResultValues = new Set(["pending", "passed", "failed", "incomplete"]);
const completionValues = new Set(["passed", "completed"]);
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
  evidence.manualItems = evidence.manualItems.map((item) => ({
    id: item.id,
    label: item.label.replace("Template label for ", ""),
    status: "passed",
    evidence: {
      observations: [`Observed ${item.id} pass during clean macOS DMG smoke.`],
      artifacts: [`release-smoke/${item.id}.txt`],
      skipReason: "",
    },
    notes: `${item.id} passed with recorded evidence.`,
  }));
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

function fieldLabel(pathParts) {
  return pathParts.join(".");
}

function valueAt(object, pathParts) {
  return pathParts.reduce((current, part) => current?.[part], object);
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

function hasMeaningfulEvidence(value) {
  if (nonEmptyString(value)) {
    return true;
  }
  if (typeof value === "number" || typeof value === "boolean") {
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

function skipReasonFor(item) {
  if (nonEmptyString(item.skipReason)) {
    return item.skipReason;
  }
  if (isPlainObject(item.evidence) && nonEmptyString(item.evidence.skipReason)) {
    return item.evidence.skipReason;
  }
  return "";
}

function collectTemplatePlaceholders(errors, value, options, pathParts = []) {
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (
      /\bTEMPLATE_[A-Z0-9_]+/.test(value) ||
      value.trim().startsWith("PENDING:") ||
      (!options.allowIncomplete && normalized === "pending")
    ) {
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

  expectAccepted(
    "allow-incomplete draft evidence with pending statuses",
    validateEvidence(createAllowIncompleteDraftFixture(), { allowIncomplete: true }),
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
