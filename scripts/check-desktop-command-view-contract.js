const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const root = path.resolve(__dirname, "..");
const fixturePath = path.join(root, "apps", "desktop", "contracts", "desktop-command-view-contract.fixture.json");
const schemaPath = path.join(root, "apps", "desktop", "contracts", "desktop-command-view-contract.schema.json");
const receiptPath = path.join(root, "release-artifacts", "contracts", "desktop-command-view-contract.receipt.json");
const fixtureLabel = "apps/desktop/contracts/desktop-command-view-contract.fixture.json";
const schemaLabel = "apps/desktop/contracts/desktop-command-view-contract.schema.json";
const receiptLabel = "release-artifacts/contracts/desktop-command-view-contract.receipt.json";
const scriptLabel = "scripts/check-desktop-command-view-contract.js";
const writeArtifactCommand = "node scripts/check-desktop-command-view-contract.js --write-artifact";
const sourceInputDescriptors = [
  {
    path: "apps/desktop/src-tauri/src/main.rs",
    role: "rust-producer-fixture-owner",
  },
  {
    path: "apps/desktop/src-tauri/src/calendar.rs",
    role: "rust-calendar-context-producer",
  },
  {
    path: "apps/desktop/src/commandAdapter.ts",
    role: "typescript-command-facade-and-ui-mapping",
  },
  {
    path: "apps/desktop/src/desktopContract.ts",
    role: "typescript-runtime-contract-validator",
  },
  {
    path: "apps/desktop/src/commandAdapter.contract.test.ts",
    role: "typescript-consumer-contract-tests",
  },
];

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function readJson(file, label) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (error) {
    fail(label, `Unable to read or parse JSON: ${error.message}`);
    return null;
  }
}

function parseArgs(argv) {
  const options = {
    help: false,
    writeArtifact: false,
    checkArtifact: null,
    selfTest: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--write-artifact") {
      options.writeArtifact = true;
    } else if (arg === "--self-test") {
      options.selfTest = true;
    } else if (arg === "--check-artifact") {
      const artifactPath = argv[index + 1];
      if (!artifactPath || artifactPath.startsWith("--")) {
        fail(scriptLabel, "--check-artifact requires a receipt path");
      } else {
        options.checkArtifact = artifactPath;
        index += 1;
      }
    } else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else {
      fail(scriptLabel, `Unexpected argument: ${arg}`);
    }
  }

  if (options.writeArtifact && options.checkArtifact) {
    fail(scriptLabel, "--write-artifact and --check-artifact cannot be combined");
  }
  if (options.selfTest && (options.writeArtifact || options.checkArtifact)) {
    fail(scriptLabel, "--self-test cannot be combined with artifact read/write modes");
  }

  return options;
}

function sha256File(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

function buildSourceInputs() {
  return sourceInputDescriptors.map((input) => ({
    path: input.path,
    role: input.role,
    sha256: sha256File(path.join(root, input.path)),
  }));
}

function buildReceipt(fixture, schema) {
  return {
    version: 1,
    kind: "desktop-command-view-contract-receipt",
    status: "passed",
    checker: {
      path: scriptLabel,
      command: writeArtifactCommand,
    },
    fixture: {
      path: fixtureLabel,
      sha256: sha256File(fixturePath),
      version: fixture.version,
      owner: fixture.owner,
    },
    schema: {
      path: schemaLabel,
      sha256: sha256File(schemaPath),
      version: schema.version,
      kind: schema.kind,
      scope: schema.scope,
      expectedCases: schema.expectedCases,
      forbiddenStrings: schema.forbiddenStrings,
    },
    sourceInputs: buildSourceInputs(),
  };
}

function writeReceipt(fixture, schema) {
  fs.mkdirSync(path.dirname(receiptPath), { recursive: true });
  fs.writeFileSync(receiptPath, `${JSON.stringify(buildReceipt(fixture, schema), null, 2)}\n`);
  console.log(`Wrote desktop command/view contract receipt: ${receiptLabel}`);
}

function pathLabel(pathParts) {
  return pathParts.map((part) => `[${JSON.stringify(part)}]`).join("");
}

function getPath(value, pathParts) {
  let current = value;
  for (const part of pathParts) {
    if (current === null || typeof current !== "object" || !(part in current)) {
      return { found: false, value: undefined };
    }
    current = current[part];
  }
  return { found: true, value: current };
}

function valueType(value) {
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return "array";
  }
  return typeof value;
}

