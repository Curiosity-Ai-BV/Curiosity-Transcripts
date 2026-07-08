#!/usr/bin/env node
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "..");
const defaultArtifactDir = path.join(repoRoot, "release-artifacts", "supply-chain");
const scriptLabel = "scripts/check-supply-chain-artifacts.js";
const releaseRustTarget = "aarch64-apple-darwin";

const expectedFiles = [
  "desktop-npm-cyclonedx-sbom.json",
  "desktop-npm-lock-license-metadata.json",
  "root-cargo-aarch64-apple-darwin-license-metadata.json",
  "desktop-tauri-cargo-aarch64-apple-darwin-license-metadata.json",
];

const cargoArtifacts = [
  {
    file: "root-cargo-aarch64-apple-darwin-license-metadata.json",
    source: "root workspace",
    cargoMetadataCommand: "cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-darwin",
  },
  {
    file: "desktop-tauri-cargo-aarch64-apple-darwin-license-metadata.json",
    source: "desktop Tauri",
    cargoMetadataCommand:
      "cargo metadata --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --format-version 1 --filter-platform aarch64-apple-darwin",
  },
];

function hasText(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function readJsonArtifact(file) {
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (error) {
    return { error: `${path.basename(file)} could not be read: ${error.message}` };
  }

  if (text.trim().length === 0) {
    return { error: `${path.basename(file)} is empty` };
  }

  try {
    const value = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { error: `${path.basename(file)} must contain a JSON object` };
    }
    return { value };
  } catch (error) {
    return { error: `${path.basename(file)} is not parseable JSON: ${error.message}` };
  }
}

function validatePackageRows(label, metadata, requiredFields) {
  const errors = [];

  if (!Array.isArray(metadata.packages) || metadata.packages.length === 0) {
    errors.push(`${label} packages must be a non-empty array`);
    return errors;
  }

  if (metadata.checkedPackageCount !== metadata.packages.length) {
    errors.push(`${label} checkedPackageCount must equal packages.length`);
  }

  metadata.packages.forEach((pkg, index) => {
    if (!pkg || typeof pkg !== "object" || Array.isArray(pkg)) {
      errors.push(`${label} package row ${index + 1} must be an object`);
      return;
    }

    for (const field of requiredFields) {
      if (!hasText(pkg[field])) {
        errors.push(`${label} package row ${index + 1} must include non-empty ${field}`);
      }
    }

    if (!hasText(pkg.license) && !hasText(pkg.licenseFile)) {
      errors.push(`${label} package row ${index + 1} must include license or licenseFile`);
    }
  });

  return errors;
}

function validateNpmMetadata(metadata) {
  const errors = [];

  if (metadata.source !== "apps/desktop/package-lock.json") {
    errors.push("npm license metadata source must be apps/desktop/package-lock.json");
  }

  errors.push(...validatePackageRows("npm license metadata", metadata, ["lockPath", "name", "version"]));
  return errors;
}

function validateCargoMetadata(metadata, expected) {
  const errors = [];

  if (metadata.releaseRustTarget !== releaseRustTarget) {
    errors.push(`Cargo license metadata releaseRustTarget must be ${releaseRustTarget}`);
  }
  if (metadata.source !== expected.source) {
    errors.push(`Cargo license metadata source must be ${expected.source}`);
  }
  if (metadata.cargoMetadataCommand !== expected.cargoMetadataCommand) {
    errors.push(`Cargo license metadata command must be ${expected.cargoMetadataCommand}`);
  }

  errors.push(...validatePackageRows("Cargo license metadata", metadata, ["name", "version", "source"]));
  return errors;
}

function validateNpmSbom(sbom) {
  const errors = [];

  if (hasOwn(sbom, "serialNumber")) {
    errors.push("npm SBOM must not include top-level serialNumber");
  }
  if (hasOwn(sbom, "metadata")) {
    if (!sbom.metadata || typeof sbom.metadata !== "object" || Array.isArray(sbom.metadata)) {
      errors.push("npm SBOM metadata must be an object when present");
    } else if (hasOwn(sbom.metadata, "timestamp")) {
      errors.push("npm SBOM must not include metadata.timestamp");
    }
  }
  if (sbom.bomFormat !== "CycloneDX") {
    errors.push("npm SBOM bomFormat must be CycloneDX");
  }
  if (!Array.isArray(sbom.components) || sbom.components.length === 0) {
    errors.push("npm SBOM components must be a non-empty array");
  }
  if (!Array.isArray(sbom.dependencies)) {
    errors.push("npm SBOM dependencies must be an array");
  }

  return errors;
}

