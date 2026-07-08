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
const releaseWorkflowLabel = ".github/workflows/release.yml";

const criticalReleaseSteps = [
  "Check release version sources",
  "Check publication readiness",
  "Configure Apple signing credentials",
  "Build signed and notarized macOS DMG",
  "Stage versioned release assets",
  "Create or update GitHub Release",
];

const requiredWorkflowText = [
  "macos-26",
  "contents: write",
  "'v*'",
  "node scripts/check-release-workflow.js",
  "bash scripts/check-publication-readiness.sh",
  "APPLE_CERTIFICATE_P12_BASE64",
  "APPLE_API_KEY_ID",
  "./scripts/configure-apple-signing-ci.sh",
  "./scripts/build-macos-dmg.sh",
  'runner_arch="$(uname -m)"',
  'if [ "$runner_arch" != "arm64" ]; then',
  "Curiosity-Transcripts-${version}-macos-aarch64.dmg",
  'hdiutil verify "$release_asset"',
  'xcrun stapler validate "$release_asset"',
  'spctl -a -vvv -t open --context context:primary-signature "$release_asset"',
  'hdiutil attach "$release_asset" -readonly -nobrowse',
  '[ ! -d "$mount_dir/Curiosity Transcripts.app" ]',
  'codesign --verify --deep --strict --verbose=2 "$mount_dir/Curiosity Transcripts.app"',
  'spctl -a -vvv -t exec "$mount_dir/Curiosity Transcripts.app"',
  "shasum -a 256",
  "Release scope:",
  "arm64-only macOS DMG",
  '$(basename "$CHECKSUM_PATH")',
  "Manual smoke status:",
  "Skipped smoke checks are not passes",
  "docs/release-candidate-smoke-evidence.template.json",
  "node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json",
  "This workflow does not validate filled manual evidence automatically",
  "leaves this GitHub Release as a draft until filled manual smoke evidence validates",
  "Publishing requires an explicit manual release action",
  'gh release edit "$RELEASE_TAG" --draft=false',
  "Record the build, machine, macOS version, model paths",
  "Privacy and storage disclosure:",
  "App-level encryption-at-rest is not implemented in v1",
  "user-owned source files and exports can remain outside the app delete boundary",
  'release_view_error="$(mktemp)"',
  'release_is_draft="$(gh release view "$RELEASE_TAG" --json isDraft --jq .isDraft 2>"$release_view_error")"',
  'if [ "$release_is_draft" != "true" ]; then',
  "already exists and is published; refusing to edit notes or upload assets",
  'grep -Eiq "not found|HTTP 404" "$release_view_error"',
  "Unable to inspect GitHub Release $RELEASE_TAG draft status; refusing to create, edit, or upload assets",
  'gh release edit "$RELEASE_TAG" --draft \\',
  'gh release create "$RELEASE_TAG" --draft \\',
  "gh release upload",
  "--clobber",
];

const requiredReadmeText = [
  "Versioning Rules",
  "SemVer",
  "vMAJOR.MINOR.PATCH",
  "bash scripts/check-publication-readiness.sh",
  "apps/desktop/package.json",
  "apps/desktop/package-lock.json",
  "Cargo.toml",
  "[workspace.package]",
  "apps/desktop/src-tauri/Cargo.toml",
  "apps/desktop/src-tauri/tauri.conf.json",
  "GitHub Release",
  "Curiosity-Transcripts-<version>-macos-aarch64.dmg",
  "The default command is for signed/notarized release builds",
  "./scripts/build-macos-dmg.sh --no-sign",
];

const requiredBuildScriptText = [
  "bash scripts/check-publication-readiness.sh",
  "npm ci",
  "npm run test",
  "tauri build --features system-audio-screencapturekit --bundles app --ci",
  "env -u CURIOSITY_SKIP_DMG_SIGN",
  "scripts/package-macos-dmg.sh",
];

