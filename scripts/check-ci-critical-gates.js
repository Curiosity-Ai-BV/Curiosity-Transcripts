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
  ["checks", "Upload supply-chain artifacts"],
  ["checks", "Audit desktop npm dependencies"],
  ["checks", "Test desktop frontend"],
  ["checks", "Generate desktop command/view contract artifact"],
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

const text = fs.readFileSync(workflowPath, "utf8");
const errors = validateCriticalGates(text);

for (const error of errors) {
  console.error(`::error file=${workflowLabel}::${error}`);
}

if (errors.length > 0) {
  process.exit(1);
}

console.log("Critical CI gate metadata passed.");
