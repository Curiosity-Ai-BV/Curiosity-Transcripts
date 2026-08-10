const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const workflowPath = path.join(root, ".github", "workflows", "pages.yml");

const requiredText = [
  "node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json",
  "macos-signing",
  "github.ref != 'refs/heads/main'",
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
  'latest_asset="pages-download/Curiosity-Transcripts-latest.dmg"',
  'provenance_path="pages-download/Curiosity-Transcripts-latest.provenance.json"',
  'cp "$dmg_path" "$latest_asset"',
  "hdiutil verify pages-download/Curiosity-Transcripts-latest.dmg",
  'hdiutil_verify_status="passed"',
  "xcrun stapler validate pages-download/Curiosity-Transcripts-latest.dmg",
  'stapler_validation_status="passed"',
  "spctl -a -vvv -t open --context context:primary-signature pages-download/Curiosity-Transcripts-latest.dmg",
  'dmg_gatekeeper_assessment_status="passed"',
  'hdiutil attach pages-download/Curiosity-Transcripts-latest.dmg -readonly -nobrowse -mountpoint "$mount_dir"',
  '[ ! -d "$mount_dir/Curiosity Transcripts.app" ]',
  'readonly_attach_app_presence_status="passed"',
  'codesign --verify --deep --strict --verbose=2 "$mount_dir/Curiosity Transcripts.app"',
  'app_codesign_verification_status="passed"',
  'spctl -a -vvv -t exec "$mount_dir/Curiosity Transcripts.app"',
  'app_gatekeeper_assessment_status="passed"',
  'hdiutil detach "$mount_dir"',
  'rm -rf "$mount_dir"',
  'shasum -a 256 "$latest_asset"',
  'printf \'%s  %s\\n\' "$checksum" "$(basename "$latest_asset")" | tee "$latest_asset.sha256"',
  '"kind": "curiosity-transcripts-pages-latest-provenance"',
  '"schema_version": 1',
  '"version": process.env.PAGES_VERSION',
  '"github_ref_name": process.env.GITHUB_REF_NAME_VALUE',
  '"github_sha": process.env.GITHUB_SHA_VALUE',
  '"github_ref": process.env.GITHUB_REF_VALUE',
  "PAGES_REF_NAME:",
  "PAGES_GIT_SHA:",
  "PAGES_GIT_REF:",
  "PAGES_REF_NAME: ${{ github.ref_name }}",
  "PAGES_GIT_SHA: ${{ github.sha }}",
  "PAGES_GIT_REF: ${{ github.ref }}",
  'GITHUB_REF_NAME_VALUE="$PAGES_REF_NAME"',
  'GITHUB_SHA_VALUE="$PAGES_GIT_SHA"',
  'GITHUB_REF_VALUE="$PAGES_GIT_REF"',
  '"runner_architecture": process.env.RUNNER_ARCH',
  '"dmg_asset_name": path.basename(process.env.DMG_ASSET_PATH)',
  '"dmg_asset_path": process.env.DMG_ASSET_PATH',
  '"dmg_sha256": process.env.DMG_SHA256',
  '"hdiutil_verify": process.env.HDIUTIL_VERIFY_STATUS',
  '"stapler_validation": process.env.STAPLER_VALIDATION_STATUS',
  '"dmg_gatekeeper_assessment": process.env.DMG_GATEKEEPER_ASSESSMENT_STATUS',
  '"readonly_attach_app_presence": process.env.READONLY_ATTACH_APP_PRESENCE_STATUS',
  '"app_codesign_verification": process.env.APP_CODESIGN_VERIFICATION_STATUS',
  '"app_gatekeeper_assessment": process.env.APP_GATEKEEPER_ASSESSMENT_STATUS',
  'node scripts/check-release-provenance-artifact.js "$provenance_path"',
  "Curiosity-Transcripts-latest.dmg.sha256",
  "Curiosity-Transcripts-latest.provenance.json",
  "actions/upload-artifact@v7",
  "actions/download-artifact@v8",
  "downloads/Curiosity-Transcripts-latest.dmg",
  "downloads/Curiosity-Transcripts-latest.dmg.sha256",
  "downloads/Curiosity-Transcripts-latest.provenance.json",
  "shasum -a 256 -c Curiosity-Transcripts-latest.dmg.sha256",
  "actions/configure-pages@v6",
  "actions/upload-pages-artifact@v3",
  "actions/deploy-pages@v5",
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

function stripYamlComments(text) {
  return text
    .split(/\r?\n/)
    .filter((line) => !line.trim().startsWith("#"))
    .map((line) => {
      let quote = null;
      for (let index = 0; index < line.length; index += 1) {
        const char = line[index];
        if ((char === "'" || char === '"') && line[index - 1] !== "\\") {
          quote = quote === char ? null : quote ?? char;
        }
        if (char === "#" && quote === null && /\s/.test(line[index - 1] ?? " ")) {
          return line.slice(0, index).trimEnd();
        }
      }
      return line;
    })
    .join("\n");
}

function indentation(line) {
  return line.match(/^\s*/)[0].length;
}

function keyPattern(key, indent) {
  return new RegExp(`^ {${indent}}${key.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}:\\s*(.*)$`);
}

function findKeyRange(lines, key, indent, start = 0, end = lines.length) {
  const pattern = keyPattern(key, indent);
  for (let index = start; index < end; index += 1) {
    if (!pattern.test(lines[index])) {
      continue;
    }

    let blockEnd = lines.length;
    for (let next = index + 1; next < end; next += 1) {
      if (lines[next].trim() !== "" && indentation(lines[next]) <= indent) {
        blockEnd = next;
        break;
      }
    }

    return { line: index, start: index + 1, end: blockEnd, value: lines[index].match(pattern)[1].trim() };
  }

  return null;
}

function requireKeyRange(lines, key, indent, start, end, description) {
  const range = findKeyRange(lines, key, indent, start, end);
  if (!range) {
    console.error(`::error file=.github/workflows/pages.yml::Missing ${description}`);
    ok = false;
  }
  return range;
}

function rangeText(lines, range) {
  return range ? lines.slice(range.line, range.end).join("\n") : "";
}

function findStepBlockContaining(lines, range, text, description) {
  if (!range) {
    return null;
  }

  for (let index = range.start; index < range.end; index += 1) {
    if (!/^ {6}- /.test(lines[index])) {
      continue;
    }

    let end = range.end;
    for (let next = index + 1; next < range.end; next += 1) {
      if (lines[next].trim() !== "" && indentation(lines[next]) <= 6) {
        end = next;
        break;
      }
    }

    const stepText = lines.slice(index, end).join("\n");
    if (stepText.includes(text)) {
      return { line: index, start: index + 1, end, text: stepText };
    }
  }

  console.error(`::error file=.github/workflows/pages.yml::Missing ${description}`);
  ok = false;
  return null;
}

function requireStepWithText(step, text, description) {
  if (!step) {
    return;
  }

  const withRange = findKeyRange(workflowLines, "with", 8, step.start, step.end);
  const withText = rangeText(workflowLines, withRange);
  if (!withRange || !withText.includes(text)) {
    console.error(`::error file=.github/workflows/pages.yml::Missing ${description}`);
    ok = false;
  }
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function firstLineContaining(text, needle) {
  const lines = text.split(/\r?\n/);
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].includes(needle)) {
      return index;
    }
  }
  return -1;
}

