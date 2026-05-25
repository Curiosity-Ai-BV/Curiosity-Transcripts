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
  "Markdown, JSON, and SRT",
  "https://curiosityai.nl",
  "downloads/Curiosity-Transcripts-latest.dmg",
  "Curiosity-Transcripts-latest.dmg",
];

let ok = true;

if (!fs.existsSync(sitePath)) {
  console.error("::error file=site/index.html::Missing GitHub Pages homepage");
  process.exit(1);
}

const html = fs.readFileSync(sitePath, "utf8");

for (const text of requiredText) {
  if (!html.includes(text)) {
    console.error(`::error file=site/index.html::Missing required homepage content: ${text}`);
    ok = false;
  }
}

if (!/<meta\s+name="viewport"\s+content="width=device-width,\s*initial-scale=1"/.test(html)) {
  console.error("::error file=site/index.html::Missing responsive viewport meta tag");
  ok = false;
}

if (!/<main[\s>]/.test(html) || !/<section[\s>]/.test(html)) {
  console.error("::error file=site/index.html::Homepage should expose semantic main and section landmarks");
  ok = false;
}

process.exit(ok ? 0 : 1);