const requiredPackageScriptText = [
  "APPLE_API_KEY_ID",
  "require_release_signing_credentials",
  "APPLE_SIGNING_IDENTITY is required for signed release DMG builds",
  "Notarization credentials are required for signed release DMG builds",
  "Use ./scripts/build-macos-dmg.sh --no-sign only for local ad-hoc verification",
  '[[ "${CURIOSITY_SKIP_DMG_SIGN:-}" == "1" ]]',
  '[[ "${CURIOSITY_SKIP_DMG_SIGN:-}" != "1" && -n "${APPLE_SIGNING_IDENTITY:-}" ]]',
  'codesign --force --deep --sign - "$APP_PATH"',
  'codesign_args=(--force --deep --options runtime --timestamp --sign "$APPLE_SIGNING_IDENTITY")',
  'codesign --verify --deep --strict --verbose=2 "$APP_PATH"',
  'codesign --verify --deep --strict --verbose=2 "$VERIFY_MOUNT_DIR/$APP_NAME.app"',
  'xcrun stapler validate "$DMG_PATH"',
  'spctl -a -vvv -t open --context context:primary-signature "$DMG_PATH"',
  'spctl -a -vvv -t exec "$VERIFY_MOUNT_DIR/$APP_NAME.app"',
  'hdiutil verify "$DMG_PATH"',
  'hdiutil attach "$DMG_PATH" -readonly -nobrowse',
  '[[ ! -d "$VERIFY_MOUNT_DIR/$APP_NAME.app" ]]',
  'Curiosity Transcripts.app',
];

const requiredDmgDocsText = [
  "APPLE_CERTIFICATE_P12_BASE64",
  "APPLE_API_KEY_ID",
  "bash scripts/check-publication-readiness.sh",
  "Developer ID signed and notarized",
  "The default release build fails when Developer ID signing or notarization",
  "credentials are missing; use `--no-sign` only for local ad-hoc verification",
  "./scripts/build-macos-dmg.sh --no-sign",
  "Release and Pages workflow Apple secrets",
  "Local code cannot enforce GitHub",
  "environment rules; repository settings must allow",
  "Pages `main`",
  "dispatch and release tag path",
  "protected `v*` tags",
  "unintended refs before they can reach the signing path",
  "hdiutil verify",
  "stapler validate",
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

function exactRunStepLine(text, command) {
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].trim() === `run: ${command}`) {
      return index;
    }
  }
  return -1;
}

function hasAdHocDmgBuildRunCommand(text) {
  const lines = text.split(/\r?\n/);
  const isBypassCommand = (line) => {
    const trimmed = line.trim();
    return (
      trimmed !== "" &&
      !trimmed.startsWith("#") &&
      !trimmed.startsWith("echo ") &&
      trimmed.includes("build-macos-dmg.sh") &&
      (trimmed.includes("--no-sign") || trimmed.includes("CURIOSITY_SKIP_DMG_SIGN"))
    );
  };

  for (let index = 0; index < lines.length; index += 1) {
    const runMatch = lines[index].match(/^(\s*)run:\s*(.*)$/);
    if (!runMatch) {
      continue;
    }

    const runIndent = runMatch[1].length;
    const inlineCommand = runMatch[2].trim();
    if (inlineCommand !== "|" && inlineCommand !== ">") {
      if (isBypassCommand(inlineCommand)) {
        return true;
      }
      continue;
    }

    for (let next = index + 1; next < lines.length; next += 1) {
      if (lines[next].trim() !== "" && lines[next].match(/^\s*/)[0].length <= runIndent) {
        break;
      }
      if (isBypassCommand(lines[next])) {
        return true;
      }
    }
  }

  return false;
}

function workflowJobBlock(text, jobName) {
  const lines = text.split(/\r?\n/);
  const start = lines.findIndex((line) => line === `  ${jobName}:`);
  if (start === -1) {
    return [];
  }

  const block = [];
  for (let index = start; index < lines.length; index += 1) {
    if (index !== start && /^  [A-Za-z0-9_-]+:\s*$/.test(lines[index])) {
      break;
    }
    block.push(lines[index]);
  }

  return block;
}

function unquoteWorkflowValue(value) {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith('"') && trimmed.endsWith('"'))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function parseReleaseWorkflowSteps(text) {
  const steps = [];
  let current = null;
  let currentJob = null;
  let inJobs = false;
  let inSteps = false;

  const pushCurrent = () => {
    if (current) {
      steps.push(current);
      current = null;
    }
  };

  for (const line of text.split(/\r?\n/)) {
    if (/^jobs:\s*$/.test(line)) {
      inJobs = true;
      continue;
    }

    if (inJobs) {
      const jobMatch = line.match(/^ {2}([A-Za-z0-9_-]+):\s*$/);
      if (jobMatch) {
        pushCurrent();
        currentJob = jobMatch[1];
        inSteps = false;
        continue;
      }

      if (/^ {4}steps:\s*$/.test(line)) {
        pushCurrent();
        inSteps = true;
        continue;
      }

      if (/^ {4}[A-Za-z0-9_-]+:\s*/.test(line)) {
        pushCurrent();
        inSteps = false;
      }
    }

    if (!inSteps) {
      continue;
    }

    const stepStartMatch = line.match(/^ {6}-\s+(.+?)\s*$/);
    if (stepStartMatch) {
      pushCurrent();
      current = { job: currentJob, name: undefined, lines: [line] };
      const inlineNameMatch = stepStartMatch[1].match(/^name:\s*(.+?)\s*$/);
      if (inlineNameMatch) {
        current.name = unquoteWorkflowValue(inlineNameMatch[1]);
      }
      continue;
    }

    if (current) {
      current.lines.push(line);
      const nameMatch = line.match(/^ {8}name:\s*(.+?)\s*$/);
      if (!current.name && nameMatch) {
        current.name = unquoteWorkflowValue(nameMatch[1]);
      }
    }
  }

  pushCurrent();
  return steps;
}

