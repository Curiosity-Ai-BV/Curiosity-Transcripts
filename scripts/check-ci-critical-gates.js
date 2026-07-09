const fs = require("fs");
const path = require("path");

const repoRoot = path.resolve(__dirname, "..");
const workflowPath = path.join(repoRoot, ".github", "workflows", "ci.yml");
const workflowLabel = ".github/workflows/ci.yml";

const criticalSteps = [
  ["checks", "Check GitHub Actions workflow syntax"],
  ["checks", "Check critical CI gate metadata"],
  ["checks", "Check publication readiness"],
  ["checks", "Check GitHub Pages homepage"],
  ["checks", "Check GitHub Pages deployment workflow"],
  ["checks", "Check GitHub Release workflow"],
  ["checks", "Check Rust formatting"],
  ["checks", "Audit Rust workspace dependencies"],
  ["checks", "Check desktop Rust formatting"],
  ["checks", "Audit desktop Rust backend dependencies"],
  ["checks", "Test Rust workspace"],
  ["checks", "Test desktop Rust backend"],
  ["checks", "Test desktop Rust backend without Whisper"],
  ["checks", "Generate Rust coverage artifacts"],
  ["checks", "Lint Rust workspace"],
  ["checks", "Lint desktop Rust backend"],
  ["checks", "Check audio smoke fails loud without hardware request"],
  ["checks", "Check Whisper smoke fails loud without model inputs"],
  ["checks", "Generate supply-chain artifacts"],
  ["checks", "Check supply-chain artifacts"],
  ["checks", "Upload supply-chain artifacts"],
  ["checks", "Audit desktop npm dependencies"],
  ["checks", "Test desktop frontend"],
  ["checks", "Generate desktop command/view contract artifact"],
  ["checks", "Validate desktop command/view contract artifact"],
  ["checks", "Upload desktop command/view contract artifact"],
  ["checks", "Generate desktop frontend coverage"],
  ["checks", "Check coverage artifacts"],
  ["checks", "Upload coverage artifacts"],
  ["checks", "Build desktop frontend"],
  ["macos-system-audio-check", "Check desktop Rust backend with ScreenCaptureKit system audio"],
  ["macos-system-audio-check", "Check release desktop Rust backend with ScreenCaptureKit system audio"],
];

function parseSteps(text) {
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
    const stepMatch = line.match(/^ {6}- name:\s*(.+?)\s*$/);
    if (stepMatch) {
      if (current) {
        steps.push(current);
      }
      current = { name: stepMatch[1], job: currentJob, lines: [line] };
    } else if (current) {
      current.lines.push(line);
    }
  }

  if (current) {
    steps.push(current);
  }

  return steps;
}

function hasStepKey(step, key) {
  return step.lines.some((line) => new RegExp(`^ {8}${key}\\s*:`).test(line));
}

function validateTopLevelPermissions(text) {
  const errors = [];
  const lines = text.split(/\r?\n/);
  const permissionsDeclarations = lines
    .map((line, index) => {
      const match = line.match(/^(\s*)permissions\s*:/);
      return match ? { indent: match[1].length, index } : null;
    })
    .filter(Boolean);
  const topLevelPermissions = permissionsDeclarations.filter((declaration) => declaration.indent === 0);
  const jobsIndex = lines.findIndex((line) => /^jobs:\s*$/.test(line));

  if (topLevelPermissions.length === 0) {
    return ["CI workflow must declare top-level read-only permissions"];
  }
  if (topLevelPermissions.length > 1) {
    errors.push("CI workflow must declare exactly one top-level permissions block");
  }

  const overriddenPermissions = permissionsDeclarations.filter((declaration) => declaration.indent !== 0);
  if (overriddenPermissions.length > 0) {
    errors.push("CI workflow permissions must not be overridden below the workflow level");
  }

  const permissionsIndex = topLevelPermissions[0].index;
  if (jobsIndex !== -1 && permissionsIndex > jobsIndex) {
    errors.push("CI workflow permissions must be top-level before jobs");
  }

  const block = [];
  for (const line of lines.slice(permissionsIndex + 1)) {
    if (/^[A-Za-z0-9_-]+:\s*$/.test(line)) {
      break;
    }
    if (line.trim() !== "") {
      block.push(line);
    }
  }

  const normalized = block.map((line) => line.trim());
  if (normalized.length !== 1 || normalized[0] !== "contents: read") {
    errors.push("CI workflow permissions must be exactly contents: read");
  }

  return errors;
}

function validateCriticalGates(text) {
  const steps = parseSteps(text);
  const errors = validateTopLevelPermissions(text);

  for (const [job, name] of criticalSteps) {
    const matches = steps.filter((candidate) => candidate.job === job && candidate.name === name);
    if (matches.length === 0) {
      errors.push(`Missing critical CI gate: ${job} / ${name}`);
      continue;
    }
    if (matches.length > 1) {
      errors.push(`Critical CI gate must be unique: ${job} / ${name}`);
    }
    for (const step of matches) {
      if (hasStepKey(step, "if")) {
        errors.push(`Critical CI gate must not be conditionally skipped: ${job} / ${name}`);
      }
      if (hasStepKey(step, "continue-on-error")) {
        errors.push(`Critical CI gate must fail CI when its command fails: ${job} / ${name}`);
      }
    }
  }

  return errors;
}