function matchesType(value, expectedType) {
  if (expectedType === "nonEmptyString") {
    return typeof value === "string" && value.trim().length > 0;
  }
  if (expectedType === "object") {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }
  if (expectedType === "array") {
    return Array.isArray(value);
  }
  if (expectedType === "number") {
    return typeof value === "number" && Number.isFinite(value);
  }
  return valueType(value) === expectedType;
}

function compareLists(actual, expected) {
  return actual.length === expected.length && actual.every((item, index) => item === expected[index]);
}

function jsonEquals(actual, expected) {
  if (Object.is(actual, expected)) {
    return true;
  }
  if (valueType(actual) !== valueType(expected)) {
    return false;
  }
  if (Array.isArray(actual)) {
    return actual.length === expected.length && actual.every((item, index) => jsonEquals(item, expected[index]));
  }
  if (actual !== null && typeof actual === "object") {
    const actualKeys = Object.keys(actual).sort();
    const expectedKeys = Object.keys(expected).sort();
    return compareLists(actualKeys, expectedKeys)
      && actualKeys.every((key) => jsonEquals(actual[key], expected[key]));
  }
  return false;
}

function validateReceiptField(errors, receipt, pathParts, expected) {
  const located = getPath(receipt, pathParts);
  const label = pathLabel(pathParts);
  if (!located.found) {
    errors.push(`Receipt missing required field ${label}`);
    return;
  }
  if (!jsonEquals(located.value, expected)) {
    errors.push(`Receipt field ${label} must match current contract metadata`);
  }
}

function validateSchema(schema) {
  const errors = [];

  if (!schema || typeof schema !== "object" || Array.isArray(schema)) {
    return ["Schema lock must be a JSON object"];
  }
  if (schema.version !== 1) {
    errors.push("Schema lock version must be 1");
  }
  if (schema.kind !== "fixture-derived-command-view-contract-shape-lock") {
    errors.push("Schema lock kind must identify fixture-derived contract shape");
  }
  if (schema.scope !== "Locks the checked-in Rust-produced desktop command/view fixture shape. This is not generated DTO ownership.") {
    errors.push("Schema lock scope must avoid claiming generated DTO ownership");
  }
  if (schema.fixturePath !== fixtureLabel) {
    errors.push(`Schema fixturePath must be ${fixtureLabel}`);
  }
  if (schema.fixtureMetadata?.version !== 1) {
    errors.push("Schema fixtureMetadata.version must be 1");
  }
  if (schema.fixtureMetadata?.owner !== "apps/desktop/src-tauri/src/main.rs") {
    errors.push("Schema fixtureMetadata.owner must point at the Rust producer");
  }
  if (!Array.isArray(schema.expectedCases) || schema.expectedCases.length === 0) {
    errors.push("Schema expectedCases must be a non-empty array");
  }
  if (!Array.isArray(schema.cases) || schema.cases.length === 0) {
    errors.push("Schema cases must be a non-empty array");
  }
  if (!Array.isArray(schema.forbiddenStrings) || !schema.forbiddenStrings.includes("seed_dev_fixture")) {
    errors.push("Schema forbiddenStrings must include seed_dev_fixture");
  }

  const expectedCases = new Set(schema.expectedCases ?? []);
  const actualSchemaCases = [];
  for (const contract of schema.cases ?? []) {
    actualSchemaCases.push(contract.name);
    if (!expectedCases.has(contract.name)) {
      errors.push(`Schema case ${contract.name ?? "(missing name)"} is not listed in expectedCases`);
    }
    if (!Array.isArray(contract.paths) || contract.paths.length === 0) {
      errors.push(`Schema case ${contract.name ?? "(missing name)"} must include path checks`);
    }
  }
  if (!compareLists(actualSchemaCases, schema.expectedCases ?? [])) {
    errors.push("Schema cases must match expectedCases exactly and in order");
  }

  return errors;
}

