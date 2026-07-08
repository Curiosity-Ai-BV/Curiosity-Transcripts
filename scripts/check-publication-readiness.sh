#!/usr/bin/env bash
set -euo pipefail

failures=0

check_file() {
  local file="$1"
  if [[ ! -s "$file" ]]; then
    printf '::error file=%s::Missing required publication file\n' "$file" >&2
    failures=1
  fi
}

require_text() {
  local file="$1"
  local pattern="$2"
  local description="$3"

  if [[ ! -f "$file" ]] || ! grep -Eq "$pattern" "$file"; then
    printf '::error file=%s::Expected %s\n' "$file" "$description" >&2
    failures=1
  fi
}

require_normalized_text() {
  local file="$1"
  local expected="$2"
  local description="$3"

  if [[ ! -f "$file" ]] || ! CHECK_FILE="$file" CHECK_EXPECTED="$expected" node <<'NODE'
const fs = require("fs");

const normalize = (value) => value.replace(/\s+/g, " ").trim();
const file = process.env.CHECK_FILE;
const expected = normalize(process.env.CHECK_EXPECTED ?? "");
const actual = normalize(fs.readFileSync(file, "utf8"));

process.exit(actual.includes(expected) ? 0 : 1);
NODE
  then
    printf '::error file=%s::Expected %s\n' "$file" "$description" >&2
    failures=1
  fi
}

for file in LICENSE NOTICE ATTRIBUTION.md CONTRIBUTING.md SECURITY.md README.md; do
  check_file "$file"
done
check_file "site/index.html"
check_file "docs/at-rest-data-strategy.md"
check_file "docs/release-candidate-checklist.md"
check_file "scripts/generate-supply-chain-artifacts.js"
check_file "scripts/check-coverage-artifacts.js"
check_file "scripts/check-tauri-command-surface.js"
check_file "apps/desktop/contracts/desktop-command-view-contract.fixture.json"
check_file "apps/desktop/contracts/desktop-command-view-contract.schema.json"
check_file ".github/dependabot.yml"
check_file ".github/workflows/codeql.yml"
check_file ".github/workflows/pages.yml"
check_file ".github/workflows/release.yml"
check_file ".github/workflows/secret-scanning.yml"