function buildSelfTestWorkflow(options = {}) {
  const permissionsLines = Object.prototype.hasOwnProperty.call(options, "permissionsLines")
    ? options.permissionsLines
    : ["  contents: read"];
  const omitStep = options.omitStep;
  const duplicateStep = options.duplicateStep;
  const stepExtraLines = options.stepExtraLines ?? {};
  const jobs = new Map();

  for (const [job, name] of criticalSteps) {
    if (!jobs.has(job)) {
      jobs.set(job, []);
    }
    jobs.get(job).push(name);
  }

  const lines = ["name: CI", "", "on:", "  pull_request:", "  push:", ""];
  if (permissionsLines !== null) {
    lines.push("permissions:", ...permissionsLines, "");
  }
  lines.push("jobs:");

  for (const [job, names] of jobs) {
    lines.push(`  ${job}:`, "    runs-on: ubuntu-latest", "", "    steps:");
    for (const name of names) {
      if (omitStep && omitStep.job === job && omitStep.name === name) {
        continue;
      }
      const copies = duplicateStep && duplicateStep.job === job && duplicateStep.name === name ? 2 : 1;
      for (let copy = 0; copy < copies; copy += 1) {
        lines.push(`      - name: ${name}`);
        for (const extraLine of stepExtraLines[`${job}/${name}`] ?? []) {
          lines.push(`        ${extraLine}`);
        }
        lines.push(`        run: echo ${JSON.stringify(name)}`);
      }
    }
    lines.push("");
  }

  return lines.join("\n");
}

function failSelfTest(message) {
  console.error(`::error file=${workflowLabel}::${message}`);
  process.exit(1);
}

function expectSelfTestRejection(label, workflowText, expectedError) {
  const errors = validateCriticalGates(workflowText);
  if (!errors.some((error) => error.includes(expectedError))) {
    failSelfTest(
      `Self-test did not reject ${label}; expected error containing "${expectedError}", got: ${errors.join("; ") || "none"}`,
    );
  }
}

function runSelfTests() {
  const [targetJob, targetName] = criticalSteps[0];
  const targetStep = { job: targetJob, name: targetName };
  const targetKey = `${targetJob}/${targetName}`;
  const validWorkflow = buildSelfTestWorkflow();
  const validErrors = validateCriticalGates(validWorkflow);

  if (validErrors.length > 0) {
    failSelfTest(`Self-test rejected the valid critical gate fixture: ${validErrors.join("; ")}`);
  }

  expectSelfTestRejection(
    "required critical check missing",
    buildSelfTestWorkflow({ omitStep: targetStep }),
    `Missing critical CI gate: ${targetJob} / ${targetName}`,
  );
  expectSelfTestRejection(
    "critical check guarded by if",
    buildSelfTestWorkflow({ stepExtraLines: { [targetKey]: ["if: always()"] } }),
    `Critical CI gate must not be conditionally skipped: ${targetJob} / ${targetName}`,
  );
  expectSelfTestRejection(
    "critical check using continue-on-error",
    buildSelfTestWorkflow({ stepExtraLines: { [targetKey]: ["continue-on-error: true"] } }),
    `Critical CI gate must fail CI when its command fails: ${targetJob} / ${targetName}`,
  );
  expectSelfTestRejection(
    "CI permissions missing contents read",
    buildSelfTestWorkflow({ permissionsLines: null }),
    "CI workflow must declare top-level read-only permissions",
  );
  expectSelfTestRejection(
    "CI permissions weakened to contents write",
    buildSelfTestWorkflow({ permissionsLines: ["  contents: write"] }),
    "CI workflow permissions must be exactly contents: read",
  );
  expectSelfTestRejection(
    "duplicate critical steps",
    buildSelfTestWorkflow({ duplicateStep: targetStep }),
    `Critical CI gate must be unique: ${targetJob} / ${targetName}`,
  );
}

function runWorkflowCheck() {
  const text = fs.readFileSync(workflowPath, "utf8");
  const errors = validateCriticalGates(text);

  for (const error of errors) {
    console.error(`::error file=${workflowLabel}::${error}`);
  }

  if (errors.length > 0) {
    process.exit(1);
  }

  console.log("Critical CI gate metadata passed.");
}

const args = process.argv.slice(2);

if (args.length === 1 && args[0] === "--self-test") {
  runSelfTests();
  console.log("Critical CI gate metadata self-tests passed.");
  process.exit(0);
}

if (args.length > 0) {
  console.error("Usage: node scripts/check-ci-critical-gates.js");
  console.error("       node scripts/check-ci-critical-gates.js --self-test");
  process.exit(1);
}

runWorkflowCheck();