function stepBlocksBeforeCheckout(lines, range) {
  if (!range) {
    return [];
  }

  const checkoutLine = lines.findIndex(
    (line, index) => index >= range.start && index < range.end && line.includes("actions/checkout@v7"),
  );
  if (checkoutLine === -1) {
    console.error("::error file=.github/workflows/pages.yml::Missing checkout step in Pages DMG build job");
    ok = false;
    return [];
  }

  const blocks = [];
  for (let index = range.start; index < checkoutLine; index += 1) {
    if (!/^ {6}- name:\s+/.test(lines[index])) {
      continue;
    }

    let end = checkoutLine;
    for (let next = index + 1; next < checkoutLine; next += 1) {
      if (lines[next].trim() !== "" && indentation(lines[next]) <= 6) {
        end = next;
        break;
      }
    }

    blocks.push(lines.slice(index, end).join("\n"));
  }

  return blocks;
}

function requireFailingIfStepBeforeCheckout(stepBlocks, condition, description) {
  const ifPattern = new RegExp(`\\n {8}if:\\s*\\$\\{\\{\\s*${escapeRegExp(condition)}\\s*\\}\\}\\s*(?:\\n|$)`);
  const hasFailingStep = stepBlocks.some((block) => {
    const text = `\n${block}\n`;
    return ifPattern.test(text) && /\n {8}run:\s*\|\s*\n[\s\S]*\n {10}exit 1\s*\n/.test(text);
  });

  if (!hasFailingStep) {
    console.error(`::error file=.github/workflows/pages.yml::${description}`);
    ok = false;
  }
}

if (!fs.existsSync(workflowPath)) {
  console.error("::error file=.github/workflows/pages.yml::Missing GitHub Pages deployment workflow");
  process.exit(1);
}