require_text LICENSE 'Apache License' 'Apache-2.0 license text'
require_text NOTICE 'Curiosity Transcripts' 'Curiosity Transcripts attribution notice'
require_text ATTRIBUTION.md 'Apache-2\.0' 'Apache-2.0 attribution guidance'
require_text CONTRIBUTING.md 'cargo test --workspace' 'deterministic Rust test guidance'
require_text CONTRIBUTING.md 'cargo fmt --manifest-path apps/desktop/src-tauri/Cargo\.toml --check' 'desktop Rust formatting local gate'
require_text CONTRIBUTING.md 'cargo test --manifest-path apps/desktop/src-tauri/Cargo\.toml' 'desktop Rust test local gate'
require_text CONTRIBUTING.md 'cargo clippy --manifest-path apps/desktop/src-tauri/Cargo\.toml --all-targets -- -D warnings' 'desktop Rust clippy local gate'
require_text SECURITY.md 'Report' 'private vulnerability reporting guidance'
require_text README.md 'Apache-2\.0' 'license metadata'
require_text README.md 'ATTRIBUTION\.md' 'commercial attribution pointer'
require_text README.md 'SECURITY\.md' 'security policy pointer'
require_text README.md 'GitHub Pages' 'public homepage deployment documentation'
require_text README.md 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable Pages DMG download documentation'
require_text README.md 'Versioning Rules' 'versioning and GitHub Release documentation'
require_text README.md 'desktop app exposes JSON, Markdown, and SRT export' 'current desktop export format claim'
require_text README.md 'JSON remains the deterministic integration format' 'deterministic JSON integration export claim'
require_text README.md 'Imported local WAV workflow' 'implemented imported WAV workflow claim'
require_text README.md 'Transcript segment correction' 'implemented transcript correction workflow claim'
require_text README.md 'desktop-command-view-contract\.fixture\.json' 'checked-in desktop command/view contract fixture documentation'
require_text README.md 'docs/release-candidate-checklist\.md' 'release-candidate checklist link'
require_text README.md 'docs/at-rest-data-strategy\.md' 'at-rest data strategy link'
require_text README.md 'App-level encryption-at-rest and keychain-backed secret storage are not' 'current encryption/keychain non-implementation disclosure'
require_text README.md 'arm64-only' 'first public release architecture documentation'
require_text docs/production-readiness-roadmap.md 'check-tauri-security\.js' 'Tauri renderer CSP release gate documentation'
require_text docs/production-readiness-roadmap.md 'Current Phase 2A CSP status' 'Phase 2A CSP implementation status'
require_text docs/production-readiness-roadmap.md 'connect-src ipc: http://ipc\.localhost' 'Phase 2A local Tauri IPC CSP boundary'
require_text docs/production-readiness-roadmap.md 'unsafe-inline, unsafe-eval' 'Phase 2A unsafe CSP source rejection boundary'
require_text docs/production-readiness-roadmap.md 'Current Tauri command surface status' 'Tauri command surface status'
require_text docs/production-readiness-roadmap.md 'check-tauri-command-surface\.js' 'Tauri command surface publication gate documentation'
require_text docs/production-readiness-roadmap.md 'seed_dev_fixture' 'debug fixture command surface boundary'
require_text docs/production-readiness-roadmap.md 'Current Phase 2D at-rest/keychain status' 'Phase 2D at-rest/keychain status'
require_text docs/production-readiness-roadmap.md 'docs/at-rest-data-strategy\.md' 'Phase 2D at-rest strategy reference'
require_text docs/production-readiness-roadmap.md 'keychain-backed secret storage, migration/recovery support' 'Phase 2D implementation remains later work'
require_text docs/production-readiness-roadmap.md 'Current Phase 2E delete cleanup status' 'Phase 2E delete cleanup status'
require_text docs/production-readiness-roadmap.md 'delete intents for deleted or deleted-at meetings' 'Phase 2E pending delete intent finalization scope'
require_text docs/production-readiness-roadmap.md '`processing_jobs`' 'Phase 2E processing_jobs cleanup scope'
require_text docs/production-readiness-roadmap.md '`meeting_search` rows' 'Phase 2E meeting_search cleanup scope'
require_text docs/production-readiness-roadmap.md 'Job recovery skips deleted/deleted-at' 'Phase 2E deleted meeting job recovery guard'
require_text docs/production-readiness-roadmap.md 'First public release architecture: arm64-only macOS DMG' 'first public release architecture decision'
require_text docs/production-readiness-roadmap.md 'Current Phase 3B/3C/3D status: transcription and summary job start, cancel,' 'current durable transcription, summary, and feature-matrix status'
require_normalized_text docs/production-readiness-roadmap.md 'CI now gates no-Whisper desktop tests and the ScreenCaptureKit system-audio feature compile path on macOS' 'no-Whisper and ScreenCaptureKit feature-matrix CI status'
require_text docs/production-readiness-roadmap.md 'Real-hardware smoke and release confidence remain manual/later work' 'real hardware release confidence remains later work'
require_text docs/production-readiness-roadmap.md 'retryable jobs now surface retry UX' 'durable job retry UX implementation status'
require_text docs/production-readiness-roadmap.md 'Current Phase 5A status' 'Phase 5A command/view contract fixture status'
require_text docs/production-readiness-roadmap.md 'desktop-command-view-contract\.fixture\.json' 'Phase 5A checked-in contract fixture path'
require_text docs/production-readiness-roadmap.md 'desktop-command-view-contract\.schema\.json' 'Phase 5A checked-in contract shape artifact path'
require_text docs/production-readiness-roadmap.md 'Rust tests guard exact equality' 'Phase 5A Rust exact-equality guard'
require_text docs/production-readiness-roadmap.md 'TS command adapter contract tests consume the same fixture' 'Phase 5A TS command adapter contract consumption'
require_text docs/production-readiness-roadmap.md 'fixture-derived shape lock' 'Phase 5A fixture-derived schema boundary'
require_text docs/production-readiness-roadmap.md '`search_meetings` results are also validated' 'Phase 5A search result command contract boundary'
require_text docs/production-readiness-roadmap.md 'Current Phase 5B coverage artifact status' 'Phase 5B coverage artifact status'
require_text docs/production-readiness-roadmap.md 'release-artifacts/coverage' 'Phase 5B coverage artifact output path'
require_text docs/production-readiness-roadmap.md 'no global percentage threshold' 'Phase 5B non-percentage coverage boundary'
require_normalized_text docs/production-readiness-roadmap.md 'not generated DTOs or module splitting' 'Phase 5B generated DTO/module split boundary'
require_text docs/production-readiness-roadmap.md 'Current CodeQL code scanning status' 'current CodeQL code scanning status'
require_text docs/production-readiness-roadmap.md 'Current supply-chain artifact status' 'current supply-chain artifact status'
require_text docs/production-readiness-roadmap.md 'metadata/reporting gate' 'supply-chain metadata/reporting boundary'
require_text docs/production-readiness-roadmap.md 'legal license allowlist' 'non-allowlist supply-chain boundary'
require_text docs/production-readiness-roadmap.md 'Current secret scanning status' 'current secret scanning status'
require_text docs/production-readiness-roadmap.md 'ghcr\.io/gitleaks/gitleaks:v8\.30\.0' 'pinned Gitleaks CLI container documentation'
require_text docs/production-readiness-roadmap.md 'GITLEAKS_LICENSE' 'Gitleaks Action org-license boundary'
require_text docs/production-readiness-roadmap.md 'protection, and alert triage policy' 'secret scanning governance boundary'
require_text docs/production-readiness-roadmap.md 'branch-protection or alert triage policy' 'CodeQL policy boundary'
require_text docs/macos-dmg-release.md 'docs/release-candidate-checklist\.md' 'release-candidate checklist link from release docs'
require_text docs/release-candidate-checklist.md 'check-tauri-security\.js' 'Tauri renderer CSP release-candidate gate'
require_text docs/release-candidate-checklist.md 'check-tauri-command-surface\.js' 'Tauri command surface release-candidate gate'
require_text docs/release-candidate-checklist.md 'Clean-user install' 'clean-user install release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'macOS permissions' 'macOS permissions release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Model setup' 'model setup release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Offline-after-setup' 'offline-after-setup release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Recording' 'recording release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Import WAV' 'imported WAV release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Transcription' 'transcription release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Summary' 'summary release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'Export' 'export release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'durable job recovery' 'durable job recovery release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'desktop-command-view-contract\.fixture\.json' 'command/view contract fixture release-candidate expectation'
require_text docs/release-candidate-checklist.md 'desktop-command-view-contract\.schema\.json' 'command/view contract shape release-candidate expectation'
require_text docs/release-candidate-checklist.md 'node scripts/check-desktop-command-view-contract\.js' 'command/view contract shape checker release-candidate command'
require_text docs/release-candidate-checklist.md 'node scripts/check-coverage-artifacts\.js' 'coverage artifact checker release-candidate command'
require_text docs/release-candidate-checklist.md 'release-artifacts/coverage' 'coverage artifact release-candidate output path'
require_text docs/release-candidate-checklist.md 'apps/desktop/src/App\.tsx' 'frontend App coverage source-path expectation'
require_text docs/release-candidate-checklist.md 'apps/desktop/src/commandAdapter\.ts' 'frontend command adapter coverage source-path expectation'
require_text docs/release-candidate-checklist.md 'crates/store/src/lib\.rs' 'Rust store coverage source-path expectation'
require_text docs/release-candidate-checklist.md 'apps/desktop/src-tauri/src/main\.rs' 'desktop Tauri main coverage source-path expectation'
require_text docs/release-candidate-checklist.md 'no global percentage threshold' 'release-candidate non-percentage coverage boundary'
require_text docs/release-candidate-checklist.md 'Delete' 'delete release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'startup finalizes pending' 'pending delete restart cleanup release-candidate smoke expectation'
require_text docs/release-candidate-checklist.md 'user-owned exports' 'user-owned export delete boundary release-candidate smoke expectation'
require_text docs/release-candidate-checklist.md 'Uninstall and private-data handling' 'uninstall/private-data release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'JSON, Markdown, and SRT' 'current release-candidate export formats'
require_text docs/release-candidate-checklist.md 'At-rest disclosure' 'at-rest disclosure release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'encryption-at-rest is not implemented in v1' 'release notes at-rest encryption disclosure'
require_text docs/release-candidate-checklist.md 'CodeQL scans Rust and JavaScript/TypeScript' 'CodeQL release-candidate visibility expectation'
require_text docs/release-candidate-checklist.md 'branch-protection or alert triage policy' 'CodeQL policy boundary'
require_text docs/release-candidate-checklist.md 'node scripts/generate-supply-chain-artifacts\.js' 'supply-chain artifact release-candidate command'
require_text docs/release-candidate-checklist.md 'release-artifacts/supply-chain' 'supply-chain artifact output path'
require_text docs/release-candidate-checklist.md 'metadata/reporting check' 'supply-chain metadata/reporting boundary'
require_text docs/release-candidate-checklist.md 'Secret scanning runs through `.github/workflows/secret-scanning\.yml`' 'secret scanning release-candidate workflow'
require_text docs/release-candidate-checklist.md 'ghcr\.io/gitleaks/gitleaks:v8\.30\.0' 'pinned Gitleaks release-candidate workflow'
require_text docs/release-candidate-checklist.md 'GITLEAKS_LICENSE' 'Gitleaks Action org-license release-candidate boundary'
require_text docs/release-candidate-checklist.md 'protection, and alert triage policy' 'secret scanning governance release-candidate boundary'
require_text docs/release-candidate-checklist.md 'arm64' 'arm64 release-candidate architecture'
require_text docs/at-rest-data-strategy.md 'app-private local storage' 'v1 app-private storage decision'
require_text docs/at-rest-data-strategy.md 'encryption-at-rest is not implemented yet' 'v1 encryption-at-rest non-implementation'
require_text docs/at-rest-data-strategy.md 'SQLite database' 'SQLite at-rest data scope'
require_text docs/at-rest-data-strategy.md 'private meeting audio artifacts' 'private audio artifact at-rest data scope'
require_text docs/at-rest-data-strategy.md 'provider API keys, OAuth tokens, calendar' 'not-stored secret classes'
require_text docs/at-rest-data-strategy.md 'keychain or equivalent secure storage' 'future OS keychain boundary'
require_text docs/at-rest-data-strategy.md 'must not be stored in SQLite, app settings, plain JSON files' 'future secrets must not use plain storage'
require_text docs/at-rest-data-strategy.md 'migration, recovery, delete, backup/restore' 'future encryption migration and recovery boundary'

