const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const workflowPath = path.join(root, ".github", "workflows", "release.yml");
const readmePath = path.join(root, "README.md");
const buildScriptPath = path.join(root, "scripts", "build-macos-dmg.sh");
const packageScriptPath = path.join(root, "scripts", "package-macos-dmg.sh");
const dmgDocsPath = path.join(root, "docs", "macos-dmg-release.md");
const workspaceCargoPath = path.join(root, "Cargo.toml");
const packagePath = path.join(root, "apps", "desktop", "package.json");
const lockPath = path.join(root, "apps", "desktop", "package-lock.json");
const tauriCargoPath = path.join(root, "apps", "desktop", "src-tauri", "Cargo.toml");
const tauriConfigPath = path.join(root, "apps", "desktop", "src-tauri", "tauri.conf.json");

const requiredWorkflowText = [
  "macos-26",
  "contents: write",
  "'v*'",
  "node scripts/check-release-workflow.js",
  "./scripts/build-macos-dmg.sh --no-sign",
  'runner_arch="$(uname -m)"',
  'if [ "$runner_arch" != "arm64" ]; then',
  "Curiosity-Transcripts-${version}-macos-aarch64.dmg",
  'hdiutil verify "$release_asset"',
  'hdiutil attach "$release_asset" -readonly -nobrowse',
  '[ ! -d "$mount_dir/Curiosity Transcripts.app" ]',
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
  "Cargo.toml",
  "[workspace.package]",
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/tauri.conf.json",
  "GitHub Release",
  "Curiosity-Transcripts-<version>-macos-aarch64.dmg",
];

const requiredBuildScriptText = [
  "npm ci",
  "npm run test",
  "tauri build --features system-audio-screencapturekit --bundles app --ci",
  "scripts/package-macos-dmg.sh",
];

const requiredPackageScriptText = [
  'codesign --force --deep --sign - "$APP_PATH"',
  'codesign --verify --deep --strict --verbose=2 "$APP_PATH"',
  'codesign --verify --deep --strict --verbose=2 "$VERIFY_MOUNT_DIR/$APP_NAME.app"',
  'hdiutil verify "$DMG_PATH"',
  'hdiutil attach "$DMG_PATH" -readonly -nobrowse',
  '[[ ! -d "$VERIFY_MOUNT_DIR/$APP_NAME.app" ]]',
  'Curiosity Transcripts.app',
];

const requiredDmgDocsText = [
  "hdiutil verify",
  "read-only attach",
  "Curiosity Transcripts.app",
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

function readTomlSectionValue(text, sectionName, key) {
  let inSection = false;

  for (const line of text.split(/\r?\n/)) {
    if (/^\[[^\]]+\]\s*$/.test(line)) {
      inSection = line.trim() === `[${sectionName}]`;
      continue;
    }

    if (inSection) {
      const match = line.match(new RegExp(`^${key} = "([^"]+)"$`));
      if (match) {
        return match[1];
      }
    }
  }

  return undefined;
}

const workflow = readRequired(workflowPath, ".github/workflows/release.yml");
const readme = readRequired(readmePath, "README.md");
const buildScript = readRequired(buildScriptPath, "scripts/build-macos-dmg.sh");
const packageScript = readRequired(packageScriptPath, "scripts/package-macos-dmg.sh");
const dmgDocs = readRequired(dmgDocsPath, "docs/macos-dmg-release.md");

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

for (const text of requiredBuildScriptText) {
  if (!buildScript.includes(text)) {
    fail("scripts/build-macos-dmg.sh", `Missing required release build script content: ${text}`);
  }
}

for (const text of requiredPackageScriptText) {
  if (!packageScript.includes(text)) {
    fail("scripts/package-macos-dmg.sh", `Missing required DMG verification content: ${text}`);
  }
}

for (const text of requiredDmgDocsText) {
  if (!dmgDocs.includes(text)) {
    fail("docs/macos-dmg-release.md", `Missing required DMG release documentation: ${text}`);
  }
}

const pkg = JSON.parse(readRequired(packagePath, "apps/desktop/package.json"));
const lock = JSON.parse(readRequired(lockPath, "apps/desktop/package-lock.json"));
const workspaceCargo = readRequired(workspaceCargoPath, "Cargo.toml");
const tauriCargo = readRequired(tauriCargoPath, "apps/desktop/src-tauri/Cargo.toml");
const tauriConfig = JSON.parse(
  readRequired(tauriConfigPath, "apps/desktop/src-tauri/tauri.conf.json"),
);
const workspaceVersion = readTomlSectionValue(workspaceCargo, "workspace.package", "version");
const tauriVersionMatch = tauriCargo.match(/^version = "([^"]+)"$/m);
const tauriVersion = tauriVersionMatch?.[1];
const lockVersion = lock.packages?.[""]?.version ?? lock.version;

if (!/^\d+\.\d+\.\d+$/.test(pkg.version)) {
  fail("apps/desktop/package.json", "Desktop package version must be SemVer without a leading v");
}

if (process.env.GITHUB_REF_NAME) {
  const expectedTag = `v${pkg.version}`;
  if (process.env.GITHUB_REF_NAME !== expectedTag) {
    fail(
      ".github/workflows/release.yml",
      `Release tag ${process.env.GITHUB_REF_NAME} must match apps/desktop/package.json version ${pkg.version}`,
    );
  }
}

const expectedDesktopScripts = {
  "tauri:build:mac": "../../scripts/build-macos-dmg.sh",
  "tauri:build:mac:unsigned": "../../scripts/build-macos-dmg.sh --no-sign",
  "release:mac:dmg": "../../scripts/build-macos-dmg.sh",
};

for (const [scriptName, expectedCommand] of Object.entries(expectedDesktopScripts)) {
  if (pkg.scripts?.[scriptName] !== expectedCommand) {
    fail(
      "apps/desktop/package.json",
      `${scriptName} must delegate to ${expectedCommand} so it cannot bypass npm ci or tauri --ci`,
    );
  }
}

if (lockVersion !== pkg.version) {
  fail("apps/desktop/package-lock.json", "Root lockfile package version must match apps/desktop/package.json");
}

if (workspaceVersion !== pkg.version) {
  fail("Cargo.toml", "[workspace.package] version must match apps/desktop/package.json");
}

if (tauriVersion !== pkg.version) {
  fail("apps/desktop/src-tauri/Cargo.toml", "Tauri package version must match apps/desktop/package.json");
}

if (tauriConfig.version !== pkg.version) {
  fail("apps/desktop/src-tauri/tauri.conf.json", "Tauri config version must match apps/desktop/package.json");
}

process.exit(ok ? 0 : 1);
