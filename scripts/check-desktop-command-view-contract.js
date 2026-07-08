const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const fixturePath = path.join(root, "apps", "desktop", "contracts", "desktop-command-view-contract.fixture.json");
const schemaPath = path.join(root, "apps", "desktop", "contracts", "desktop-command-view-contract.schema.json");
const fixtureLabel = "apps/desktop/contracts/desktop-command-view-contract.fixture.json";
const schemaLabel = "apps/desktop/contracts/desktop-command-view-contract.schema.json";
const scriptLabel = "scripts/check-desktop-command-view-contract.js";

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

const fixture = readJson(fixturePath, fixtureLabel);
const schema = readJson(schemaPath, schemaLabel);

if (fixture && schema) {
  const missingCase = clone(fixture);
  delete missingCase.cases["desktop_snapshot.with_setup_evidence"];
  expectRejected("missing fixture case", missingCase, schema);

  const missingField = clone(fixture);
  delete missingField.cases["desktop_snapshot.empty"].commandSurface.ready;
  expectRejected("missing required commandSurface.ready", missingField, schema);

  const badPrimitive = clone(fixture);
  badPrimitive.cases["desktop_snapshot.with_setup_evidence"].setupGuidance.ollama.availability = 42;
  expectRejected("bad primitive at setupGuidance.ollama.availability", badPrimitive, schema);

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

  for (const error of validateFixture(fixture, schema)) {
    const label = error.startsWith(`${schemaLabel}:`) ? schemaLabel : fixtureLabel;
    fail(label, error.replace(`${schemaLabel}: `, ""));
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Desktop command/view contract shape gate passed.");