require_text Cargo.toml '^license = "Apache-2\.0"$' 'workspace Apache-2.0 license metadata'

for manifest in crates/*/Cargo.toml; do
  require_text "$manifest" '^license\.workspace = true$' 'workspace-inherited license metadata'
done

require_text apps/desktop/src-tauri/Cargo.toml '^license = "Apache-2\.0"$' 'desktop backend Apache-2.0 license metadata'
require_text .github/workflows/ci.yml 'cargo fmt --check' 'Rust formatting CI gate'
require_text .github/workflows/ci.yml 'cargo install cargo-audit --locked' 'cargo-audit CI installation'
require_text .github/workflows/ci.yml 'cargo install cargo-llvm-cov --locked' 'cargo-llvm-cov CI installation'
if ! node <<'NODE'
const fs = require("fs");

const file = ".github/workflows/ci.yml";
const text = fs.readFileSync(file, "utf8");
const steps = [];
let current = null;
let inJobs = false;
let currentJob = null;

for (const line of text.split(/\r?\n/)) {
  if (/^jobs:\s*$/.test(line)) {
    inJobs = true;
    continue;
  }
  if (inJobs) {
    const jobMatch = line.match(/^ {2}([A-Za-z0-9_-]+):\s*$/);
    if (jobMatch) {
      if (current) {
        steps.push(current);
        current = null;
      }
      currentJob = jobMatch[1];
    }
  }
  const match = line.match(/^ {6}- name:\s*(.+?)\s*$/);
  if (match) {
    if (current) {
      steps.push(current);
    }
    current = { name: match[1], job: currentJob, index: steps.length, lines: [line] };
  } else if (current) {
    current.lines.push(line);
  }
}

if (current) {
  steps.push(current);
}

let ok = true;

function fail(message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function requireStep(name) {
  const step = steps.find((candidate) => candidate.name === name);
  if (!step) {
    fail(`Missing CI step: ${name}`);
  }
  return step;
}

function hasLine(step, pattern) {
  return step?.lines.some((line) => pattern.test(line)) ?? false;
}

const install = requireStep("Install cargo-audit");
if (!hasLine(install, /^\s*run:\s*cargo install cargo-audit --locked\s*$/)) {
  fail("Install cargo-audit step must run cargo install cargo-audit --locked");
}

const rootAudit = requireStep("Audit Rust workspace dependencies");
if (!hasLine(rootAudit, /^\s*run:\s*cargo audit\s*$/)) {
  fail("Root Rust advisory audit step must run cargo audit");
}
if (hasLine(rootAudit, /^\s*working-directory:/)) {
  fail("Root Rust advisory audit step must not set working-directory");
}

const desktopAudit = requireStep("Audit desktop Rust backend dependencies");
if (!hasLine(desktopAudit, /^\s*working-directory:\s*apps\/desktop\/src-tauri\s*$/)) {
  fail("Desktop Rust advisory audit step must run from apps/desktop/src-tauri");
}
if (!hasLine(desktopAudit, /^\s*run:\s*cargo audit\s*$/)) {
  fail("Desktop Rust advisory audit step must run cargo audit");
}

if (install?.job !== rootAudit?.job || install?.job !== desktopAudit?.job) {
  fail("cargo-audit install, root audit, and desktop audit steps must be in the same CI job");
}
if (
  install &&
  rootAudit &&
  desktopAudit &&
  (install.index > rootAudit.index || install.index > desktopAudit.index)
) {
  fail("Install cargo-audit step must run before root and desktop Rust advisory audit steps");
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi
require_text .github/workflows/ci.yml 'cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check' 'desktop Rust formatting CI gate'
require_text .github/workflows/ci.yml 'cargo test --workspace' 'Rust test CI gate'
require_text .github/workflows/ci.yml 'cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml' 'desktop Rust test CI gate'
require_text .github/workflows/ci.yml 'cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --no-default-features' 'no-Whisper desktop Rust test CI gate'
require_text .github/workflows/ci.yml 'cargo llvm-cov --workspace --lcov --output-path release-artifacts/coverage/rust/workspace\.lcov' 'Rust workspace coverage artifact CI gate'
require_text .github/workflows/ci.yml 'cargo llvm-cov --manifest-path apps/desktop/src-tauri/Cargo.toml --lcov --output-path release-artifacts/coverage/rust/desktop-tauri\.lcov' 'desktop Tauri coverage artifact CI gate'
require_text .github/workflows/ci.yml 'runs-on: macos-26' 'macOS runner for ScreenCaptureKit system-audio compile gate'
require_text .github/workflows/ci.yml 'cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml --no-default-features --features system-audio-screencapturekit' 'ScreenCaptureKit system-audio desktop compile CI gate'
require_text .github/workflows/ci.yml 'cargo clippy --workspace --all-targets -- -D warnings' 'Rust clippy CI gate'
require_text .github/workflows/ci.yml 'cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings' 'desktop Rust clippy CI gate'
require_text .github/workflows/ci.yml 'cargo run -p curiosity-audio --bin audio-smoke' 'audio smoke fail-loud CI gate'
require_text .github/workflows/ci.yml 'cargo run -p curiosity-transcription --bin whisper-smoke' 'Whisper smoke fail-loud CI gate'
require_text .github/workflows/ci.yml 'apt-get update' 'Linux system dependency update before Tauri backend checks'
require_text .github/workflows/ci.yml 'libwebkit2gtk-4\.1-dev' 'Tauri Linux WebKitGTK dependency'
require_text .github/workflows/ci.yml 'libgtk-3-dev' 'Tauri Linux GTK dependency'
require_text .github/workflows/ci.yml 'pkg-config' 'Tauri Linux pkg-config dependency'
require_text .github/workflows/ci.yml 'libayatana-appindicator3-dev' 'Tauri Linux app indicator dependency'
require_text .github/workflows/ci.yml 'librsvg2-dev' 'Tauri Linux SVG dependency'
require_text .github/workflows/ci.yml 'npm run test' 'desktop test CI gate'
require_text .github/workflows/ci.yml 'npm run test:coverage' 'desktop frontend coverage artifact CI gate'
require_text .github/workflows/ci.yml 'node scripts/check-coverage-artifacts\.js' 'coverage artifact source-path checker CI gate'
require_text .github/workflows/ci.yml 'path: release-artifacts/coverage' 'coverage artifact upload path'
require_text .github/workflows/ci.yml 'npm run build' 'desktop build CI gate'
require_text .github/workflows/ci.yml 'npm audit --audit-level=high' 'desktop npm audit CI gate'
require_text apps/desktop/package.json '"test:coverage": "vitest run --coverage"' 'desktop Vitest coverage npm script'
require_text apps/desktop/package.json '"@vitest/coverage-v8"' 'desktop Vitest V8 coverage dependency'
require_text apps/desktop/vite.config.ts 'reportsDirectory: "../../release-artifacts/coverage/frontend"' 'frontend coverage output directory'
require_text apps/desktop/vite.config.ts 'reporter: \["lcovonly"\]' 'frontend LCOV-only coverage reporter'
require_text scripts/check-coverage-artifacts.js 'apps/desktop/src/App\.tsx' 'coverage checker frontend App path'
require_text scripts/check-coverage-artifacts.js 'apps/desktop/src/commandAdapter\.ts' 'coverage checker frontend command adapter path'
require_text scripts/check-coverage-artifacts.js 'crates/store/src/lib\.rs' 'coverage checker Rust store path'
require_text scripts/check-coverage-artifacts.js 'apps/desktop/src-tauri/src/main\.rs' 'coverage checker desktop Tauri main path'
if ! node --check scripts/generate-supply-chain-artifacts.js >/dev/null; then
  failures=1
fi
if ! node --check scripts/check-coverage-artifacts.js >/dev/null; then
  failures=1
fi
if ! node <<'NODE'
const fs = require("fs");

const file = ".github/workflows/ci.yml";
const text = fs.readFileSync(file, "utf8");
const steps = [];
let current = null;
let inJobs = false;
let currentJob = null;

for (const line of text.split(/\r?\n/)) {
  if (/^jobs:\s*$/.test(line)) {
    inJobs = true;
    continue;
  }
  if (inJobs) {
    const jobMatch = line.match(/^ {2}([A-Za-z0-9_-]+):\s*$/);
    if (jobMatch) {
      if (current) {
        steps.push(current);
        current = null;
      }
      currentJob = jobMatch[1];
    }
  }
  const match = line.match(/^ {6}- name:\s*(.+?)\s*$/);
  if (match) {
    if (current) {
      steps.push(current);
    }
    current = { name: match[1], job: currentJob, index: steps.length, lines: [line] };
  } else if (current) {
    current.lines.push(line);
  }
}

if (current) {
  steps.push(current);
}

let ok = true;

function fail(message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function requireStep(name) {
  const step = steps.find((candidate) => candidate.name === name);
  if (!step) {
    fail(`Missing CI step: ${name}`);
  }
  return step;
}

function hasLine(step, pattern) {
  return step?.lines.some((line) => pattern.test(line)) ?? false;
}

const installCargoLlvmCov = requireStep("Install cargo-llvm-cov");
const installTauriLinuxDeps = requireStep("Install Tauri Linux system dependencies");
const generateRustCoverage = requireStep("Generate Rust coverage artifacts");
const installDesktop = requireStep("Install desktop dependencies");
const generateFrontendCoverage = requireStep("Generate desktop frontend coverage");
const checkCoverage = requireStep("Check coverage artifacts");
const uploadCoverage = requireStep("Upload coverage artifacts");

if (!hasLine(installCargoLlvmCov, /^\s*run:\s*cargo install cargo-llvm-cov --locked\s*$/)) {
  fail("Install cargo-llvm-cov step must run cargo install cargo-llvm-cov --locked");
}
if (!hasLine(generateRustCoverage, /^\s*cargo llvm-cov --workspace --lcov --output-path release-artifacts\/coverage\/rust\/workspace\.lcov\s*$/)) {
  fail("Generate Rust coverage artifacts step must create the workspace LCOV report");
}
if (!hasLine(generateRustCoverage, /^\s*cargo llvm-cov --manifest-path apps\/desktop\/src-tauri\/Cargo\.toml --lcov --output-path release-artifacts\/coverage\/rust\/desktop-tauri\.lcov\s*$/)) {
  fail("Generate Rust coverage artifacts step must create the desktop Tauri LCOV report");
}
if (!hasLine(generateFrontendCoverage, /^\s*working-directory:\s*apps\/desktop\s*$/)) {
  fail("Generate desktop frontend coverage step must run from apps/desktop");
}
if (!hasLine(generateFrontendCoverage, /^\s*run:\s*npm run test:coverage\s*$/)) {
  fail("Generate desktop frontend coverage step must run npm run test:coverage");
}
if (!hasLine(checkCoverage, /^\s*run:\s*node scripts\/check-coverage-artifacts\.js\s*$/)) {
  fail("Check coverage artifacts step must run node scripts/check-coverage-artifacts.js");
}
if (!hasLine(uploadCoverage, /^\s*uses:\s*actions\/upload-artifact@v4\s*$/)) {
  fail("Upload coverage artifacts step must use actions/upload-artifact@v4");
}
if (!hasLine(uploadCoverage, /^\s*name:\s*coverage-artifacts\s*$/)) {
  fail("Upload coverage artifacts step must name the artifact coverage-artifacts");
}
if (!hasLine(uploadCoverage, /^\s*path:\s*release-artifacts\/coverage\s*$/)) {
  fail("Upload coverage artifacts step must upload release-artifacts/coverage");
}
if (!hasLine(uploadCoverage, /^\s*if-no-files-found:\s*error\s*$/)) {
  fail("Upload coverage artifacts step must fail when coverage artifacts are missing");
}

const coverageSteps = [
  installCargoLlvmCov,
  installTauriLinuxDeps,
  generateRustCoverage,
  installDesktop,
  generateFrontendCoverage,
  checkCoverage,
  uploadCoverage,
];
if (!coverageSteps.every((step) => step?.job === "checks")) {
  fail("Coverage install, generation, check, and upload steps must run in the checks CI job");
}
if (
  installCargoLlvmCov &&
  installTauriLinuxDeps &&
  generateRustCoverage &&
  installDesktop &&
  generateFrontendCoverage &&
  checkCoverage &&
  uploadCoverage &&
  (
    installCargoLlvmCov.index > generateRustCoverage.index ||
    installTauriLinuxDeps.index > generateRustCoverage.index ||
    installDesktop.index > generateFrontendCoverage.index ||
    generateRustCoverage.index > checkCoverage.index ||
    generateFrontendCoverage.index > checkCoverage.index ||
    checkCoverage.index > uploadCoverage.index
  )
) {
  fail("Coverage artifacts must be generated after tool/dependency setup, checked after Rust and frontend coverage, and uploaded after the checker");
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi
if ! node <<'NODE'
const fs = require("fs");

const file = ".github/workflows/ci.yml";
const text = fs.readFileSync(file, "utf8");
const steps = [];
let current = null;
let inJobs = false;
let currentJob = null;

for (const line of text.split(/\r?\n/)) {
  if (/^jobs:\s*$/.test(line)) {
    inJobs = true;
    continue;
  }
  if (inJobs) {
    const jobMatch = line.match(/^ {2}([A-Za-z0-9_-]+):\s*$/);
    if (jobMatch) {
      if (current) {
        steps.push(current);
        current = null;
      }
      currentJob = jobMatch[1];
    }
  }
  const match = line.match(/^ {6}- name:\s*(.+?)\s*$/);
  if (match) {
    if (current) {
      steps.push(current);
    }
    current = { name: match[1], job: currentJob, index: steps.length, lines: [line] };
  } else if (current) {
    current.lines.push(line);
  }
}

if (current) {
  steps.push(current);
}

let ok = true;

function fail(message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function requireStep(name) {
  const step = steps.find((candidate) => candidate.name === name);
  if (!step) {
    fail(`Missing CI step: ${name}`);
  }
  return step;
}

function hasLine(step, pattern) {
  return step?.lines.some((line) => pattern.test(line)) ?? false;
}

const installDesktop = requireStep("Install desktop dependencies");
const generate = requireStep("Generate supply-chain artifacts");
const upload = requireStep("Upload supply-chain artifacts");

if (!hasLine(generate, /^\s*run:\s*node scripts\/generate-supply-chain-artifacts\.js\s*$/)) {
  fail("Generate supply-chain artifacts step must run node scripts/generate-supply-chain-artifacts.js");
}
if (!hasLine(upload, /^\s*uses:\s*actions\/upload-artifact@v4\s*$/)) {
  fail("Upload supply-chain artifacts step must use actions/upload-artifact@v4");
}
if (!hasLine(upload, /^\s*name:\s*supply-chain-artifacts\s*$/)) {
  fail("Upload supply-chain artifacts step must name the artifact supply-chain-artifacts");
}
if (!hasLine(upload, /^\s*path:\s*release-artifacts\/supply-chain\s*$/)) {
  fail("Upload supply-chain artifacts step must upload release-artifacts/supply-chain");
}
if (!hasLine(upload, /^\s*if-no-files-found:\s*error\s*$/)) {
  fail("Upload supply-chain artifacts step must fail when artifacts are missing");
}
if (installDesktop?.job !== generate?.job || installDesktop?.job !== upload?.job) {
  fail("desktop npm install, supply-chain generation, and supply-chain upload steps must be in the same CI job");
}
if (
  installDesktop &&
  generate &&
  upload &&
  (installDesktop.index > generate.index || generate.index > upload.index)
) {
  fail("Supply-chain artifact generation must run after desktop npm install and before upload");
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi
require_text .github/workflows/ci.yml 'check-publication-readiness\.sh' 'publication readiness CI gate'
require_text .github/workflows/ci.yml 'check-pages-site\.js' 'Pages site validation CI gate'
require_text .github/workflows/ci.yml 'check-pages-workflow\.js' 'Pages workflow validation CI gate'
require_text .github/workflows/ci.yml 'check-release-workflow\.js' 'GitHub Release workflow validation CI gate'
if ! node <<'NODE'
const fs = require("fs");

const file = ".github/workflows/codeql.yml";
const text = fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n");
const expected = `name: CodeQL

on:
  push:
  pull_request:
  schedule:
    - cron: "21 4 * * 2"

permissions:
  contents: read
  security-events: write

jobs:
  analyze:
    name: Analyze (\${{ matrix.language }})
    runs-on: ubuntu-latest
    timeout-minutes: 30
    strategy:
      fail-fast: false
      matrix:
        include:
          - language: rust
            build-mode: none
          - language: javascript-typescript
            build-mode: none
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4

      - name: Initialize CodeQL
        uses: github/codeql-action/init@v4
        with:
          languages: \${{ matrix.language }}
          build-mode: \${{ matrix.build-mode }}

      - name: Perform CodeQL analysis
        uses: github/codeql-action/analyze@v4
        with:
          category: "/language:\${{ matrix.language }}"
`;
let ok = true;

function fail(message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

if (text !== expected) {
  fail(
    "CodeQL workflow must match the approved Rust and JavaScript/TypeScript advanced setup; update this readiness guard with any intentional workflow change",
  );
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi
if ! node <<'NODE'
const fs = require("fs");

const file = ".github/workflows/secret-scanning.yml";
const text = fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n");
const expected = `name: Secret Scanning

on:
  push:
  pull_request:
  workflow_dispatch:
  schedule:
    - cron: "37 4 * * 3"

permissions:
  contents: read

jobs:
  gitleaks:
    runs-on: ubuntu-latest

    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Run Gitleaks secret scan
        run: |
          docker run --rm -v "$PWD:/repo" ghcr.io/gitleaks/gitleaks:v8.30.0 detect --source=/repo --redact --verbose --no-banner
`;
let ok = true;

function fail(message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

if (text !== expected) {
  fail(
    "Secret scanning workflow must use the approved pinned Gitleaks CLI container scan with full history, redaction, and default fail-on-detection behavior; update this readiness guard with any intentional workflow change",
  );
}

if (/--exit-code\s+0/.test(text) || /continue-on-error:\s*true/.test(text)) {
  fail("Secret scanning must fail on detected leaks");
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi
require_text .github/dependabot.yml 'package-ecosystem: "npm"' 'Dependabot npm update automation'
require_text .github/dependabot.yml 'directory: "/apps/desktop"' 'Dependabot desktop npm directory'
require_text .github/dependabot.yml 'package-ecosystem: "cargo"' 'Dependabot cargo update automation'
require_text .github/dependabot.yml 'directory: "/"' 'Dependabot root cargo directory'
require_text .github/dependabot.yml 'directory: "/apps/desktop/src-tauri"' 'Dependabot desktop Tauri cargo directory'
require_text .github/dependabot.yml 'package-ecosystem: "github-actions"' 'Dependabot GitHub Actions update automation'
require_text scripts/check-publication-readiness.sh 'if ! node scripts/check-tauri-security\.js; then' 'Tauri renderer CSP publication readiness gate'
require_text scripts/check-publication-readiness.sh 'if ! node scripts/check-tauri-command-surface\.js; then' 'Tauri command surface publication readiness gate'
require_text scripts/check-publication-readiness.sh 'if ! node scripts/check-desktop-command-view-contract\.js; then' 'desktop command/view contract shape publication readiness gate'
require_text scripts/check-desktop-command-view-contract.js 'This is not generated DTO ownership' 'desktop command/view schema boundary'
require_text scripts/generate-supply-chain-artifacts.js 'npm", \["sbom", "--sbom-format", "cyclonedx", "--sbom-type", "application"\]' 'npm CycloneDX SBOM generation command'
require_text scripts/generate-supply-chain-artifacts.js 'cargo metadata --locked --format-version 1' 'root Cargo locked metadata command'
require_text scripts/generate-supply-chain-artifacts.js 'aarch64-apple-darwin' 'arm64 macOS Cargo metadata target filter'
require_text scripts/generate-supply-chain-artifacts.js 'filter-platform' 'Cargo metadata platform filter'
require_text scripts/generate-supply-chain-artifacts.js 'delete sbom\.serialNumber' 'npm SBOM serial number normalization'
require_text scripts/generate-supply-chain-artifacts.js 'delete sbom\.metadata\.timestamp' 'npm SBOM timestamp normalization'
require_text scripts/generate-supply-chain-artifacts.js 'apps/desktop/src-tauri/Cargo\.toml' 'desktop Tauri Cargo metadata command'
require_text scripts/generate-supply-chain-artifacts.js 'license_file' 'Cargo license_file fallback'
require_text scripts/generate-supply-chain-artifacts.js 'root-cargo-\$\{releaseRustTarget\}-license-metadata\.json' 'root Cargo license metadata artifact'
require_text scripts/generate-supply-chain-artifacts.js 'desktop-tauri-cargo-\$\{releaseRustTarget\}-license-metadata\.json' 'desktop Tauri Cargo license metadata artifact'
require_text scripts/generate-supply-chain-artifacts.js 'release-artifacts", "supply-chain' 'supply-chain artifact output directory'
require_text .github/workflows/pages.yml 'macos-26' 'macOS 26 runner for ScreenCaptureKit DMG build'
require_text .github/workflows/pages.yml 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable Pages DMG download path'
require_text .github/workflows/release.yml 'gh release upload' 'versioned GitHub Release asset upload'
require_text .github/workflows/release.yml 'runner_arch="\$\(uname -m\)"' 'release runner architecture assertion before aarch64 asset naming'
require_text .github/workflows/release.yml 'if \[ "\$runner_arch" != "arm64" \]; then' 'arm64 runner assertion before aarch64 asset naming'
require_text .github/workflows/release.yml 'Curiosity-Transcripts-\$\{version\}-macos-aarch64\.dmg' 'versioned macOS DMG release asset name'
require_text .github/workflows/release.yml 'hdiutil verify "\$release_asset"' 'release asset hdiutil verification before upload'
require_text .github/workflows/release.yml 'hdiutil attach "\$release_asset" -readonly -nobrowse' 'release asset read-only attach verification before upload'
require_text .github/workflows/release.yml 'Release scope:' 'release notes scope section'
require_text .github/workflows/release.yml 'arm64-only macOS DMG' 'release notes arm64-only scope disclosure'
require_text .github/workflows/release.yml 'SHA-256 checksum asset uploaded beside the DMG' 'release notes checksum asset disclosure'
require_text .github/workflows/release.yml 'Manual smoke status:' 'release notes manual smoke status section'
require_text .github/workflows/release.yml 'Skipped smoke checks are not passes' 'release notes skipped smoke disclosure'
require_text .github/workflows/release.yml 'App-level encryption-at-rest is not implemented in v1' 'release notes encryption-at-rest disclosure'
require_text .github/workflows/release.yml 'user-owned source files and exports can remain outside the app delete boundary' 'release notes source/export boundary disclosure'
require_text site/index.html 'https://curiosityai\.nl' 'CuriosityAI maker link'
require_text site/index.html 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable homepage DMG download link'

if ! node <<'NODE'
const fs = require("fs");

const pkg = JSON.parse(fs.readFileSync("apps/desktop/package.json", "utf8"));
const lock = JSON.parse(fs.readFileSync("apps/desktop/package-lock.json", "utf8"));
const rootLockPackage = lock.packages?.[""];

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

if (pkg.license !== "Apache-2.0") {
  fail("apps/desktop/package.json", "Expected Apache-2.0 license metadata");
}

if (pkg.repository?.url !== "https://github.com/Curiosity-Ai-BV/Curiosity-Transcripts.git") {
  fail("apps/desktop/package.json", "Expected repository metadata");
}

if (rootLockPackage?.license !== "Apache-2.0") {
  fail("apps/desktop/package-lock.json", "Expected root package Apache-2.0 license metadata");
}

process.exit(ok ? 0 : 1);
NODE
then
  failures=1
fi

if ! node scripts/check-pages-site.js; then
  failures=1
fi

if ! node scripts/check-pages-workflow.js; then
  failures=1
fi

if ! node scripts/check-release-workflow.js; then
  failures=1
fi

if ! node scripts/check-tauri-security.js; then
  failures=1
fi

if ! node scripts/check-tauri-command-surface.js; then
  failures=1
fi

if ! node scripts/check-desktop-command-view-contract.js; then
  failures=1
fi

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'Publication readiness metadata is present.\n'
