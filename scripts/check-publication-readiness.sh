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

for file in LICENSE NOTICE ATTRIBUTION.md CONTRIBUTING.md SECURITY.md README.md; do
  check_file "$file"
done
check_file "site/index.html"
check_file "docs/at-rest-data-strategy.md"
check_file "docs/release-candidate-checklist.md"
check_file "apps/desktop/contracts/desktop-command-view-contract.fixture.json"
check_file ".github/dependabot.yml"
check_file ".github/workflows/codeql.yml"
check_file ".github/workflows/pages.yml"
check_file ".github/workflows/release.yml"

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
require_text docs/production-readiness-roadmap.md 'CI now gates no-Whisper desktop tests and the' 'no-Whisper desktop CI feature-matrix status'
require_text docs/production-readiness-roadmap.md 'ScreenCaptureKit system-audio feature compile path on macOS' 'ScreenCaptureKit feature compile macOS CI status'
require_text docs/production-readiness-roadmap.md 'Real-hardware smoke and release confidence remain manual/later work' 'real hardware release confidence remains later work'
require_text docs/production-readiness-roadmap.md 'processing_jobs.*retry UX' 'durable job retry UX remains later work'
require_text docs/production-readiness-roadmap.md 'Current Phase 5A status' 'Phase 5A command/view contract fixture status'
require_text docs/production-readiness-roadmap.md 'desktop-command-view-contract\.fixture\.json' 'Phase 5A checked-in contract fixture path'
require_text docs/production-readiness-roadmap.md 'Rust tests guard exact equality' 'Phase 5A Rust exact-equality guard'
require_text docs/production-readiness-roadmap.md 'TS command adapter contract tests consume the same fixture' 'Phase 5A TS command adapter contract consumption'
require_text docs/production-readiness-roadmap.md 'Current CodeQL code scanning status' 'current CodeQL code scanning status'
require_text docs/production-readiness-roadmap.md 'SBOM, license output, and secret scanning expectations remain later Phase 2' 'remaining Phase 2 security automation scope'
require_text docs/production-readiness-roadmap.md 'branch-protection or alert triage policy' 'CodeQL policy boundary'
require_text docs/macos-dmg-release.md 'docs/release-candidate-checklist\.md' 'release-candidate checklist link from release docs'
require_text docs/release-candidate-checklist.md 'check-tauri-security\.js' 'Tauri renderer CSP release-candidate gate'
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
require_text docs/release-candidate-checklist.md 'Delete' 'delete release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'startup finalizes pending' 'pending delete restart cleanup release-candidate smoke expectation'
require_text docs/release-candidate-checklist.md 'user-owned exports' 'user-owned export delete boundary release-candidate smoke expectation'
require_text docs/release-candidate-checklist.md 'Uninstall and private-data handling' 'uninstall/private-data release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'JSON, Markdown, and SRT' 'current release-candidate export formats'
require_text docs/release-candidate-checklist.md 'At-rest disclosure' 'at-rest disclosure release-candidate smoke item'
require_text docs/release-candidate-checklist.md 'encryption-at-rest is not implemented in v1' 'release notes at-rest encryption disclosure'
require_text docs/release-candidate-checklist.md 'CodeQL scans Rust and JavaScript/TypeScript' 'CodeQL release-candidate visibility expectation'
require_text docs/release-candidate-checklist.md 'branch-protection or alert triage policy' 'CodeQL policy boundary'
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
require_text .github/workflows/ci.yml 'npm run build' 'desktop build CI gate'
require_text .github/workflows/ci.yml 'npm audit --audit-level=high' 'desktop npm audit CI gate'
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
require_text .github/dependabot.yml 'package-ecosystem: "npm"' 'Dependabot npm update automation'
require_text .github/dependabot.yml 'directory: "/apps/desktop"' 'Dependabot desktop npm directory'
require_text .github/dependabot.yml 'package-ecosystem: "cargo"' 'Dependabot cargo update automation'
require_text .github/dependabot.yml 'directory: "/"' 'Dependabot root cargo directory'
require_text .github/dependabot.yml 'directory: "/apps/desktop/src-tauri"' 'Dependabot desktop Tauri cargo directory'
require_text .github/dependabot.yml 'package-ecosystem: "github-actions"' 'Dependabot GitHub Actions update automation'
require_text scripts/check-publication-readiness.sh 'if ! node scripts/check-tauri-security\.js; then' 'Tauri renderer CSP publication readiness gate'
require_text .github/workflows/pages.yml 'macos-26' 'macOS 26 runner for ScreenCaptureKit DMG build'
require_text .github/workflows/pages.yml 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable Pages DMG download path'
require_text .github/workflows/release.yml 'gh release upload' 'versioned GitHub Release asset upload'
require_text .github/workflows/release.yml 'runner_arch="\$\(uname -m\)"' 'release runner architecture assertion before aarch64 asset naming'
require_text .github/workflows/release.yml 'if \[ "\$runner_arch" != "arm64" \]; then' 'arm64 runner assertion before aarch64 asset naming'
require_text .github/workflows/release.yml 'Curiosity-Transcripts-\$\{version\}-macos-aarch64\.dmg' 'versioned macOS DMG release asset name'
require_text .github/workflows/release.yml 'hdiutil verify "\$release_asset"' 'release asset hdiutil verification before upload'
require_text .github/workflows/release.yml 'hdiutil attach "\$release_asset" -readonly -nobrowse' 'release asset read-only attach verification before upload'
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

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'Publication readiness metadata is present.\n'