function validateFixture(fixture, schema) {
  const errors = [];
  const schemaErrors = validateSchema(schema);

  if (schemaErrors.length > 0) {
    errors.push(...schemaErrors.map((error) => `${schemaLabel}: ${error}`));
    return errors;
  }

  if (!fixture || typeof fixture !== "object" || Array.isArray(fixture)) {
    return ["Fixture must be a JSON object"];
  }
  if (fixture.version !== schema.fixtureMetadata.version) {
    errors.push(`Fixture version must be ${schema.fixtureMetadata.version}`);
  }
  if (fixture.owner !== schema.fixtureMetadata.owner) {
    errors.push(`Fixture owner must be ${schema.fixtureMetadata.owner}`);
  }
  if (!fixture.cases || typeof fixture.cases !== "object" || Array.isArray(fixture.cases)) {
    errors.push("Fixture cases must be an object");
    return errors;
  }

  const actualCases = Object.keys(fixture.cases);
  if (!compareLists(actualCases, schema.expectedCases)) {
    errors.push(`Fixture case names must match schema expectedCases exactly. Found: ${actualCases.join(", ")}`);
  }

  const fixtureText = JSON.stringify(fixture);
  for (const forbidden of schema.forbiddenStrings) {
    if (fixtureText.includes(forbidden)) {
      errors.push(`Fixture must not contain debug-only string ${forbidden}`);
    }
  }

  for (const contract of schema.cases) {
    const caseValue = fixture.cases[contract.name];
    if (caseValue === undefined) {
      errors.push(`Missing fixture case ${contract.name}`);
      continue;
    }

    for (const rule of contract.paths) {
      const pathParts = rule.path;
      if (!Array.isArray(pathParts) || pathParts.length === 0) {
        errors.push(`${contract.name}: path rule must include a non-empty path array`);
        continue;
      }

      const located = getPath(caseValue, pathParts);
      const label = `${contract.name}${pathLabel(pathParts)}`;
      if (!located.found) {
        errors.push(`Missing required contract path ${label}`);
        continue;
      }

      if (!matchesType(located.value, rule.type)) {
        errors.push(`Expected ${label} to be ${rule.type}, got ${valueType(located.value)}`);
        continue;
      }

      if (Array.isArray(rule.enum) && !rule.enum.includes(located.value)) {
        errors.push(`Expected ${label} to be one of ${rule.enum.join(", ")}`);
      }
      if (Object.prototype.hasOwnProperty.call(rule, "exact") && located.value !== rule.exact) {
        errors.push(`Expected ${label} to equal ${JSON.stringify(rule.exact)}`);
      }
      if (typeof rule.minItems === "number" && located.value.length < rule.minItems) {
        errors.push(`Expected ${label} to have at least ${rule.minItems} item(s)`);
      }
      if (typeof rule.maxItems === "number" && located.value.length > rule.maxItems) {
        errors.push(`Expected ${label} to have at most ${rule.maxItems} item(s)`);
      }
      if (typeof rule.pattern === "string" && !new RegExp(rule.pattern).test(located.value)) {
        errors.push(`Expected ${label} to match ${rule.pattern}`);
      }
    }
  }

  return errors;
}

