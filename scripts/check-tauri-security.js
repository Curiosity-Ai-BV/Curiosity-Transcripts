const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const configPath = path.join(root, "apps", "desktop", "src-tauri", "tauri.conf.json");
const configLabel = "apps/desktop/src-tauri/tauri.conf.json";
const capabilitiesDir = path.join(root, "apps", "desktop", "src-tauri", "capabilities");
const capabilitiesLabel = "apps/desktop/src-tauri/capabilities";
const defaultCapabilityLabel = "apps/desktop/src-tauri/capabilities/default.json";

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
const approvedCapabilityFile = "default.json";
const approvedCapabilityIdentifier = "main-window-default";
const approvedCapabilityWindows = ["main"];
const approvedCapabilityPermissions = ["core:default", "dialog:allow-open"];
const broadPermissionPrefixes = ["fs:", "shell:", "http:"];

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

const approvedCapabilityFixture = {
  files: [
    {
      name: "default.json",
      json: {
        identifier: "main-window-default",
        windows: ["main"],
        permissions: ["core:default", "dialog:allow-open"],
      },
    },
  ],
};

const capabilityRejectionCases = [
  {
    name: "extra capability files broaden the desktop permission surface",
    surface: {
      files: [
        ...approvedCapabilityFixture.files,
        {
          name: "extra.json",
          json: {
            identifier: "extra",
            windows: ["main"],
            permissions: ["core:default"],
          },
        },
      ],
    },
  },
  {
    name: "extra TOML capability files broaden the desktop permission surface",
    surface: {
      files: [
        ...approvedCapabilityFixture.files,
        {
          name: "extra.toml",
          parseError: "TOML capability files are not part of the approved release surface",
        },
      ],
    },
  },
  {
    name: "filesystem wildcard permissions are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["main"],
            permissions: ["core:default", "dialog:allow-open", "fs:*"],
          },
        },
      ],
    },
  },
  {
    name: "shell wildcard permissions are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["main"],
            permissions: ["core:default", "dialog:allow-open", "shell:*"],
          },
        },
      ],
    },
  },
  {
    name: "HTTP wildcard permissions are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["main"],
            permissions: ["core:default", "dialog:allow-open", "http:*"],
          },
        },
      ],
    },
  },
  {
    name: "wildcard permissions are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["main"],
            permissions: ["core:default", "dialog:allow-open", "*"],
          },
        },
      ],
    },
  },
  {
    name: "object-shaped permissions are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["main"],
            permissions: ["core:default", "dialog:allow-open", { identifier: "fs:allow-home-read" }],
          },
        },
      ],
    },
  },
  {
    name: "wildcard windows are not approved",
    surface: {
      files: [
        {
          name: "default.json",
          json: {
            identifier: "main-window-default",
            windows: ["*"],
            permissions: ["core:default", "dialog:allow-open"],
          },
        },
      ],
    },
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

function validateSecurityConfig(security) {
  const errors = [];

  if (!security || typeof security !== "object" || Array.isArray(security)) {
    return ["Missing app.security configuration"];
  }

  errors.push(...validateCsp(security.csp));

  if (Object.prototype.hasOwnProperty.call(security, "devCsp")) {
    errors.push("Do not set app.security.devCsp until a separate restrictive dev policy is validated");
  }

  if (Object.prototype.hasOwnProperty.call(security, "capabilities")) {
    errors.push("Do not set app.security.capabilities; keep release capabilities pinned in capabilities/default.json");
  }

  return errors;
}

function arraysEqual(actual, expected) {
  return (
    Array.isArray(actual) &&
    actual.length === expected.length &&
    actual.every((value, index) => value === expected[index])
  );
}

function validateCapabilitySurface(surface) {
  const errors = [];
  const files = Array.isArray(surface?.files) ? surface.files : [];
  const fileNames = files.map((file) => file.name).sort();

  if (!fileNames.includes(approvedCapabilityFile)) {
    errors.push(`Missing approved Tauri capability file: ${approvedCapabilityFile}`);
  }

  const extraFiles = fileNames.filter((name) => name !== approvedCapabilityFile);
  if (extraFiles.length > 0) {
    errors.push(`Unexpected Tauri capability file(s): ${extraFiles.join(", ")}`);
  }

  const defaultFile = files.find((file) => file.name === approvedCapabilityFile);
  if (!defaultFile) {
    return errors;
  }

  if (defaultFile.parseError) {
    errors.push(`${approvedCapabilityFile} must contain valid JSON: ${defaultFile.parseError}`);
    return errors;
  }

  const capability = defaultFile.json;
  if (!capability || typeof capability !== "object" || Array.isArray(capability)) {
    errors.push(`${approvedCapabilityFile} must contain a JSON object`);
    return errors;
  }

  if (capability.identifier !== approvedCapabilityIdentifier) {
    errors.push(`${approvedCapabilityFile} identifier must be ${approvedCapabilityIdentifier}`);
  }

  if (!arraysEqual(capability.windows, approvedCapabilityWindows)) {
    errors.push(`${approvedCapabilityFile} windows must be exactly ["main"]`);
  }

  if (Array.isArray(capability.windows)) {
    for (const windowName of capability.windows) {
      if (windowName === "*") {
        errors.push(`${approvedCapabilityFile} must not include wildcard window ${windowName}`);
      }
    }
  }

  if (!arraysEqual(capability.permissions, approvedCapabilityPermissions)) {
    errors.push(
      `${approvedCapabilityFile} permissions must be exactly ["core:default", "dialog:allow-open"]`,
    );
  }

  if (Array.isArray(capability.permissions)) {
    for (const permission of capability.permissions) {
      if (typeof permission !== "string" || !permission.trim()) {
        errors.push(`${approvedCapabilityFile} permissions must contain only non-empty strings`);
        continue;
      }

      if (permission === "*" || permission.includes("*")) {
        errors.push(`${approvedCapabilityFile} must not include wildcard permission ${permission}`);
      }

      if (
        broadPermissionPrefixes.some((prefix) => permission === `${prefix}*` || permission.startsWith(prefix)) &&
        !approvedCapabilityPermissions.includes(permission)
      ) {
        errors.push(`${approvedCapabilityFile} must not include broad native API permission ${permission}`);
      }
    }
  }

  return errors;
}

function readCapabilitySurface() {
  if (!fs.existsSync(capabilitiesDir)) {
    return { files: [] };
  }
  if (!fs.statSync(capabilitiesDir).isDirectory()) {
    return {
      files: [],
      surfaceError: "Tauri capabilities path must be a directory",
    };
  }

  const files = fs.readdirSync(capabilitiesDir, { withFileTypes: true }).map((entry) => {
    if (!entry.isFile()) {
      return {
        name: entry.name,
        parseError: "Capability entries must be files",
      };
    }

    if (!entry.name.endsWith(".json")) {
      return {
        name: entry.name,
        parseError: "Only the approved default.json capability file is allowed",
      };
    }

    const filePath = path.join(capabilitiesDir, entry.name);
    try {
      return {
        name: entry.name,
        json: JSON.parse(fs.readFileSync(filePath, "utf8")),
      };
    } catch (error) {
      return {
        name: entry.name,
        parseError: error.message,
      };
    }
  });

  return { files };
}

for (const testCase of rejectionCases) {
  if (validateCsp(testCase.policy).length === 0) {
    fail("scripts/check-tauri-security.js", `Guardrail did not reject: ${testCase.name}`);
  }
}

if (typeof validateCapabilitySurface !== "function") {
  fail("scripts/check-tauri-security.js", "Capability drift validator is not implemented");
} else {
  if (validateCapabilitySurface(approvedCapabilityFixture).length !== 0) {
    fail("scripts/check-tauri-security.js", "Guardrail rejected the approved Tauri capability fixture");
  }

  for (const testCase of capabilityRejectionCases) {
    if (validateCapabilitySurface(testCase.surface).length === 0) {
      fail("scripts/check-tauri-security.js", `Guardrail did not reject: ${testCase.name}`);
    }
  }
}

if (validateSecurityConfig({ csp: strictFixture }).length !== 0) {
  fail("scripts/check-tauri-security.js", "Guardrail rejected the approved Tauri security fixture");
}
if (validateSecurityConfig({ csp: strictFixture, capabilities: [] }).length === 0) {
  fail("scripts/check-tauri-security.js", "Guardrail did not reject inline app.security.capabilities");
}

if (!fs.existsSync(configPath)) {
  fail(configLabel, "Missing Tauri config");
} else {
  const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
  for (const error of validateSecurityConfig(config.app?.security)) {
    fail(configLabel, error);
  }
}

if (!fs.existsSync(capabilitiesDir)) {
  fail(capabilitiesLabel, "Missing Tauri capabilities directory");
}

const capabilitySurface = readCapabilitySurface();
if (capabilitySurface.surfaceError) {
  fail(capabilitiesLabel, capabilitySurface.surfaceError);
}
for (const error of validateCapabilitySurface(capabilitySurface)) {
  fail(defaultCapabilityLabel, error);
}

if (!ok) {
  process.exit(1);
}

console.log("Tauri renderer CSP and capability gates passed.");