function validateArtifacts(artifactDir) {
  const errors = [];

  if (!fs.existsSync(artifactDir)) {
    return [`Missing supply-chain artifact directory: ${artifactDir}`];
  }
  if (!fs.statSync(artifactDir).isDirectory()) {
    return [`Supply-chain artifact path must be a directory: ${artifactDir}`];
  }

  const actualEntries = fs
    .readdirSync(artifactDir, { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name));
  const actualFileSet = new Set(actualEntries.filter((entry) => entry.isFile()).map((entry) => entry.name));
  const actualEntrySet = new Set(actualEntries.map((entry) => entry.name));
  const expectedFileSet = new Set(expectedFiles);

  for (const expected of expectedFiles) {
    if (!actualEntrySet.has(expected)) {
      errors.push(`Missing expected artifact: ${expected}`);
    } else if (!actualFileSet.has(expected)) {
      errors.push(`Expected supply-chain artifact must be a file: ${expected}`);
    }
  }

  for (const actual of actualEntrySet) {
    if (!expectedFileSet.has(actual)) {
      errors.push(`Unexpected supply-chain artifact: ${actual}`);
    }
  }

  const parsed = new Map();
  for (const file of expectedFiles) {
    if (!actualFileSet.has(file)) {
      continue;
    }
    const result = readJsonArtifact(path.join(artifactDir, file));
    if (result.error) {
      errors.push(result.error);
    } else {
      parsed.set(file, result.value);
    }
  }

  const npmSbom = parsed.get("desktop-npm-cyclonedx-sbom.json");
  if (npmSbom) {
    errors.push(...validateNpmSbom(npmSbom));
  }

  const npmMetadata = parsed.get("desktop-npm-lock-license-metadata.json");
  if (npmMetadata) {
    errors.push(...validateNpmMetadata(npmMetadata));
  }

  for (const cargoArtifact of cargoArtifacts) {
    const metadata = parsed.get(cargoArtifact.file);
    if (metadata) {
      errors.push(...validateCargoMetadata(metadata, cargoArtifact));
    }
  }

  return errors;
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function baseNpmMetadata() {
  return {
    source: "apps/desktop/package-lock.json",
    checkedPackageCount: 1,
    packages: [
      {
        lockPath: "node_modules/example",
        name: "example",
        version: "1.0.0",
        license: "MIT",
        licenseFile: null,
      },
    ],
  };
}

function baseCargoMetadata(source, cargoMetadataCommand) {
  return {
    source,
    releaseRustTarget,
    cargoMetadataCommand,
    checkedPackageCount: 1,
    packages: [
      {
        name: "example",
        version: "1.0.0",
        source: "registry+https://github.com/rust-lang/crates.io-index",
        license: "MIT",
        licenseFile: null,
      },
    ],
  };
}

function baseSbom() {
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    version: 1,
    metadata: {
      component: {
        name: "curiosity-transcripts-desktop",
        version: "0.0.0",
      },
    },
    components: [
      {
        type: "library",
        name: "example",
        version: "1.0.0",
      },
    ],
    dependencies: [],
  };
}

function writeValidArtifacts(dir) {
  fs.mkdirSync(dir, { recursive: true });
  writeJson(path.join(dir, "desktop-npm-cyclonedx-sbom.json"), baseSbom());
  writeJson(path.join(dir, "desktop-npm-lock-license-metadata.json"), baseNpmMetadata());
  for (const artifact of cargoArtifacts) {
    writeJson(
      path.join(dir, artifact.file),
      baseCargoMetadata(artifact.source, artifact.cargoMetadataCommand),
    );
  }
}