function hasWorkflowStepKey(step, key) {
  const escapedKey = key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const quotedKeyPattern = `(?:"${escapedKey}"|'${escapedKey}'|${escapedKey})`;
  return step.lines.some((line) => {
    return (
      new RegExp(`^ {6}-\\s+${quotedKeyPattern}\\s*:`).test(line) ||
      new RegExp(`^ {8}${quotedKeyPattern}\\s*:`).test(line)
    );
  });
}

function validateCriticalReleaseStepMetadata(text) {
  const errors = [];
  const steps = parseReleaseWorkflowSteps(text);

  for (const name of criticalReleaseSteps) {
    const matches = steps.filter((step) => step.job === "build-release-dmg" && step.name === name);
    if (matches.length === 0) {
      errors.push(`Missing critical release step: build-release-dmg / ${name}`);
      continue;
    }
    if (matches.length > 1) {
      errors.push(`Critical release step must be unique: build-release-dmg / ${name}`);
    }
    for (const step of matches) {
      if (hasWorkflowStepKey(step, "if")) {
        errors.push(`Critical release step must not be conditionally skipped: build-release-dmg / ${name}`);
      }
      if (hasWorkflowStepKey(step, "continue-on-error")) {
        errors.push(`Critical release step must fail release when its command fails: build-release-dmg / ${name}`);
      }
    }
  }

  return errors;
}

function expectCriticalReleaseStepRejected(name, text, expectedText) {
  const errors = validateCriticalReleaseStepMetadata(text);
  if (errors.length === 0) {
    fail("scripts/check-release-workflow.js", `Guardrail fixture did not reject: ${name}`);
    return;
  }
  if (!errors.some((error) => error.includes(expectedText))) {
    fail(
      "scripts/check-release-workflow.js",
      `Guardrail fixture rejected ${name}, but not for ${expectedText}`,
    );
  }
}

function insertCriticalStepSiblingKey(text, stepName, keyLine) {
  const marker = `      - name: ${stepName}\n`;
  return text.replace(marker, `${marker}${keyLine}\n`);
}

function moveCriticalStepNameAfterInlineKey(text, stepName, keyLine) {
  return text.replace(`      - name: ${stepName}\n`, `      - ${keyLine}\n        name: ${stepName}\n`);
}

function runCriticalReleaseStepSelfGuards(text) {
  const conditionalReadinessStep = insertCriticalStepSiblingKey(
    text,
    "Check publication readiness",
    '        "if": ${{ always() }}',
  );
  if (conditionalReadinessStep === text) {
    fail("scripts/check-release-workflow.js", "Guardrail fixture did not mutate source: conditional critical step");
  } else {
    expectCriticalReleaseStepRejected(
      "conditional critical step",
      conditionalReadinessStep,
      "conditionally skipped",
    );
  }

  const failOpenBuildStep = moveCriticalStepNameAfterInlineKey(
    text,
    "Build signed and notarized macOS DMG",
    "'continue-on-error': true",
  );
  if (failOpenBuildStep === text) {
    fail("scripts/check-release-workflow.js", "Guardrail fixture did not mutate source: fail-open critical step");
  } else {
    expectCriticalReleaseStepRejected(
      "fail-open critical step",
      failOpenBuildStep,
      "must fail release",
    );
  }
}

function firstShellCommandLine(text, matches) {
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (trimmed === "" || trimmed.startsWith("#")) {
      continue;
    }
    if (matches(trimmed)) {
      return index;
    }
  }
  return -1;
}

