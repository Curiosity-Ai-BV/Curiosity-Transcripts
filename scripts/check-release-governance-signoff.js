#!/usr/bin/env node
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const repoRoot = path.resolve(__dirname, "..");
const scriptLabel = "scripts/check-release-governance-signoff.js";
const defaultSignoffPath = path.join(repoRoot, "docs", "release-governance-signoff.template.json");

const expectedCheckIds = [
  "protected-v-tags",
  "release-branch-rules",
  "codeql-alerts-triaged",
  "gitleaks-alerts-triaged",
  "github-secret-scanning-alerts-triaged",
  "publisher-authorized",
];

const semverPattern =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const gitShaPattern = /^[0-9a-f]{40}$/i;
const iso8601TimestampWithTimezonePattern =
  /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?(Z|([+-])(\d{2}):(\d{2}))$/;
const weakEvidenceValues = new Set([
  "-",
  "example",
  "fill me",
  "n/a",
  "na",
  "none",
  "pending",
  "placeholder",
  "sample",
  "tbd",
  "template",
  "todo",
  "unknown",
]);
const weakEvidencePrefixPattern =
  /^(?:todo|pending|tbd|placeholder|unknown|n\/a|na|example|sample|fill me)(?:\s*:|\s+|$)/i;

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function normalize(value) {
  return String(value ?? "")
    .toLowerCase()
    .replace(/[`*_]/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function isPlainObject(value) {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function valueAt(object, pathParts) {
  let current = object;
  for (const part of pathParts) {
    if (!isPlainObject(current) || !(part in current)) {
      return undefined;
    }
    current = current[part];
  }
  return current;
}

function hasExternalBoundary(signoff) {
  const normalized = normalize(signoff.externalBoundary);
  return (
    normalized.includes("repo local automation cannot verify github branch protection tag rulesets") &&
    normalized.includes("live code scanning") &&
    normalized.includes("secret scanning alert state")
  );
}

function hasStrongText(value) {
  if (typeof value !== "string") {
    return false;
  }
  const trimmed = value.trim();
  const normalized = normalize(trimmed);
  return trimmed.length >= 12 && !weakEvidenceValues.has(normalized) && !weakEvidencePrefixPattern.test(trimmed);
}

function collectStrings(value, strings = []) {
  if (typeof value === "string") {
    strings.push(value);
  } else if (Array.isArray(value)) {
    for (const item of value) {
      collectStrings(item, strings);
    }
  } else if (isPlainObject(value)) {
    for (const item of Object.values(value)) {
      collectStrings(item, strings);
    }
  }
  return strings;
}

function hasStrongEvidenceObject(evidence) {
  if (!isPlainObject(evidence)) {
    return false;
  }
  const strings = collectStrings(evidence);
  return strings.length > 0 && strings.every(hasStrongText);
}

function isValidIso8601TimestampWithTimezone(value) {
  const match = typeof value === "string" ? value.match(iso8601TimestampWithTimezonePattern) : null;
  if (!match) {
    return false;
  }

  const [, yearText, monthText, dayText, hourText, minuteText, secondText, fractionText, zoneText, offsetSign, offsetHourText, offsetMinuteText] =
    match;
  const year = Number(yearText);
  const month = Number(monthText);
  const day = Number(dayText);
  const hour = Number(hourText);
  const minute = Number(minuteText);
  const second = Number(secondText);
  const millisecond = fractionText ? Number(fractionText.slice(1, 4).padEnd(3, "0")) : 0;

  if (
    month < 1 ||
    month > 12 ||
    hour > 23 ||
    minute > 59 ||
    second > 59
  ) {
    return false;
  }

  const localDate = new Date(Date.UTC(year, month - 1, day, hour, minute, second, millisecond));
  if (
    localDate.getUTCFullYear() !== year ||
    localDate.getUTCMonth() !== month - 1 ||
    localDate.getUTCDate() !== day ||
    localDate.getUTCHours() !== hour ||
    localDate.getUTCMinutes() !== minute ||
    localDate.getUTCSeconds() !== second
  ) {
    return false;
  }

  if (zoneText === "Z") {
    return true;
  }

  const offsetHour = Number(offsetHourText);
  const offsetMinute = Number(offsetMinuteText);
  if (offsetHour > 23 || offsetMinute > 59) {
    return false;
  }
  const offsetMinutes = offsetHour * 60 + offsetMinute;
  const signedOffsetMinutes = offsetSign === "+" ? offsetMinutes : -offsetMinutes;
  return !Number.isNaN(localDate.getTime() - signedOffsetMinutes * 60 * 1000);
}

function pushCommonShapeErrors(signoff, errors) {
  if (!isPlainObject(signoff)) {
    errors.push("sign-off root must be a JSON object");
    return;
  }
  if (signoff.kind !== "release-governance-signoff") {
    errors.push('kind must be "release-governance-signoff"');
  }
  if (signoff.schemaVersion !== 1) {
    errors.push("schemaVersion must be 1");
  }
  if (!hasExternalBoundary(signoff)) {
    errors.push(
      "externalBoundary must state that repo-local automation cannot verify GitHub branch protection, tag rulesets, or live code-scanning/secret-scanning alert state",
    );
  }
  if (!Array.isArray(signoff.checks)) {
    errors.push("checks must be an array");
    return;
  }
  if (signoff.checks.length !== expectedCheckIds.length) {
    errors.push(`checks must contain exactly ${expectedCheckIds.length} governance items`);
  }
  expectedCheckIds.forEach((expectedId, index) => {
    const item = signoff.checks[index];
    if (!isPlainObject(item)) {
      errors.push(`checks[${index}] must be an object for ${expectedId}`);
      return;
    }
    if (item.id !== expectedId) {
      errors.push(`checks[${index}].id must be "${expectedId}"`);
    }
  });
}

function validateTemplate(signoff) {
  const errors = [];
  pushCommonShapeErrors(signoff, errors);
  if (!isPlainObject(signoff)) {
    return errors;
  }

  if (signoff.template !== true) {
    errors.push("template must be true for the checked-in governance sign-off template");
  }
  if (signoff.templateStatus !== "pending") {
    errors.push('templateStatus must be "pending"');
  }

  for (const field of [
    ["release", "version"],
    ["release", "gitRef"],
    ["release", "gitSha"],
    ["signer", "signerName"],
    ["signer", "signerRole"],
    ["signer", "signedAt"],
    ["publisherAuthorization", "publisherName"],
    ["publisherAuthorization", "authorizationEvidence"],
  ]) {
    const value = valueAt(signoff, field);
    if (typeof value !== "string" || !/^PENDING:/i.test(value.trim())) {
      errors.push(`${field.join(".")} must be a pending placeholder`);
    }
  }

  for (const [index, item] of (signoff.checks ?? []).entries()) {
    if (!isPlainObject(item)) {
      continue;
    }
    if (item.status !== "pending") {
      errors.push(`checks[${index}].status must be "pending" in the template`);
    }
    const evidenceStrings = collectStrings(item.evidence);
    if (evidenceStrings.length === 0) {
      errors.push(`checks[${index}].evidence must include pending evidence placeholders`);
    } else if (!evidenceStrings.every((value) => /^PENDING:/i.test(value.trim()))) {
      errors.push(`checks[${index}].evidence must remain pending-only in the template`);
    }
  }

  return errors;
}

function validateFilledSignoff(signoff) {
  const errors = [];
  pushCommonShapeErrors(signoff, errors);
  if (!isPlainObject(signoff)) {
    return errors;
  }

  if (signoff.template === true || signoff.templateStatus === "pending") {
    errors.push("filled sign-off must not be the pending template");
  }

  const version = valueAt(signoff, ["release", "version"]);
  const gitRef = valueAt(signoff, ["release", "gitRef"]);
  const gitSha = valueAt(signoff, ["release", "gitSha"]);
  if (typeof version !== "string" || !semverPattern.test(version)) {
    errors.push("release.version must be valid SemVer without a leading v");
  }
  if (typeof gitRef !== "string" || gitRef !== `refs/tags/v${version}`) {
    errors.push("release.gitRef must match refs/tags/v${release.version}");
  }
  if (typeof gitSha !== "string" || !gitShaPattern.test(gitSha)) {
    errors.push("release.gitSha must be a full 40-character Git SHA");
  }

  for (const field of [
    ["signer", "signerName"],
    ["signer", "signerRole"],
    ["publisherAuthorization", "publisherName"],
    ["publisherAuthorization", "authorizationEvidence"],
  ]) {
    if (!hasStrongText(valueAt(signoff, field))) {
      errors.push(`${field.join(".")} must be present and non-placeholder`);
    }
  }

  const signedAt = valueAt(signoff, ["signer", "signedAt"]);
  if (!isValidIso8601TimestampWithTimezone(signedAt)) {
    errors.push("signer.signedAt must be a valid ISO-8601 timestamp with timezone");
  }

  for (const [index, item] of (signoff.checks ?? []).entries()) {
    if (!isPlainObject(item)) {
      continue;
    }
    if (item.status !== "confirmed") {
      errors.push(`checks[${index}].status must be "confirmed" for filled sign-off evidence`);
    }
    if (!hasStrongEvidenceObject(item.evidence)) {
      errors.push(`checks[${index}].evidence must be present and non-placeholder`);
    }
  }

  return errors;
}

function parseJsonFile(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

function labelFor(filePath) {
  const relative = path.relative(repoRoot, filePath);
  if (!relative.startsWith("..") && !path.isAbsolute(relative)) {
    return relative;
  }
  return filePath;
}

function createFilledFixture() {
  return {
    kind: "release-governance-signoff",
    schemaVersion: 1,
    template: false,
    externalBoundary:
      "Repo-local automation cannot verify GitHub branch protection, tag rulesets, or live code-scanning/secret-scanning alert state.",
    release: {
      version: "1.2.3",
      gitRef: "refs/tags/v1.2.3",
      gitSha: "0123456789abcdef0123456789abcdef01234567",
    },
    signer: {
      signerName: "Ada Maintainer",
      signerRole: "Release maintainer",
      signedAt: "2026-07-09T12:34:56Z",
    },
    publisherAuthorization: {
      publisherName: "Ada Maintainer",
      authorizationEvidence: "Maintainer roster confirms Ada Maintainer is authorized for manual release publication.",
    },
    checks: expectedCheckIds.map((id) => ({
      id,
      status: "confirmed",
      evidence: {
        summary: `Maintainer reviewed ${id} for release 1.2.3 and recorded the external GitHub evidence.`,
        references: [`release-governance/1.2.3/${id}.md`],
      },
    })),
  };
}

function expectAccepted(name, errors) {
  if (errors.length > 0) {
    fail(scriptLabel, `Self-test expected ${name} to pass, got: ${errors.join("; ")}`);
  }
}

function expectRejected(name, errors, expectedMessagePart) {
  if (errors.length === 0) {
    fail(scriptLabel, `Self-test expected ${name} to fail`);
    return;
  }
  if (!errors.some((error) => error.includes(expectedMessagePart))) {
    fail(scriptLabel, `Self-test expected ${name} to mention "${expectedMessagePart}", got: ${errors.join("; ")}`);
  }
}

function runSelfTests() {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "release-governance-signoff-"));
  try {
    const validPath = path.join(tempDir, "filled-signoff.json");
    fs.writeFileSync(validPath, `${JSON.stringify(createFilledFixture(), null, 2)}\n`);
    expectAccepted("valid filled sign-off fixture", validateFilledSignoff(parseJsonFile(validPath)));

    const templatePath = path.join(tempDir, "template.json");
    const templateFixture = parseJsonFile(defaultSignoffPath);
    fs.writeFileSync(templatePath, `${JSON.stringify(templateFixture, null, 2)}\n`);
    expectAccepted("checked-in template fixture", validateTemplate(parseJsonFile(templatePath)));
    expectRejected("template as filled sign-off", validateFilledSignoff(templateFixture), "pending template");

    const missingTemplatePublisherName = JSON.parse(JSON.stringify(templateFixture));
    delete missingTemplatePublisherName.publisherAuthorization.publisherName;
    expectRejected(
      "template missing publisher name placeholder",
      validateTemplate(missingTemplatePublisherName),
      "publisherAuthorization.publisherName",
    );

    const missingRequiredField = createFilledFixture();
    delete missingRequiredField.release.gitSha;
    expectRejected("missing gitSha", validateFilledSignoff(missingRequiredField), "release.gitSha");

    const wrongOrder = createFilledFixture();
    [wrongOrder.checks[0], wrongOrder.checks[1]] = [wrongOrder.checks[1], wrongOrder.checks[0]];
    expectRejected("wrong check ID order", validateFilledSignoff(wrongOrder), "checks[0].id");

    const pendingStatus = createFilledFixture();
    pendingStatus.checks[0].status = "pending";
    expectRejected("pending check status", validateFilledSignoff(pendingStatus), "checks[0].status");

    const weakEvidence = createFilledFixture();
    weakEvidence.checks[0].evidence.summary = "PENDING: attach proof";
    expectRejected("weak evidence", validateFilledSignoff(weakEvidence), "checks[0].evidence");

    for (const placeholderEvidence of [
      "TODO: attach evidence from GitHub ruleset page",
      "PENDING attach evidence from GitHub ruleset page",
      "TBD: attach evidence from GitHub ruleset page",
      "PLACEHOLDER attach evidence from GitHub ruleset page",
      "UNKNOWN: attach evidence from GitHub ruleset page",
      "N/A: attach evidence from GitHub ruleset page",
      "EXAMPLE: attach evidence from GitHub ruleset page",
      "EXAMPLE attach evidence from GitHub ruleset page",
      "SAMPLE: attach evidence from GitHub ruleset page",
      "SAMPLE attach evidence from GitHub ruleset page",
      "FILL ME: attach evidence from GitHub ruleset page",
      "FILL ME with GitHub ruleset evidence",
    ]) {
      const placeholderPrefixEvidence = createFilledFixture();
      placeholderPrefixEvidence.checks[0].evidence.summary = placeholderEvidence;
      expectRejected(
        `placeholder-prefixed evidence ${placeholderEvidence}`,
        validateFilledSignoff(placeholderPrefixEvidence),
        "checks[0].evidence",
      );
    }

    const standalonePlaceholderEvidence = createFilledFixture();
    standalonePlaceholderEvidence.checks[0].evidence.summary = "none";
    expectRejected(
      "standalone placeholder evidence",
      validateFilledSignoff(standalonePlaceholderEvidence),
      "checks[0].evidence",
    );

    const realisticPlaceholderWordEvidence = createFilledFixture();
    realisticPlaceholderWordEvidence.checks[1].evidence.summary =
      "Reviewed the release branch ruleset template and confirmed it is active for main.";
    realisticPlaceholderWordEvidence.checks[4].evidence.summary =
      "GitHub secret scanning shows none open after triage on 2026-07-09.";
    expectAccepted(
      "realistic evidence containing placeholder words in context",
      validateFilledSignoff(realisticPlaceholderWordEvidence),
    );

    const badVersion = createFilledFixture();
    badVersion.release.version = "v1.2.3";
    expectRejected("malformed SemVer", validateFilledSignoff(badVersion), "release.version");

    const leadingZeroPrereleaseVersion = createFilledFixture();
    leadingZeroPrereleaseVersion.release.version = "1.2.3-01";
    leadingZeroPrereleaseVersion.release.gitRef = "refs/tags/v1.2.3-01";
    expectRejected(
      "numeric prerelease identifier with leading zero",
      validateFilledSignoff(leadingZeroPrereleaseVersion),
      "release.version",
    );

    const mismatchedRef = createFilledFixture();
    mismatchedRef.release.gitRef = "refs/tags/v1.2.4";
    expectRejected("mismatched gitRef", validateFilledSignoff(mismatchedRef), "release.gitRef");

    const malformedSha = createFilledFixture();
    malformedSha.release.gitSha = "abc123";
    expectRejected("malformed gitSha", validateFilledSignoff(malformedSha), "release.gitSha");

    const weakSigner = createFilledFixture();
    weakSigner.signer.signerName = "TBD";
    expectRejected("weak signer evidence", validateFilledSignoff(weakSigner), "signer.signerName");

    const placeholderPrefixSigner = createFilledFixture();
    placeholderPrefixSigner.signer.signerName = "EXAMPLE: maintainer name";
    expectRejected(
      "placeholder-prefixed signer name",
      validateFilledSignoff(placeholderPrefixSigner),
      "signer.signerName",
    );

    const weakPublisher = createFilledFixture();
    weakPublisher.publisherAuthorization.authorizationEvidence = "placeholder";
    expectRejected(
      "weak publisher authorization evidence",
      validateFilledSignoff(weakPublisher),
      "publisherAuthorization.authorizationEvidence",
    );

    const placeholderPrefixPublisher = createFilledFixture();
    placeholderPrefixPublisher.publisherAuthorization.authorizationEvidence =
      "FILL ME with publisher authorization evidence";
    expectRejected(
      "placeholder-prefixed publisher authorization",
      validateFilledSignoff(placeholderPrefixPublisher),
      "publisherAuthorization.authorizationEvidence",
    );

    const malformedSignedAt = createFilledFixture();
    malformedSignedAt.signer.signedAt = "2026-07-09";
    expectRejected("malformed signedAt", validateFilledSignoff(malformedSignedAt), "signer.signedAt");

    const impossibleSignedAtDate = createFilledFixture();
    impossibleSignedAtDate.signer.signedAt = "2026-02-31T12:00:00Z";
    expectRejected(
      "impossible signedAt calendar date",
      validateFilledSignoff(impossibleSignedAtDate),
      "signer.signedAt",
    );

    const missingBoundary = createFilledFixture();
    delete missingBoundary.externalBoundary;
    expectRejected("missing external boundary", validateFilledSignoff(missingBoundary), "externalBoundary");
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const options = {
    selfTest: false,
    signoffPath: defaultSignoffPath,
    pathProvided: false,
  };

  for (const arg of argv) {
    if (arg === "--self-test") {
      options.selfTest = true;
    } else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else if (!options.pathProvided) {
      options.signoffPath = path.resolve(arg);
      options.pathProvided = true;
    } else {
      fail(scriptLabel, `Unexpected argument: ${arg}`);
    }
  }

  return options;
}

const options = parseArgs(process.argv.slice(2));

if (options.help) {
  console.log(
    [
      "Usage: node scripts/check-release-governance-signoff.js [path/to/filled-signoff.json]",
      "       node scripts/check-release-governance-signoff.js --self-test",
      "",
      "Default path: docs/release-governance-signoff.template.json",
      "No path validates only the checked-in pending template shape.",
      'Path-based validation expects filled sign-off evidence with every check status set to "confirmed".',
      "This script never calls GitHub APIs and does not verify live GitHub settings or alert state.",
    ].join("\n"),
  );
  process.exit(ok ? 0 : 1);
}

if (options.selfTest) {
  runSelfTests();
  if (!ok) {
    process.exit(1);
  }
  console.log("Release governance sign-off self-tests passed.");
  process.exit(0);
}

const signoffLabel = labelFor(options.signoffPath);
let signoff;

try {
  signoff = parseJsonFile(options.signoffPath);
} catch (error) {
  fail(signoffLabel, `Unable to read or parse release governance sign-off: ${error.message}`);
}

if (signoff) {
  const errors = options.pathProvided ? validateFilledSignoff(signoff) : validateTemplate(signoff);
  for (const error of errors) {
    fail(signoffLabel, error);
  }
}

if (!ok) {
  process.exit(1);
}
