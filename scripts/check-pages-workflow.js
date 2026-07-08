const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const workflowPath = path.join(root, ".github", "workflows", "pages.yml");

const requiredText = [
  "macos-26",
  "bash scripts/check-publication-readiness.sh",
  "APPLE_CERTIFICATE_P12_BASE64",
  "APPLE_API_KEY_ID",
  "./scripts/configure-apple-signing-ci.sh",
  "./scripts/build-macos-dmg.sh",
  'version="$(node -p "require(\'./apps/desktop/package.json\').version")"',
  'runner_arch="$(uname -m)"',
  'if [ "$runner_arch" != "arm64" ]; then',
  "macos-aarch64",
  'dmg_path="apps/desktop/src-tauri/target/release/bundle/dmg/Curiosity Transcripts_${version}_aarch64.dmg"',
  '[ ! -f "$dmg_path" ]',
  'cp "$dmg_path" pages-download/Curiosity-Transcripts-latest.dmg',
  "hdiutil verify pages-download/Curiosity-Transcripts-latest.dmg",
  "xcrun stapler validate pages-download/Curiosity-Transcripts-latest.dmg",
  "spctl -a -vvv -t open --context context:primary-signature pages-download/Curiosity-Transcripts-latest.dmg",
  'hdiutil attach pages-download/Curiosity-Transcripts-latest.dmg -readonly -nobrowse -mountpoint "$mount_dir"',
  '[ ! -d "$mount_dir/Curiosity Transcripts.app" ]',
  'codesign --verify --deep --strict --verbose=2 "$mount_dir/Curiosity Transcripts.app"',
  'spctl -a -vvv -t exec "$mount_dir/Curiosity Transcripts.app"',
  'hdiutil detach "$mount_dir"',
  'rm -rf "$mount_dir"',
  "actions/upload-artifact@v4",
  "actions/download-artifact@v4",
  "downloads/Curiosity-Transcripts-latest.dmg",
  "actions/configure-pages@v5",
  "actions/upload-pages-artifact@v3",
  "actions/deploy-pages@v4",
  "pages: write",
  "id-token: write",
  "github-pages",
];

let ok = true;

function exactRunStepLine(text, command) {
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trim() === `run: ${command}`) {
      return index;
    }
  }
  return -1;
}

if (!fs.existsSync(workflowPath)) {
  console.error("::error file=.github/workflows/pages.yml::Missing GitHub Pages deployment workflow");
  process.exit(1);
}

const yaml = fs.readFileSync(workflowPath, "utf8");

for (const text of requiredText) {
  if (!yaml.includes(text)) {
    console.error(`::error file=.github/workflows/pages.yml::Missing required Pages workflow content: ${text}`);
    ok = false;
  }
}

const publicationReadinessStepLine = exactRunStepLine(yaml, "bash scripts/check-publication-readiness.sh");
const buildDmgStepLine = exactRunStepLine(yaml, "./scripts/build-macos-dmg.sh");
if (publicationReadinessStepLine === -1 || buildDmgStepLine === -1 || publicationReadinessStepLine > buildDmgStepLine) {
  console.error(
    "::error file=.github/workflows/pages.yml::Publication readiness must run before building the public Pages DMG",
  );
  ok = false;
}

if (/ubuntu-latest[\s\S]*build-macos-dmg\.sh/.test(yaml)) {
  console.error("::error file=.github/workflows/pages.yml::macOS DMG build must not run on ubuntu-latest");
  ok = false;
}

if (/runs-on:\s*macos-latest/.test(yaml)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages DMG build must pin macos-26 for the ScreenCaptureKit/apple-metal SDK requirement",
  );
  ok = false;
}

if (/find\s+apps\/desktop\/src-tauri\/target\/release\/bundle\/dmg\s+-name\s+['"]?\*\.dmg['"]?\s+-type\s+f\s+-print\s+-quit/.test(yaml)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages latest DMG staging must use the deterministic versioned aarch64 path instead of first-match find",
  );
  ok = false;
}

process.exit(ok ? 0 : 1);
