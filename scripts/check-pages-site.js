const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const sitePath = path.join(root, "site", "index.html");

const requiredText = [
  "Curiosity Transcripts",
  "Local-first meeting transcripts",
  "microphone and system audio",
  "local Whisper",
  "Ollama",
  "exports JSON, Markdown, and SRT from the",
  "desktop app. JSON remains the deterministic integration format",
  "Search and transcript export",
  "Developer ID signed and notarized arm64 macOS DMG",
  "latest public DMG is manually published after the signed/notarized workflow",
  "filled smoke evidence validation",
  "https://curiosityai.nl",
  "downloads/Curiosity-Transcripts-latest.dmg",
  "downloads/Curiosity-Transcripts-latest.dmg.sha256",
  "downloads/Curiosity-Transcripts-latest.provenance.json",
  "Curiosity-Transcripts-latest.dmg",
  "Curiosity-Transcripts-latest.dmg.sha256",
  "Curiosity-Transcripts-latest.provenance.json",
];

const forbiddenText = [
  "Markdown and SRT are lower-level helpers",
  "not desktop export",
  "not desktop export buttons",
  "until productized",
  "provides JSON export from the desktop app today",
  "current unsigned macOS",
  "Signed and notarized distribution remains a separate release step",
  "Verify unsigned DMG before publishing",
  "The macOS DMG should be attached to each deployment",
  "Automated deployment artifact",
  "Each GitHub Pages deployment publishes",
];

let ok = true;

if (!fs.existsSync(sitePath)) {
  console.error("::error file=site/index.html::Missing GitHub Pages homepage");
  process.exit(1);
}

const html = fs.readFileSync(sitePath, "utf8");
const visibleHtml = html.replace(/<!--[\s\S]*?-->/g, "");
const downloadSectionMatch = visibleHtml.match(/<section\b[^>]*\bid="download"[^>]*>[\s\S]*?<\/section>/);
const downloadSection = downloadSectionMatch?.[0] ?? "";

for (const text of requiredText) {
  if (!visibleHtml.includes(text)) {
    console.error(`::error file=site/index.html::Missing required homepage content: ${text}`);
    ok = false;
  }
}

for (const text of forbiddenText) {
  if (visibleHtml.includes(text)) {
    console.error(`::error file=site/index.html::Stale homepage content should be removed: ${text}`);
    ok = false;
  }
}

for (const text of [
  "Manual release artifact",
  "latest public DMG is manually published after the signed/notarized workflow",
  "filled smoke evidence validation",
  'href="downloads/Curiosity-Transcripts-latest.dmg"',
  'href="downloads/Curiosity-Transcripts-latest.dmg.sha256"',
  'href="downloads/Curiosity-Transcripts-latest.provenance.json"',
]) {
  if (!downloadSection.includes(text)) {
    console.error(`::error file=site/index.html::Download section is missing required content: ${text}`);
    ok = false;
  }
}

if (!/<meta\s+name="viewport"\s+content="width=device-width,\s*initial-scale=1"/.test(visibleHtml)) {
  console.error("::error file=site/index.html::Missing responsive viewport meta tag");
  ok = false;
}

if (!/<main[\s>]/.test(visibleHtml) || !/<section[\s>]/.test(visibleHtml)) {
  console.error("::error file=site/index.html::Homepage should expose semantic main and section landmarks");
  ok = false;
}

process.exit(ok ? 0 : 1);