function validateReceipt(receipt, fixture, schema) {
  const errors = [];

  if (!receipt || typeof receipt !== "object" || Array.isArray(receipt)) {
    return ["Receipt must be a JSON object"];
  }

  const expectedReceipt = buildReceipt(fixture, schema);
  const requiredFields = [
    ["version"],
    ["kind"],
    ["status"],
    ["checker", "path"],
    ["checker", "command"],
    ["fixture", "path"],
    ["fixture", "sha256"],
    ["fixture", "version"],
    ["fixture", "owner"],
    ["schema", "path"],
    ["schema", "sha256"],
    ["schema", "version"],
    ["schema", "kind"],
    ["schema", "scope"],
    ["schema", "expectedCases"],
    ["schema", "forbiddenStrings"],
    ["sourceInputs"],
  ];

  for (const fieldPath of requiredFields) {
    const expected = getPath(expectedReceipt, fieldPath).value;
    validateReceiptField(errors, receipt, fieldPath, expected);
  }

  return errors;
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function expectRejected(name, fixture, schema) {
  if (validateFixture(fixture, schema).length === 0) {
    fail(scriptLabel, `Guardrail did not reject: ${name}`);
  }
}

function expectSchemaRejected(name, schema) {
  if (validateSchema(schema).length === 0) {
    fail(scriptLabel, `Guardrail did not reject schema mutation: ${name}`);
  }
}

function expectReceiptRejected(name, receipt, fixture, schema) {
  if (typeof validateReceipt !== "function") {
    fail(scriptLabel, `Receipt guardrail is not implemented: ${name}`);
    return;
  }
  if (validateReceipt(receipt, fixture, schema).length === 0) {
    fail(scriptLabel, `Guardrail did not reject receipt mutation: ${name}`);
  }
}

function findSourceInput(receipt, sourcePath) {
  return receipt.sourceInputs.find((input) => input.path === sourcePath);
}

function expectSourceInputCovered(sourcePath, label, replacementHash, validReceipt, fixture, schema) {
  const staleReceipt = clone(validReceipt);
  const sourceInput = findSourceInput(staleReceipt, sourcePath);
  if (!sourceInput) {
    fail(scriptLabel, `Self-test cannot mutate missing ${label} source-input hash`);
  } else {
    sourceInput.sha256 = replacementHash;
    expectReceiptRejected(`stale ${label} source-input hash`, staleReceipt, fixture, schema);
  }

  const missingReceipt = clone(validReceipt);
  missingReceipt.sourceInputs = missingReceipt.sourceInputs.filter(
    (input) => input.path !== sourcePath,
  );
  expectReceiptRejected(`missing ${label} source input`, missingReceipt, fixture, schema);
}

function runSelfTests(fixture, schema) {
  const missingCase = clone(fixture);
  delete missingCase.cases["desktop_snapshot.with_setup_evidence"];
  expectRejected("missing fixture case", missingCase, schema);

  const missingField = clone(fixture);
  delete missingField.cases["desktop_snapshot.empty"].commandSurface.ready;
  expectRejected("missing required commandSurface.ready", missingField, schema);

  const badPrimitive = clone(fixture);
  badPrimitive.cases["desktop_snapshot.with_setup_evidence"].setupGuidance.ollama.availability = 42;
  expectRejected("bad primitive at setupGuidance.ollama.availability", badPrimitive, schema);

  const unsupportedKindDrift = clone(fixture);
  unsupportedKindDrift.cases["desktop_snapshot.unsupported_whisper_model"].model.kind = "untested";
  expectRejected("unsupported snapshot kind drift", unsupportedKindDrift, schema);

  const unsupportedPathEvidenceLeak = clone(fixture);
  unsupportedPathEvidenceLeak.cases["desktop_snapshot.unsupported_whisper_model"].setupGuidance.whisper.lastPathTest = {
    testedPath: "<app-root>/notes.txt",
    testedAtMs: 1_700_000_001_000,
    state: "Invalid",
    fileSizeBytes: null,
    sha256: null,
    failureDetail: "Unsupported Whisper model file extension.",
  };
  expectRejected("unsupported snapshot path-test evidence leak", unsupportedPathEvidenceLeak, schema);

  const debugLeak = clone(fixture);
  debugLeak.cases["desktop_snapshot.empty"].commandSurface.detail = "seed_dev_fixture";
  expectRejected("debug-only command string in fixture", debugLeak, schema);

  const hostedCandidate = clone(fixture);
  hostedCandidate.cases["desktop_snapshot.empty"].modelSetupOptions.ollama.candidates.push({
    id: "hosted-deepseek-v3-2-speciale",
    displayName: "DeepSeek V3.2 Speciale",
    modelTag: "DeepSeek-V3.2-Speciale",
    pullCommand: "ollama pull DeepSeek-V3.2-Speciale",
    defaultCandidate: false,
    setupNotes: "Hosted model must not appear in local setup options.",
  });
  expectRejected("hosted model candidate in manual setup fixture", hostedCandidate, schema);

  const missingSchemaCase = clone(schema);
  missingSchemaCase.cases = missingSchemaCase.cases.filter(
    (contract) => contract.name !== "desktop_snapshot.transcribed_analyzed_meeting",
  );
  expectSchemaRejected("missing schema case block", missingSchemaCase);

  const validReceipt = buildReceipt(fixture, schema);

  const staleFixtureHashReceipt = clone(validReceipt);
  staleFixtureHashReceipt.fixture.sha256 = "0".repeat(64);
  expectReceiptRejected("stale fixture hash", staleFixtureHashReceipt, fixture, schema);

  const wrongStatusReceipt = clone(validReceipt);
  wrongStatusReceipt.status = "failed";
  expectReceiptRejected("wrong receipt status", wrongStatusReceipt, fixture, schema);

  const missingExpectedCaseReceipt = clone(validReceipt);
  missingExpectedCaseReceipt.schema.expectedCases = missingExpectedCaseReceipt.schema.expectedCases.filter(
    (name) => name !== "desktop_snapshot.with_setup_evidence",
  );
  expectReceiptRejected("missing expected case", missingExpectedCaseReceipt, fixture, schema);

  const wrongCheckerCommandReceipt = clone(validReceipt);
  wrongCheckerCommandReceipt.checker.command = "node scripts/check-desktop-command-view-contract.js";
  expectReceiptRejected("wrong checker command", wrongCheckerCommandReceipt, fixture, schema);

  const wrongForbiddenStringsReceipt = clone(validReceipt);
  wrongForbiddenStringsReceipt.schema.forbiddenStrings = [];
  expectReceiptRejected("wrong forbidden strings", wrongForbiddenStringsReceipt, fixture, schema);

  expectSourceInputCovered(
    "apps/desktop/src-tauri/src/main.rs",
    "Rust producer",
    "0".repeat(64),
    validReceipt,
    fixture,
    schema,
  );
  expectSourceInputCovered(
    "apps/desktop/src-tauri/src/calendar.rs",
    "calendar producer",
    "3".repeat(64),
    validReceipt,
    fixture,
    schema,
  );
  expectSourceInputCovered(
    "apps/desktop/src/commandAdapter.ts",
    "TypeScript command facade",
    "1".repeat(64),
    validReceipt,
    fixture,
    schema,
  );
  expectSourceInputCovered(
    "apps/desktop/src/desktopContract.ts",
    "TypeScript runtime contract validator",
    "4".repeat(64),
    validReceipt,
    fixture,
    schema,
  );
  expectSourceInputCovered(
    "apps/desktop/src/commandAdapter.contract.test.ts",
    "TypeScript contract-test",
    "2".repeat(64),
    validReceipt,
    fixture,
    schema,
  );

  const missingSourceInputsReceipt = clone(validReceipt);
  delete missingSourceInputsReceipt.sourceInputs;
  expectReceiptRejected("missing source-input receipt section", missingSourceInputsReceipt, fixture, schema);
}

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  console.log(
    [
      "Usage: node scripts/check-desktop-command-view-contract.js",
      "       node scripts/check-desktop-command-view-contract.js --write-artifact",
      `       node scripts/check-desktop-command-view-contract.js --check-artifact ${receiptLabel}`,
      "       node scripts/check-desktop-command-view-contract.js --self-test",
      "",
      `--write-artifact writes ${receiptLabel} after validation passes.`,
      "--check-artifact validates an existing receipt against the current fixture, schema, and source inputs.",
      "--self-test runs checker mutation self-tests without reading or writing receipt artifacts.",
    ].join("\n"),
  );
  process.exit(ok ? 0 : 1);
}

if (!ok) {
  process.exit(1);
}

const fixture = readJson(fixturePath, fixtureLabel);
const schema = readJson(schemaPath, schemaLabel);

if (fixture && schema) {
  runSelfTests(fixture, schema);

  for (const error of validateFixture(fixture, schema)) {
    const label = error.startsWith(`${schemaLabel}:`) ? schemaLabel : fixtureLabel;
    fail(label, error.replace(`${schemaLabel}: `, ""));
  }
}

if (!ok) {
  process.exit(1);
}

if (options.selfTest) {
  console.log("Desktop command/view contract checker self-test passed.");
  process.exit(0);
}

if (options.writeArtifact) {
  writeReceipt(fixture, schema);
}

if (options.checkArtifact) {
  const artifactPath = path.isAbsolute(options.checkArtifact)
    ? options.checkArtifact
    : path.resolve(process.cwd(), options.checkArtifact);
  const receipt = readJson(artifactPath, options.checkArtifact);

  if (receipt && fixture && schema) {
    for (const error of validateReceipt(receipt, fixture, schema)) {
      fail(options.checkArtifact, error);
    }
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Desktop command/view contract shape gate passed.");
