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
check_file ".github/workflows/pages.yml"
check_file ".github/workflows/release.yml"

require_text LICENSE 'Apache License' 'Apache-2.0 license text'
require_text NOTICE 'Curiosity Transcripts' 'Curiosity Transcripts attribution notice'
require_text ATTRIBUTION.md 'Apache-2\.0' 'Apache-2.0 attribution guidance'
require_text CONTRIBUTING.md 'cargo test --workspace' 'deterministic Rust test guidance'
require_text SECURITY.md 'Report' 'private vulnerability reporting guidance'
require_text README.md 'Apache-2\.0' 'license metadata'
require_text README.md 'ATTRIBUTION\.md' 'commercial attribution pointer'
require_text README.md 'SECURITY\.md' 'security policy pointer'
require_text README.md 'GitHub Pages' 'public homepage deployment documentation'
require_text README.md 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable Pages DMG download documentation'
require_text README.md 'Versioning Rules' 'versioning and GitHub Release documentation'

require_text Cargo.toml '^license = "Apache-2\.0"$' 'workspace Apache-2.0 license metadata'

for manifest in crates/*/Cargo.toml; do
  require_text "$manifest" '^license\.workspace = true$' 'workspace-inherited license metadata'
done

require_text apps/desktop/src-tauri/Cargo.toml '^license = "Apache-2\.0"$' 'desktop backend Apache-2.0 license metadata'
require_text .github/workflows/ci.yml 'cargo fmt --check' 'Rust formatting CI gate'
require_text .github/workflows/ci.yml 'cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml --check' 'desktop Rust formatting CI gate'
require_text .github/workflows/ci.yml 'cargo test --workspace' 'Rust test CI gate'
require_text .github/workflows/ci.yml 'cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml' 'desktop Rust test CI gate'
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
require_text .github/workflows/ci.yml 'check-publication-readiness\.sh' 'publication readiness CI gate'
require_text .github/workflows/ci.yml 'check-pages-site\.js' 'Pages site validation CI gate'
require_text .github/workflows/ci.yml 'check-pages-workflow\.js' 'Pages workflow validation CI gate'
require_text .github/workflows/ci.yml 'check-release-workflow\.js' 'GitHub Release workflow validation CI gate'
require_text .github/workflows/pages.yml 'macos-26' 'macOS 26 runner for ScreenCaptureKit DMG build'
require_text .github/workflows/pages.yml 'downloads/Curiosity-Transcripts-latest\.dmg' 'stable Pages DMG download path'
require_text .github/workflows/release.yml 'gh release upload' 'versioned GitHub Release asset upload'
require_text .github/workflows/release.yml 'Curiosity-Transcripts-\$\{version\}-macos-aarch64\.dmg' 'versioned macOS DMG release asset name'
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

if [[ "$failures" -ne 0 ]]; then
  exit 1
fi

printf 'Publication readiness metadata is present.\n'
