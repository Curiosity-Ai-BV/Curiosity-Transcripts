#!/usr/bin/env node
const { execFileSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "..");
const desktopDir = path.join(repoRoot, "apps", "desktop");
const outputDir = path.join(repoRoot, "release-artifacts", "supply-chain");
const releaseRustTarget = "aarch64-apple-darwin";

const npmSbomPath = path.join(outputDir, "desktop-npm-cyclonedx-sbom.json");
const npmLicensePath = path.join(outputDir, "desktop-npm-lock-license-metadata.json");
const rootCargoLicensePath = path.join(
  outputDir,
  `root-cargo-${releaseRustTarget}-license-metadata.json`,
);
const desktopCargoLicensePath = path.join(
  outputDir,
  `desktop-tauri-cargo-${releaseRustTarget}-license-metadata.json`,
);

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
}

function writeJson(file, value) {
  fs.writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function hasText(value) {
  return typeof value === "string" && value.trim().length > 0;
}

function derivePackageName(lockPath) {
  const parts = lockPath.split("node_modules/").filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1].replace(/\/$/, "") : lockPath;
}

function validateNpmLockfile() {
  const lockfilePath = path.join(desktopDir, "package-lock.json");
  const lock = JSON.parse(fs.readFileSync(lockfilePath, "utf8"));

  if (!lock.packages || typeof lock.packages !== "object") {
    throw new Error("apps/desktop/package-lock.json must use npm lockfile packages metadata");
  }

  const packageRows = [];
  const missing = [];

  for (const [lockPath, metadata] of Object.entries(lock.packages)) {
    if (!lockPath.startsWith("node_modules/")) {
      continue;
    }

    const license = metadata.license;
    const licenseFile = metadata.licenseFile ?? metadata.license_file;
    const row = {
      lockPath,
      name: derivePackageName(lockPath),
      version: metadata.version ?? null,
      license: license ?? null,
      licenseFile: licenseFile ?? null,
      resolved: metadata.resolved ?? null,
    };

    packageRows.push(row);

    if (!hasText(license) && !hasText(licenseFile)) {
      missing.push(row);
    }
  }

  if (missing.length > 0) {
    const details = missing
      .map((pkg) => `${pkg.lockPath} ${pkg.name}@${pkg.version ?? "unknown-version"}`)
      .join("\n");
    throw new Error(`npm packages missing license or license-file metadata:\n${details}`);
  }

  return {
    source: "apps/desktop/package-lock.json",
    lockfileVersion: lock.lockfileVersion,
    checkedPackageCount: packageRows.length,
    packages: packageRows,
  };
}

function normalizedNpmSbom() {
  const sbom = JSON.parse(
    run("npm", ["sbom", "--sbom-format", "cyclonedx", "--sbom-type", "application"], {
      cwd: desktopDir,
    }),
  );

  delete sbom.serialNumber;
  if (sbom.metadata && typeof sbom.metadata === "object") {
    delete sbom.metadata.timestamp;
  }

  return sbom;
}

function loadCargoMetadata(args) {
  return JSON.parse(run("cargo", args));
}

function relativeRepoPath(filePath) {
  if (!hasText(filePath)) {
    return null;
  }

  const relative = path.relative(repoRoot, filePath);
  return !relative.startsWith("..") && !path.isAbsolute(relative) ? relative : null;
}

function cargoPackageSource(pkg) {
  if (hasText(pkg.source)) {
    return pkg.source;
  }

  const manifestPath = relativeRepoPath(pkg.manifest_path);
  return manifestPath ? `path:${manifestPath}` : "path";
}

function cargoLicenseReport(metadata, label) {
  const missing = metadata.packages.filter(
    (pkg) => !hasText(pkg.license) && !hasText(pkg.license_file),
  );

  if (missing.length > 0) {
    const details = missing
      .map((pkg) => {
        const source = pkg.source ?? `path:${pkg.manifest_path}`;
        return `${pkg.id} (${pkg.name}@${pkg.version}, ${source})`;
      })
      .join("\n");
    throw new Error(`${label} Cargo packages missing license or license_file metadata:\n${details}`);
  }

  const packages = metadata.packages
    .map((pkg) => ({
      name: pkg.name,
      version: pkg.version,
      source: cargoPackageSource(pkg),
      license: pkg.license ?? null,
      licenseFile: pkg.license_file ?? null,
      manifestPath: relativeRepoPath(pkg.manifest_path),
    }))
    .sort((left, right) =>
      [left.name, left.version, left.source].join("\0").localeCompare(
        [right.name, right.version, right.source].join("\0"),
      ),
    );

  return {
    source: label,
    releaseRustTarget,
    cargoMetadataCommand: label === "root workspace"
      ? `cargo metadata --locked --format-version 1 --filter-platform ${releaseRustTarget}`
      : `cargo metadata --manifest-path apps/desktop/src-tauri/Cargo.toml --locked --format-version 1 --filter-platform ${releaseRustTarget}`,
    checkedPackageCount: packages.length,
    packages,
  };
}

fs.rmSync(outputDir, { recursive: true, force: true });
fs.mkdirSync(outputDir, { recursive: true });

const npmLicenseReport = validateNpmLockfile();
writeJson(npmLicensePath, npmLicenseReport);
writeJson(npmSbomPath, normalizedNpmSbom());

const rootCargoMetadata = loadCargoMetadata([
  "metadata",
  "--locked",
  "--format-version",
  "1",
  "--filter-platform",
  releaseRustTarget,
]);
writeJson(rootCargoLicensePath, cargoLicenseReport(rootCargoMetadata, "root workspace"));

const desktopCargoMetadata = loadCargoMetadata([
  "metadata",
  "--manifest-path",
  "apps/desktop/src-tauri/Cargo.toml",
  "--locked",
  "--format-version",
  "1",
  "--filter-platform",
  releaseRustTarget,
]);
writeJson(desktopCargoLicensePath, cargoLicenseReport(desktopCargoMetadata, "desktop Tauri"));

console.log("Generated supply-chain artifacts:");
console.log(`- ${path.relative(repoRoot, npmSbomPath)}`);
console.log(`- ${path.relative(repoRoot, npmLicensePath)}`);
console.log(`- ${path.relative(repoRoot, rootCargoLicensePath)}`);
console.log(`- ${path.relative(repoRoot, desktopCargoLicensePath)}`);
