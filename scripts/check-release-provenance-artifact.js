#!/usr/bin/env node
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const scriptLabel = "scripts/check-release-provenance-artifact.js";
const releaseKind = "curiosity-transcripts-release-provenance";
const pagesKind = "curiosity-transcripts-pages-latest-provenance";
const appName = "Curiosity Transcripts";
const sha256Pattern = /^[0-9a-f]{64}$/i;
const gitShaPattern = /^[0-9a-f]{40}$/i;
const semverPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

const requiredRefFields = ["github_ref_name", "github_sha", "github_ref"];
const requiredVerificationFields = [
  "hdiutil_verify",
  "stapler_validation",
  "dmg_gatekeeper_assessment",
  "readonly_attach_app_presence",
  "app_codesign_verification",
  "app_gatekeeper_assessment",
];

function hasText(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function isObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function readJson(file) {
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (error) {
    return { errors: [`provenance file could not be read: ${error.message}`] };
  }

  try {
    const value = JSON.parse(text);
    if (!isObject(value)) {
      return { errors: ["provenance JSON must contain an object"] };
    }
    return { value };
  } catch (error) {
    return { errors: [`invalid JSON: ${error.message}`] };
  }
}

function requireObject(errors, value, field) {
  if (!isObject(value)) {
    errors.push(`${field} must be an object`);
    return false;
  }
  return true;
}

function requireTextField(errors, object, field, label = field) {
  if (!hasText(object[field])) {
    errors.push(`${label} must be a non-empty string`);
  }
}

function validateRefBlock(errors, provenance, field) {
  if (!requireObject(errors, provenance[field], field)) {
    return;
  }
  for (const refField of requiredRefFields) {
    requireTextField(errors, provenance[field], refField, `${field}.${refField}`);
  }
}

function validateVerification(errors, provenance) {
  if (!requireObject(errors, provenance.verification, "verification")) {
    return;
  }

  for (const field of requiredVerificationFields) {
    if (provenance.verification[field] !== "passed") {
      errors.push(`verification.${field} must be passed`);
    }
  }
}

function validateChecksumSidecar(errors, provenancePath, expectedDmgName, expectedSha) {
  const sidecarPath = path.join(path.dirname(provenancePath), `${expectedDmgName}.sha256`);
  if (!fs.existsSync(sidecarPath)) {
    errors.push(`checksum sidecar ${path.basename(sidecarPath)} is required`);
    return;
  }

  const lines = fs
    .readFileSync(sidecarPath, "utf8")
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (lines.length === 0) {
    errors.push(`checksum sidecar ${path.basename(sidecarPath)} must not be empty`);
    return;
  }

  const parts = lines[0].split(/\s+/);
  const checksum = parts[0];
  const referencedPath = parts.slice(1).join(" ");
  if (!sha256Pattern.test(checksum)) {
    errors.push(`checksum sidecar ${path.basename(sidecarPath)} must start with a SHA-256 checksum`);
  }
  if (!hasText(referencedPath)) {
    errors.push(`checksum sidecar ${path.basename(sidecarPath)} must reference ${expectedDmgName}`);
  } else if (path.basename(referencedPath) !== expectedDmgName) {
    errors.push(`checksum sidecar reference must be ${expectedDmgName}`);
  }
  if (checksum !== expectedSha) {
    errors.push("asset.dmg_sha256 must match sibling checksum sidecar");
  }
}

function validateCommon(provenance) {
  const errors = [];

  if (provenance.schema_version !== 1) {
    errors.push("schema_version must be 1");
  }

  if (requireObject(errors, provenance.app, "app")) {
    if (provenance.app.name !== appName) {
      errors.push(`app.name must be ${appName}`);
    }
    requireTextField(errors, provenance.app, "version", "app.version");
    if (hasText(provenance.app.version) && !semverPattern.test(provenance.app.version)) {
      errors.push("app.version must be SemVer without a leading v");
    }
  }

  if (requireObject(errors, provenance.runner, "runner")) {
    if (provenance.runner.runner_architecture !== "arm64") {
      errors.push("runner.runner_architecture must be arm64");
    }
  }

  if (requireObject(errors, provenance.asset, "asset")) {
    requireTextField(errors, provenance.asset, "dmg_asset_name", "asset.dmg_asset_name");
    requireTextField(errors, provenance.asset, "dmg_asset_path", "asset.dmg_asset_path");
    if (!sha256Pattern.test(provenance.asset.dmg_sha256 ?? "")) {
      errors.push("asset.dmg_sha256 must be a 64-character SHA-256 hex digest");
    }
  }

  validateVerification(errors, provenance);
  return errors;
}

function validateReleaseShape(errors, provenance, provenancePath) {
  if (hasOwn(provenance, "pages_latest")) {
    errors.push("release provenance must not include pages_latest");
  }
  validateRefBlock(errors, provenance, "release");

  const version = provenance.app?.version;
  if (!hasText(version)) {
    return;
  }

  const expectedTag = `v${version}`;
  if (provenance.release?.github_sha && !gitShaPattern.test(provenance.release.github_sha)) {
    errors.push("release.github_sha must be a 40-character hex Git SHA");
  }
  if (provenance.release?.github_ref_name && provenance.release.github_ref_name !== expectedTag) {
    errors.push(`release.github_ref_name must be ${expectedTag}`);
  }
  if (provenance.release?.github_ref && provenance.release.github_ref !== `refs/tags/${expectedTag}`) {
    errors.push(`release.github_ref must be refs/tags/${expectedTag}`);
  }

  const expectedDmgName = `Curiosity-Transcripts-${version}-macos-aarch64.dmg`;
  const expectedProvenanceName = `Curiosity-Transcripts-${version}-macos-aarch64.provenance.json`;
  if (path.basename(provenancePath) !== expectedProvenanceName) {
    errors.push(`provenance filename must be ${expectedProvenanceName}`);
  }
  if (provenance.asset?.dmg_asset_name !== expectedDmgName) {
    errors.push(`asset.dmg_asset_name must be ${expectedDmgName}`);
  }
  if (path.basename(provenance.asset?.dmg_asset_path ?? "") !== expectedDmgName) {
    errors.push(`asset.dmg_asset_path must reference ${expectedDmgName}`);
  }
  if (sha256Pattern.test(provenance.asset?.dmg_sha256 ?? "")) {
    validateChecksumSidecar(errors, provenancePath, expectedDmgName, provenance.asset.dmg_sha256);
  }
}

function validatePagesShape(errors, provenance, provenancePath) {
  if (hasOwn(provenance, "release")) {
    errors.push("Pages latest provenance must not include release");
  }
  validateRefBlock(errors, provenance, "pages_latest");

  const expectedDmgName = "Curiosity-Transcripts-latest.dmg";
  const expectedProvenanceName = "Curiosity-Transcripts-latest.provenance.json";
  if (path.basename(provenancePath) !== expectedProvenanceName) {
    errors.push(`provenance filename must be ${expectedProvenanceName}`);
  }
  if (provenance.asset?.dmg_asset_name !== expectedDmgName) {
    errors.push(`asset.dmg_asset_name must be ${expectedDmgName}`);
  }
  if (path.basename(provenance.asset?.dmg_asset_path ?? "") !== expectedDmgName) {
    errors.push(`asset.dmg_asset_path must reference ${expectedDmgName}`);
  }
  if (sha256Pattern.test(provenance.asset?.dmg_sha256 ?? "")) {
    validateChecksumSidecar(errors, provenancePath, expectedDmgName, provenance.asset.dmg_sha256);
  }
}

function validateProvenance(provenancePath) {
  const parsed = readJson(provenancePath);
  if (parsed.errors) {
    return parsed.errors;
  }

  const provenance = parsed.value;
  const errors = validateCommon(provenance);

  if (provenance.kind === releaseKind) {
    validateReleaseShape(errors, provenance, provenancePath);
  } else if (provenance.kind === pagesKind) {
    validatePagesShape(errors, provenance, provenancePath);
  } else {
    errors.push(`kind must be ${releaseKind} or ${pagesKind}`);
  }

  return errors;
}

function baseReleaseProvenance() {
  return {
    kind: releaseKind,
    schema_version: 1,
    app: {
      name: appName,
      version: "1.2.3",
    },
    release: {
      github_ref_name: "v1.2.3",
      github_sha: "0123456789abcdef0123456789abcdef01234567",
      github_ref: "refs/tags/v1.2.3",
    },
    runner: {
      runner_architecture: "arm64",
    },
    asset: {
      dmg_asset_name: "Curiosity-Transcripts-1.2.3-macos-aarch64.dmg",
      dmg_asset_path: "release-assets/Curiosity-Transcripts-1.2.3-macos-aarch64.dmg",
      dmg_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    },
    verification: Object.fromEntries(requiredVerificationFields.map((field) => [field, "passed"])),
  };
}

function basePagesProvenance() {
  const provenance = baseReleaseProvenance();
  provenance.kind = pagesKind;
  provenance.pages_latest = provenance.release;
  delete provenance.release;
  provenance.asset.dmg_asset_name = "Curiosity-Transcripts-latest.dmg";
  provenance.asset.dmg_asset_path = "pages-download/Curiosity-Transcripts-latest.dmg";
  return provenance;
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeChecksumSidecar(provenancePath, dmgName, checksum) {
  fs.mkdirSync(path.dirname(provenancePath), { recursive: true });
  fs.writeFileSync(path.join(path.dirname(provenancePath), `${dmgName}.sha256`), `${checksum}  ${dmgName}\n`);
}

function expectSelfTest(name, mutate, expectedError, options = {}) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-provenance-self-test-"));
  try {
    const fixtureDir = path.join(tempDir, "release-assets");
    const provenancePath = path.join(
      fixtureDir,
      "Curiosity-Transcripts-1.2.3-macos-aarch64.provenance.json",
    );
    const fixture = baseReleaseProvenance();
    mutate({ fixture, provenancePath, tempDir });
    if (options.writeSidecar !== false && fixture.asset?.dmg_asset_name && fixture.asset?.dmg_sha256) {
      writeChecksumSidecar(provenancePath, fixture.asset.dmg_asset_name, fixture.asset.dmg_sha256);
    }
    if (!fs.existsSync(provenancePath)) {
      writeJson(provenancePath, fixture);
    }

    const errors = validateProvenance(provenancePath);
    const passed = expectedError
      ? errors.some((error) => error.includes(expectedError))
      : errors.length === 0;
    if (!passed) {
      throw new Error(
        `${name} expected ${expectedError ? `error containing "${expectedError}"` : "no errors"}, got: ${errors.join("; ")}`,
      );
    }
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

function expectPagesSelfTest(name, mutate, expectedError, options = {}) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-provenance-self-test-"));
  try {
    const provenancePath = path.join(tempDir, "pages-download", "Curiosity-Transcripts-latest.provenance.json");
    const fixture = basePagesProvenance();
    mutate({ fixture, provenancePath, tempDir });
    if (options.writeSidecar !== false && fixture.asset?.dmg_asset_name && fixture.asset?.dmg_sha256) {
      writeChecksumSidecar(provenancePath, fixture.asset.dmg_asset_name, fixture.asset.dmg_sha256);
    }
    if (!fs.existsSync(provenancePath)) {
      writeJson(provenancePath, fixture);
    }

    const errors = validateProvenance(provenancePath);
    const passed = expectedError
      ? errors.some((error) => error.includes(expectedError))
      : errors.length === 0;
    if (!passed) {
      throw new Error(
        `${name} expected ${expectedError ? `error containing "${expectedError}"` : "no errors"}, got: ${errors.join("; ")}`,
      );
    }
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

function runSelfTest() {
  expectSelfTest("valid release provenance", () => {}, null);
  expectSelfTest("invalid JSON", ({ provenancePath }) => {
    fs.mkdirSync(path.dirname(provenancePath), { recursive: true });
    fs.writeFileSync(provenancePath, "{not-json");
  }, "invalid JSON");
  expectSelfTest("wrong kind", ({ fixture }) => {
    fixture.kind = "wrong";
  }, "kind must be");
  expectSelfTest("missing schema_version", ({ fixture }) => {
    delete fixture.schema_version;
  }, "schema_version must be 1");
  expectSelfTest("missing app field", ({ fixture }) => {
    delete fixture.app;
  }, "app must be an object");
  expectSelfTest("missing release ref block", ({ fixture }) => {
    delete fixture.release;
  }, "release must be an object");
  expectSelfTest("missing release ref field", ({ fixture }) => {
    delete fixture.release.github_sha;
  }, "release.github_sha must be a non-empty string");
  expectSelfTest("malformed release SHA", ({ fixture }) => {
    fixture.release.github_sha = "abcdef";
  }, "release.github_sha must be a 40-character hex Git SHA");
  expectSelfTest("mismatched release tag", ({ fixture }) => {
    fixture.release.github_ref_name = "v1.2.4";
  }, "release.github_ref_name must be v1.2.3");
  expectSelfTest("mismatched release ref", ({ fixture }) => {
    fixture.release.github_ref = "refs/tags/v1.2.4";
  }, "release.github_ref must be refs/tags/v1.2.3");
  expectSelfTest("missing runner", ({ fixture }) => {
    delete fixture.runner;
  }, "runner must be an object");
  expectSelfTest("missing asset", ({ fixture }) => {
    delete fixture.asset;
  }, "asset must be an object");
  expectSelfTest("non-passed verification", ({ fixture }) => {
    fixture.verification.hdiutil_verify = "failed";
  }, "verification.hdiutil_verify must be passed");
  expectSelfTest("missing verification field", ({ fixture }) => {
    delete fixture.verification.stapler_validation;
  }, "verification.stapler_validation must be passed");
  expectSelfTest("malformed SHA-256", ({ fixture }) => {
    fixture.asset.dmg_sha256 = "not-a-sha";
  }, "asset.dmg_sha256");
  expectSelfTest(
    "missing release checksum sidecar",
    () => {},
    "checksum sidecar Curiosity-Transcripts-1.2.3-macos-aarch64.dmg.sha256 is required",
    { writeSidecar: false },
  );
  expectSelfTest("checksum mismatch", ({ provenancePath }) => {
    const sidecarPath = path.join(path.dirname(provenancePath), "Curiosity-Transcripts-1.2.3-macos-aarch64.dmg.sha256");
    fs.mkdirSync(path.dirname(sidecarPath), { recursive: true });
    fs.writeFileSync(sidecarPath, `${"b".repeat(64)}  Curiosity-Transcripts-1.2.3-macos-aarch64.dmg\n`);
  }, "asset.dmg_sha256 must match sibling checksum sidecar", { writeSidecar: false });
  expectSelfTest("checksum sidecar reference mismatch", ({ provenancePath }) => {
    const sidecarPath = path.join(path.dirname(provenancePath), "Curiosity-Transcripts-1.2.3-macos-aarch64.dmg.sha256");
    fs.mkdirSync(path.dirname(sidecarPath), { recursive: true });
    fs.writeFileSync(sidecarPath, `${"a".repeat(64)}  Curiosity-Transcripts-latest.dmg\n`);
  }, "checksum sidecar reference must be Curiosity-Transcripts-1.2.3-macos-aarch64.dmg", { writeSidecar: false });
  expectSelfTest("wrong runner arch", ({ fixture }) => {
    fixture.runner.runner_architecture = "x64";
  }, "runner.runner_architecture");
  expectSelfTest("wrong release asset name", ({ fixture }) => {
    fixture.asset.dmg_asset_name = "Curiosity-Transcripts-latest.dmg";
  }, "asset.dmg_asset_name");
  expectSelfTest("wrong release asset path", ({ fixture }) => {
    fixture.asset.dmg_asset_path = "release-assets/Curiosity-Transcripts-latest.dmg";
  }, "asset.dmg_asset_path");
  expectSelfTest("release/pages mismatch", ({ fixture }) => {
    fixture.pages_latest = fixture.release;
  }, "release provenance must not include pages_latest");

  expectPagesSelfTest("valid Pages provenance", () => {}, null);
  expectPagesSelfTest(
    "missing Pages checksum sidecar",
    () => {},
    "checksum sidecar Curiosity-Transcripts-latest.dmg.sha256 is required",
    { writeSidecar: false },
  );
  expectPagesSelfTest("wrong Pages asset name", ({ fixture }) => {
    fixture.asset.dmg_asset_name = "Curiosity-Transcripts-1.2.3-macos-aarch64.dmg";
  }, "asset.dmg_asset_name must be Curiosity-Transcripts-latest.dmg");
  expectPagesSelfTest("wrong Pages asset path", ({ fixture }) => {
    fixture.asset.dmg_asset_path = "pages-download/Curiosity-Transcripts-1.2.3-macos-aarch64.dmg";
  }, "asset.dmg_asset_path must reference Curiosity-Transcripts-latest.dmg");
  expectPagesSelfTest("missing Pages latest ref block", ({ fixture }) => {
    delete fixture.pages_latest;
  }, "pages_latest must be an object");
  expectPagesSelfTest("missing Pages latest ref field", ({ fixture }) => {
    delete fixture.pages_latest.github_ref;
  }, "pages_latest.github_ref must be a non-empty string");
  expectPagesSelfTest("Pages/release mismatch", ({ fixture }) => {
    fixture.release = fixture.pages_latest;
  }, "Pages latest provenance must not include release");

  console.log(`${scriptLabel}: self-test passed`);
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 1 && args[0] === "--self-test") {
    runSelfTest();
    return;
  }

  if (args.length !== 1) {
    console.error(`Usage: node ${scriptLabel} path/to/provenance.json`);
    console.error(`       node ${scriptLabel} --self-test`);
    process.exit(1);
  }

  const provenancePath = args[0];
  const errors = validateProvenance(provenancePath);
  for (const error of errors) {
    console.error(`::error file=${provenancePath}::${error}`);
  }
  process.exit(errors.length === 0 ? 0 : 1);
}

main();
