const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const workflowPath = path.join(root, ".github", "workflows", "release.yml");
const readmePath = path.join(root, "README.md");
const packagePath = path.join(root, "apps", "desktop", "package.json");
const lockPath = path.join(root, "apps", "desktop", "package-lock.json");
const tauriCargoPath = path.join(root, "apps", "desktop", "src-tauri", "Cargo.toml");

const requiredWorkflowText = [
  "macos-26",
  "contents: write",
  "'v*'",
  "./scripts/build-macos-dmg.sh --no-sign",
  "Curiosity-Transcripts-${version}-macos-aarch64.dmg",
  "shasum -a 256",
  "gh release create",
  "gh release upload",
  "--clobber",
];

const requiredReadmeText = [
  "Versioning Rules",
  "SemVer",
  "vMAJOR.MINOR.PATCH",
  "apps/desktop/package.json",
  "apps/desktop/package-lock.json",
  "apps/desktop/src-tauri/Cargo.toml",
  "GitHub Release",
  "Curiosity-Transcripts-<version>-macos-aarch64.dmg",
];

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function readRequired(filePath, label) {
  if (!fs.existsSync(filePath)) {
    fail(label, "Missing required release file");
    return "";
  }
  return fs.readFileSync(filePath, "utf8");
}

const workflow = readRequired(workflowPath, ".github/workflows/release.yml");
const readme = readRequired(readmePath, "README.md");

for (const text of requiredWorkflowText) {
  if (!workflow.includes(text)) {
    fail(".github/workflows/release.yml", `Missing required release workflow content: ${text}`);
  }
}

for (const text of requiredReadmeText) {
  if (!readme.includes(text)) {
    fail("README.md", `Missing required versioning documentation: ${text}`);
  }
}

const pkg = JSON.parse(readRequired(packagePath, "apps/desktop/package.json"));
const lock = JSON.parse(readRequired(lockPath, "apps/desktop/package-lock.json"));
const tauriCargo = readRequired(tauriCargoPath, "apps/desktop/src-tauri/Cargo.toml");
const tauriVersionMatch = tauriCargo.match(/^version = "([^"]+)"$/m);
const tauriVersion = tauriVersionMatch?.[1];
const lockVersion = lock.packages?.[""]?.version ?? lock.version;

if (!/^\d+\.\d+\.\d+$/.test(pkg.version)) {
  fail("apps/desktop/package.json", "Desktop package version must be SemVer without a leading v");
}

if (lockVersion !== pkg.version) {
  fail("apps/desktop/package-lock.json", "Root lockfile package version must match apps/desktop/package.json");
}

if (tauriVersion !== pkg.version) {
  fail("apps/desktop/src-tauri/Cargo.toml", "Tauri package version must match apps/desktop/package.json");
}

process.exit(ok ? 0 : 1);