const yaml = fs.readFileSync(workflowPath, "utf8");
const workflowText = stripYamlComments(yaml);
const workflowLines = workflowText.split(/\r?\n/);

for (const text of requiredText) {
  if (!workflowText.includes(text)) {
    console.error(`::error file=.github/workflows/pages.yml::Missing required Pages workflow content: ${text}`);
    ok = false;
  }
}

const onRange = requireKeyRange(workflowLines, "on", 0, 0, workflowLines.length, "top-level on block");
const workflowDispatchRange = onRange
  ? requireKeyRange(workflowLines, "workflow_dispatch", 2, onRange.start, onRange.end, "workflow_dispatch trigger")
  : null;
if (onRange && findKeyRange(workflowLines, "push", 2, onRange.start, onRange.end)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages latest DMG publication must not run on automatic main pushes; use manual workflow_dispatch after filled smoke evidence validation",
  );
  ok = false;
}

const inputsRange = workflowDispatchRange
  ? requireKeyRange(workflowLines, "inputs", 4, workflowDispatchRange.start, workflowDispatchRange.end, "workflow_dispatch inputs")
  : null;
const smokeInputRange = inputsRange
  ? requireKeyRange(
      workflowLines,
      "filled_smoke_evidence_validated",
      6,
      inputsRange.start,
      inputsRange.end,
      "filled smoke evidence validation input",
    )
  : null;
const smokeInputText = rangeText(workflowLines, smokeInputRange);
for (const [text, description] of [
  ["description:", "filled smoke evidence input description"],
  ["node scripts/check-release-smoke-evidence.js path/to/filled-evidence.json", "filled smoke evidence validator command"],
  ["required: true", "required filled smoke evidence input"],
  ["type: boolean", "boolean filled smoke evidence input"],
  ["default: false", "false default for filled smoke evidence input"],
]) {
  if (!smokeInputText.includes(text)) {
    console.error(`::error file=.github/workflows/pages.yml::Missing ${description}`);
    ok = false;
  }
}

const jobsRange = requireKeyRange(workflowLines, "jobs", 0, 0, workflowLines.length, "top-level jobs block");
const buildJobRange = jobsRange
  ? requireKeyRange(workflowLines, "build-macos-dmg", 2, jobsRange.start, jobsRange.end, "build-macos-dmg job")
  : null;
const buildPagesJobRange = jobsRange
  ? requireKeyRange(workflowLines, "build-pages", 2, jobsRange.start, jobsRange.end, "build-pages job")
  : null;
const environmentRange = buildJobRange
  ? requireKeyRange(workflowLines, "environment", 4, buildJobRange.start, buildJobRange.end, "macOS signing environment")
  : null;
const environmentText = rangeText(workflowLines, environmentRange);
if (!/name:\s*macos-signing/.test(environmentText)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages signing job must use the protected macos-signing environment",
  );
  ok = false;
}

const preCheckoutSteps = stepBlocksBeforeCheckout(workflowLines, buildJobRange);
requireFailingIfStepBeforeCheckout(
  preCheckoutSteps,
  "github.ref != 'refs/heads/main'",
  "Pages workflow must fail before checkout unless manually dispatched from refs/heads/main",
);
requireFailingIfStepBeforeCheckout(
  preCheckoutSteps,
  "inputs.filled_smoke_evidence_validated != true",
  "Pages workflow must fail before checkout unless filled smoke evidence validation is manually confirmed",
);

const publicationReadinessStepLine = exactRunStepLine(workflowText, "bash scripts/check-publication-readiness.sh");
const signingCredentialsStepLine = exactRunStepLine(workflowText, "./scripts/configure-apple-signing-ci.sh");
const buildDmgStepLine = exactRunStepLine(workflowText, "./scripts/build-macos-dmg.sh");
if (publicationReadinessStepLine === -1 || buildDmgStepLine === -1 || publicationReadinessStepLine > buildDmgStepLine) {
  console.error(
    "::error file=.github/workflows/pages.yml::Publication readiness must run before building the public Pages DMG",
  );
  ok = false;
}
if (signingCredentialsStepLine === -1 || buildDmgStepLine === -1 || signingCredentialsStepLine > buildDmgStepLine) {
  console.error(
    "::error file=.github/workflows/pages.yml::Apple signing credentials must be configured before building the public Pages DMG",
  );
  ok = false;
}
if (hasAdHocDmgBuildRunCommand(workflowText)) {
  console.error("::error file=.github/workflows/pages.yml::Public Pages workflow must not use ad-hoc DMG signing bypasses");
  ok = false;
}

