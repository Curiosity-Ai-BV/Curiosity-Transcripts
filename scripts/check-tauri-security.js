const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const configPath = path.join(root, "apps", "desktop", "src-tauri", "tauri.conf.json");
const configLabel = "apps/desktop/src-tauri/tauri.conf.json";

const requiredDirectives = new Map([
  ["default-src", ["'self'"]],
  ["script-src", ["'self'"]],
  ["style-src", ["'self'"]],
  ["img-src", ["'self'"]],
  ["font-src", ["'self'"]],
  ["connect-src", ["ipc:", "http://ipc.localhost"]],
  ["object-src", ["'none'"]],
  ["base-uri", ["'none'"]],
  ["form-action", ["'none'"]],
  ["frame-ancestors", ["'none'"]],
  ["frame-src", ["'none'"]],
  ["worker-src", ["'none'"]],
  ["media-src", ["'none'"]],
]);

const forbiddenSources = new Set(["'unsafe-eval'", "'unsafe-inline'", "unsafe-eval", "unsafe-inline"]);

const strictFixture =
  "default-src 'self'; " +
  "script-src 'self'; " +
  "style-src 'self'; " +
  "img-src 'self'; " +
  "font-src 'self'; " +
  "connect-src ipc: http://ipc.localhost; " +
  "object-src 'none'; " +
  "base-uri 'none'; " +
  "form-action 'none'; " +
  "frame-ancestors 'none'; " +
  "frame-src 'none'; " +
  "worker-src 'none'; " +
  "media-src 'none'";

const rejectionCases = [
  {
    name: "null CSP leaves the renderer without a shipped policy",
    policy: null,
  },
  {
    name: "unsafe eval reopens string-to-code execution",
    policy: strictFixture.replace("script-src 'self'", "script-src 'self' 'unsafe-eval'"),
  },
  {
    name: "wildcard connect-src allows arbitrary renderer network access",
    policy: strictFixture.replace("connect-src ipc: http://ipc.localhost", "connect-src *"),
  },
  {
    name: "remote HTTPS origins do not belong in the local-first renderer",
    policy: strictFixture.replace(
      "connect-src ipc: http://ipc.localhost",
      "connect-src ipc: http://ipc.localhost https://api.example.com",
    ),
  },
  {
    name: "protocol-relative origins are remote origins",
    policy: strictFixture.replace(
      "connect-src ipc: http://ipc.localhost",
      "connect-src ipc: http://ipc.localhost //api.example.com",
    ),
  },
  {
    name: "bare host sources are remote origins",
    policy: strictFixture.replace(
      "connect-src ipc: http://ipc.localhost",
      "connect-src ipc: http://ipc.localhost api.example.com",
    ),
  },
  {
    name: "data sources are not needed by the current renderer policy",
    policy: strictFixture.replace("img-src 'self'", "img-src 'self' data:"),
  },
  {
    name: "blob sources are not needed by the current renderer policy",
    policy: strictFixture.replace("worker-src 'none'", "worker-src blob:"),
  },
  {
    name: "Tauri IPC sources are required for command invocation",
    policy: strictFixture.replace("connect-src ipc: http://ipc.localhost", "connect-src 'self'"),
  },
];

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function normalizeCsp(policy) {
  const errors = [];
  const directives = new Map();

  if (typeof policy === "string") {
    if (!policy.trim()) {
      return { directives, errors: ["app.security.csp must be a non-empty CSP string or directive object"] };
    }

    for (const rawDirective of policy.split(";")) {
      const trimmed = rawDirective.trim();
      if (!trimmed) {
        continue;
      }

      const [name, ...sources] = trimmed.split(/\s+/);
      addDirective(directives, errors, name, sources);
    }

    return { directives, errors };
  }

  if (!policy || typeof policy !== "object" || Array.isArray(policy)) {
    return { directives, errors: ["app.security.csp must be a non-empty CSP string or directive object"] };
  }

  for (const [name, rawSources] of Object.entries(policy)) {
    if (Array.isArray(rawSources)) {
      addDirective(directives, errors, name, rawSources);
    } else if (typeof rawSources === "string") {
      addDirective(directives, errors, name, rawSources.trim() ? rawSources.trim().split(/\s+/) : []);
    } else {
      errors.push(`${name} must use a string or string array source list`);
    }
  }

  return { directives, errors };
}

function addDirective(directives, errors, name, sources) {
  if (!name || directives.has(name)) {
    errors.push(`Duplicate or empty CSP directive: ${name || "(empty)"}`);
    return;
  }

  if (sources.some((source) => typeof source !== "string" || !source.trim())) {
    errors.push(`${name} must use non-empty string sources`);
    return;
  }

  directives.set(name, sources);
}

function validateCsp(policy) {
  const { directives, errors } = normalizeCsp(policy);

  for (const [directive, requiredSources] of requiredDirectives.entries()) {
    const sources = directives.get(directive);
    if (!sources) {
      errors.push(`Missing required directive: ${directive}`);
      continue;
    }

    for (const source of requiredSources) {
      if (!sources.includes(source)) {
        errors.push(`${directive} must include ${source}`);
      }
    }
  }

  for (const [directive, sources] of directives.entries()) {
    const allowedSources = requiredDirectives.get(directive);
    if (!allowedSources) {
      errors.push(`Unexpected directive: ${directive}`);
      continue;
    }

    if (sources.length === 0) {
      errors.push(`${directive} must not have an empty source list`);
    }

    for (const source of sources) {
      if (!allowedSources.includes(source)) {
        errors.push(`${directive} must not include unapproved source ${source}`);
      }

      if (forbiddenSources.has(source)) {
        errors.push(`${directive} must not include ${source}`);
      }

      if (source.includes("*")) {
        errors.push(`${directive} must not include wildcard source ${source}`);
      }

      if (isRemoteOrigin(source)) {
        errors.push(`${directive} must not include remote origin ${source}`);
      }

      if (source === "http:" || source === "https:" || source === "ws:" || source === "wss:") {
        errors.push(`${directive} must not include broad scheme source ${source}`);
      }
    }
  }

  return errors;
}

function isRemoteOrigin(source) {
  if (source === "http://ipc.localhost") {
    return false;
  }

  return /^(https?|wss?):\/\//.test(source);
}

for (const testCase of rejectionCases) {
  if (validateCsp(testCase.policy).length === 0) {
    fail("scripts/check-tauri-security.js", `Guardrail did not reject: ${testCase.name}`);
  }
}

if (!fs.existsSync(configPath)) {
  fail(configLabel, "Missing Tauri config");
} else {
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  const security = config.app?.security;

  if (!security || typeof security !== "object" || Array.isArray(security)) {
    fail(configLabel, "Missing app.security configuration");
  } else {
    for (const error of validateCsp(security.csp)) {
      fail(configLabel, error);
    }

    if (Object.prototype.hasOwnProperty.call(security, "devCsp")) {
      fail(configLabel, "Do not set app.security.devCsp until a separate restrictive dev policy is validated");
    }
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Tauri renderer CSP gate passed.");