function runSelfTests() {
  const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "supply-chain-artifacts-"));

  try {
    const artifactFilePath = path.join(tempRoot, "artifact-path-is-file");
    fs.writeFileSync(artifactFilePath, "not a directory");
    const artifactFileErrors = validateArtifacts(artifactFilePath);
    if (!artifactFileErrors.some((error) => error.includes("must be a directory"))) {
      fail(scriptLabel, "Self-test did not reject artifact path that is a file");
    }

    function withArtifacts(name, mutate, expectedMessage) {
      const dir = path.join(tempRoot, name);
      writeValidArtifacts(dir);
      mutate(dir);
      const errors = validateArtifacts(dir);
      if (!errors.some((error) => error.includes(expectedMessage))) {
        fail(scriptLabel, `Self-test did not reject ${name}`);
      }
    }

    withArtifacts("missing-artifact", (dir) => {
      fs.rmSync(path.join(dir, "desktop-npm-cyclonedx-sbom.json"));
    }, "Missing expected artifact");

    withArtifacts("extra-artifact", (dir) => {
      writeJson(path.join(dir, "extra.json"), {});
    }, "Unexpected supply-chain artifact");

    withArtifacts("extra-artifact-directory", (dir) => {
      fs.mkdirSync(path.join(dir, "nested-extra"));
    }, "Unexpected supply-chain artifact");

    withArtifacts("invalid-json", (dir) => {
      fs.writeFileSync(path.join(dir, "desktop-npm-lock-license-metadata.json"), "{");
    }, "is not parseable JSON");

    withArtifacts("empty-json", (dir) => {
      fs.writeFileSync(path.join(dir, "desktop-npm-lock-license-metadata.json"), "");
    }, "is empty");

    withArtifacts("npm-package-missing-license", (dir) => {
      const metadata = baseNpmMetadata();
      metadata.packages[0].license = null;
      metadata.packages[0].licenseFile = null;
      writeJson(path.join(dir, "desktop-npm-lock-license-metadata.json"), metadata);
    }, "must include license or licenseFile");

    withArtifacts("cargo-wrong-target", (dir) => {
      const metadata = baseCargoMetadata(
        cargoArtifacts[0].source,
        cargoArtifacts[0].cargoMetadataCommand,
      );
      metadata.releaseRustTarget = "x86_64-apple-darwin";
      writeJson(path.join(dir, cargoArtifacts[0].file), metadata);
    }, "releaseRustTarget");

    withArtifacts("sbom-serial-number", (dir) => {
      const sbom = baseSbom();
      sbom.serialNumber = "urn:uuid:00000000-0000-0000-0000-000000000000";
      writeJson(path.join(dir, "desktop-npm-cyclonedx-sbom.json"), sbom);
    }, "serialNumber");

    withArtifacts("sbom-timestamp", (dir) => {
      const sbom = baseSbom();
      sbom.metadata.timestamp = "2026-07-09T00:00:00.000Z";
      writeJson(path.join(dir, "desktop-npm-cyclonedx-sbom.json"), sbom);
    }, "metadata.timestamp");

    const validDir = path.join(tempRoot, "valid");
    writeValidArtifacts(validDir);
    const validErrors = validateArtifacts(validDir);
    if (validErrors.length > 0) {
      fail(scriptLabel, `Self-test rejected valid artifacts: ${validErrors.join("; ")}`);
    }
  } finally {
    fs.rmSync(tempRoot, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const options = {
    selfTest: false,
    help: false,
    artifactDir: defaultArtifactDir,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--self-test") {
      options.selfTest = true;
    } else if (arg === "--artifact-dir") {
      const value = argv[index + 1];
      if (!value) {
        fail(scriptLabel, "--artifact-dir requires a path");
      } else {
        options.artifactDir = path.resolve(value);
        index += 1;
      }
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
      "Usage: node scripts/check-supply-chain-artifacts.js",
      "       node scripts/check-supply-chain-artifacts.js --self-test",
      "",
      "Checks generated supply-chain artifacts before CI upload.",
    ].join("\n"),
  );
  process.exit(ok ? 0 : 1);
}

if (options.selfTest) {
  runSelfTests();
  if (!ok) {
    process.exit(1);
  }
  console.log("Supply-chain artifact checker self-tests passed.");
  process.exit(0);
}

for (const error of validateArtifacts(options.artifactDir)) {
  fail(path.relative(repoRoot, options.artifactDir), error);
}

if (!ok) {
  process.exit(1);
}

console.log("Supply-chain artifacts passed metadata validation.");