const workflow = readRequired(workflowPath, ".github/workflows/release.yml");
const readme = readRequired(readmePath, "README.md");
const buildScript = readRequired(buildScriptPath, "scripts/build-macos-dmg.sh");
const packageScript = readRequired(packageScriptPath, "scripts/package-macos-dmg.sh");
const dmgDocs = readRequired(dmgDocsPath, "docs/macos-dmg-release.md");

runCriticalReleaseStepSelfGuards(workflow);
for (const error of validateCriticalReleaseStepMetadata(workflow)) {
  fail(releaseWorkflowLabel, error);
}

for (const text of requiredWorkflowText) {
  if (!workflow.includes(text)) {
    fail(".github/workflows/release.yml", `Missing required release workflow content: ${text}`);
  }
}

const publicationReadinessStepLine = exactRunStepLine(workflow, "bash scripts/check-publication-readiness.sh");
const signingCredentialsStepLine = exactRunStepLine(workflow, "./scripts/configure-apple-signing-ci.sh");
const buildDmgStepLine = exactRunStepLine(workflow, "./scripts/build-macos-dmg.sh");
if (publicationReadinessStepLine === -1 || buildDmgStepLine === -1 || publicationReadinessStepLine > buildDmgStepLine) {
  fail(
    ".github/workflows/release.yml",
    "Publication readiness must run before building the public release DMG",
  );
}
if (signingCredentialsStepLine === -1 || buildDmgStepLine === -1 || signingCredentialsStepLine > buildDmgStepLine) {
  fail(
    ".github/workflows/release.yml",
    "Apple signing credentials must be configured before building the public release DMG",
  );
}
if (hasAdHocDmgBuildRunCommand(workflow)) {
  fail(
    ".github/workflows/release.yml",
    "Public release workflow must not use ad-hoc DMG signing bypasses",
  );
}

const releaseBuildJob = workflowJobBlock(workflow, "build-release-dmg");
if (releaseBuildJob.length === 0) {
  fail(".github/workflows/release.yml", "Missing build-release-dmg job");
} else if (
  !releaseBuildJob.includes("    environment:") ||
  !releaseBuildJob.includes("      name: macos-signing")
) {
  fail(
    ".github/workflows/release.yml",
    "build-release-dmg must use the protected macos-signing environment",
  );
}

const workflowLines = workflow.split(/\r?\n/);
for (let index = 0; index < workflowLines.length; index += 1) {
  const trimmed = workflowLines[index].trim();
  const allowedReleaseNoteLine =
    trimmed === `echo '- Publishing requires an explicit manual release action after evidence passes, for example: gh release edit "$RELEASE_TAG" --draft=false.'`;
  if (!trimmed.includes("gh release") || allowedReleaseNoteLine) {
    continue;
  }

  const commandLines = [trimmed];
  let cursor = index;

  while (commandLines[commandLines.length - 1].endsWith("\\") && cursor + 1 < workflowLines.length) {
    cursor += 1;
    commandLines.push(workflowLines[cursor].trim());
  }

  if (commandLines.join(" ").includes("--draft=false")) {
    fail(
      ".github/workflows/release.yml",
      `Line ${index + 1} must not execute a release publish command; keep --draft=false only in release-note text`,
    );
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

const buildScriptPublicationReadinessLine = firstShellCommandLine(
  buildScript,
  (line) => line === "bash scripts/check-publication-readiness.sh",
);
const buildScriptGuardedCommands = [
  ["npm ci", firstShellCommandLine(buildScript, (line) => line === "npm ci")],
  ["desktop tests", firstShellCommandLine(buildScript, (line) => line === "npm run test")],
  [
    "Tauri build",
    firstShellCommandLine(buildScript, (line) =>
      line.startsWith("npm exec -- tauri build --features system-audio-screencapturekit --bundles app --ci"),
    ),
  ],
  [
    "DMG packaging",
    firstShellCommandLine(buildScript, (line) =>
      /^(CURIOSITY_SKIP_DMG_SIGN=1\s+)?"\$ROOT_DIR\/scripts\/package-macos-dmg\.sh"/.test(line),
    ),
  ],
];

for (const [label, guardedLine] of buildScriptGuardedCommands) {
  if (
    buildScriptPublicationReadinessLine === -1 ||
    guardedLine === -1 ||
    buildScriptPublicationReadinessLine >= guardedLine
  ) {
    fail(
      "scripts/build-macos-dmg.sh",
      `Publication readiness must run before ${label} in direct DMG builds`,
    );
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

if (process.env.GITHUB_REF_TYPE === "tag" || process.env.GITHUB_REF?.startsWith("refs/tags/")) {
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