const uploadArtifactStep = findStepBlockContaining(
  workflowLines,
  buildJobRange,
  "uses: actions/upload-artifact@v7",
  "DMG upload artifact step",
);
for (const [text, description] of [
  ["name: curiosity-transcripts-dmg", "DMG upload artifact name"],
  ["path: |", "DMG upload multiline path block"],
  ["pages-download/Curiosity-Transcripts-latest.dmg", "latest DMG upload path"],
  ["pages-download/Curiosity-Transcripts-latest.dmg.sha256", "latest checksum upload path"],
  ["pages-download/Curiosity-Transcripts-latest.provenance.json", "latest provenance upload path"],
  ["if-no-files-found: error", "DMG upload missing-file failure policy"],
]) {
  requireStepWithText(uploadArtifactStep, text, description);
}

const downloadArtifactStep = findStepBlockContaining(
  workflowLines,
  buildPagesJobRange,
  "uses: actions/download-artifact@v8",
  "DMG download artifact step",
);
for (const [text, description] of [
  ["name: curiosity-transcripts-dmg", "DMG download artifact name"],
  ["path: site/downloads", "DMG download target path"],
]) {
  requireStepWithText(downloadArtifactStep, text, description);
}

const pagesWorkflowOrdering = [
  [
    "hdiutil verify pages-download/Curiosity-Transcripts-latest.dmg",
    'hdiutil_verify_status="passed"',
    "hdiutil verify status must be recorded only after hdiutil verify succeeds",
  ],
  [
    "xcrun stapler validate pages-download/Curiosity-Transcripts-latest.dmg",
    'stapler_validation_status="passed"',
    "stapler validation status must be recorded only after stapler validation succeeds",
  ],
  [
    "spctl -a -vvv -t open --context context:primary-signature pages-download/Curiosity-Transcripts-latest.dmg",
    'dmg_gatekeeper_assessment_status="passed"',
    "DMG Gatekeeper assessment status must be recorded only after spctl open succeeds",
  ],
  [
    '[ ! -d "$mount_dir/Curiosity Transcripts.app" ]',
    'readonly_attach_app_presence_status="passed"',
    "read-only attach/app presence status must be recorded only after the mounted app exists",
  ],
  [
    'codesign --verify --deep --strict --verbose=2 "$mount_dir/Curiosity Transcripts.app"',
    'app_codesign_verification_status="passed"',
    "app codesign status must be recorded only after codesign verification succeeds",
  ],
  [
    'spctl -a -vvv -t exec "$mount_dir/Curiosity Transcripts.app"',
    'app_gatekeeper_assessment_status="passed"',
    "app Gatekeeper assessment status must be recorded only after spctl exec succeeds",
  ],
  [
    'shasum -a 256 "$latest_asset"',
    "fs.writeFileSync(process.env.PROVENANCE_PATH",
    "Pages latest provenance manifest must be written only after the DMG checksum is generated",
  ],
  [
    "fs.writeFileSync(process.env.PROVENANCE_PATH",
    'node scripts/check-release-provenance-artifact.js "$provenance_path"',
    "Pages latest provenance manifest must be machine-validated after it is written",
  ],
  [
    'node scripts/check-release-provenance-artifact.js "$provenance_path"',
    "uses: actions/upload-artifact@v7",
    "Pages latest provenance manifest must be machine-validated before artifact upload",
  ],
];

for (const [beforeText, afterText, message] of pagesWorkflowOrdering) {
  const beforeLine = firstLineContaining(workflowText, beforeText);
  const afterLine = firstLineContaining(workflowText, afterText);
  if (beforeLine === -1 || afterLine === -1 || beforeLine >= afterLine) {
    console.error(`::error file=.github/workflows/pages.yml::${message}`);
    ok = false;
  }
}

if (/ubuntu-latest[\s\S]*build-macos-dmg\.sh/.test(workflowText)) {
  console.error("::error file=.github/workflows/pages.yml::macOS DMG build must not run on ubuntu-latest");
  ok = false;
}

if (/runs-on:\s*macos-latest/.test(workflowText)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages DMG build must pin macos-26 for the ScreenCaptureKit/apple-metal SDK requirement",
  );
  ok = false;
}

if (/find\s+apps\/desktop\/src-tauri\/target\/release\/bundle\/dmg\s+-name\s+['"]?\*\.dmg['"]?\s+-type\s+f\s+-print\s+-quit/.test(workflowText)) {
  console.error(
    "::error file=.github/workflows/pages.yml::Pages latest DMG staging must use the deterministic versioned aarch64 path instead of first-match find",
  );
  ok = false;
}

process.exit(ok ? 0 : 1);
