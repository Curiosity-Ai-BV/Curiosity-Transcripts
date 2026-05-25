const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const workflowPath = path.join(root, ".github", "workflows", "pages.yml");

const requiredText = [
  "macos-26",
  "./scripts/build-macos-dmg.sh --no-sign",
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

process.exit(ok ? 0 : 1);
