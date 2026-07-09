const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const scriptLabel = "scripts/check-plain-secret-storage.js";

const artifacts = {
  store: {
    path: path.join(root, "crates", "store", "src", "lib.rs"),
    label: "crates/store/src/lib.rs",
  },
  storeTests: {
    path: path.join(root, "crates", "store", "tests", "app_settings.rs"),
    label: "crates/store/tests/app_settings.rs",
  },
  appCrate: {
    path: path.join(root, "crates", "app", "src", "lib.rs"),
    label: "crates/app/src/lib.rs",
  },
  main: {
    path: path.join(root, "apps", "desktop", "src-tauri", "src", "main.rs"),
    label: "apps/desktop/src-tauri/src/main.rs",
  },
  calendar: {
    path: path.join(root, "apps", "desktop", "src-tauri", "src", "calendar.rs"),
    label: "apps/desktop/src-tauri/src/calendar.rs",
  },
  adapter: {
    path: path.join(root, "apps", "desktop", "src", "commandAdapter.ts"),
    label: "apps/desktop/src/commandAdapter.ts",
  },
  app: {
    path: path.join(root, "apps", "desktop", "src", "App.tsx"),
    label: "apps/desktop/src/App.tsx",
  },
  fixture: {
    path: path.join(
      root,
      "apps",
      "desktop",
      "contracts",
      "desktop-command-view-contract.fixture.json",
    ),
    label: "apps/desktop/contracts/desktop-command-view-contract.fixture.json",
  },
  schema: {
    path: path.join(
      root,
      "apps",
      "desktop",
      "contracts",
      "desktop-command-view-contract.schema.json",
    ),
    label: "apps/desktop/contracts/desktop-command-view-contract.schema.json",
  },
};

const checkedArtifacts = [
  artifacts.store,
  artifacts.storeTests,
  artifacts.appCrate,
  artifacts.main,
  artifacts.calendar,
  artifacts.adapter,
  artifacts.app,
  artifacts.fixture,
  artifacts.schema,
];

const exactForbiddenNames = new Map([
  ["apikey", "apiKey"],
  ["providerkey", "providerKey"],
  ["oauthtoken", "oauthToken"],
  ["accesstoken", "accessToken"],
  ["refreshtoken", "refreshToken"],
  ["calendartoken", "calendarToken"],
  ["encryptionkey", "encryptionKey"],
  ["hostedprovidersecret", "hostedProviderSecret"],
  ["secret", "secret"],
  ["credential", "credential"],
  ["credentials", "credentials"],
  ["password", "password"],
]);
const genericForbiddenTokens = new Set(["secret", "credential", "credentials", "password"]);

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function readText(artifact) {
  if (!fs.existsSync(artifact.path)) {
    fail(artifact.label, "Missing plain secret storage guard artifact");
    return "";
  }
  return fs.readFileSync(artifact.path, "utf8");
}

function readJson(artifact) {
  const source = readText(artifact);
  if (!source) {
    return null;
  }
  try {
    return JSON.parse(source);
  } catch (error) {
    fail(artifact.label, `Unable to read or parse JSON: ${error.message}`);
    return null;
  }
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function mutateText(name, source, from, to) {
  if (!source.includes(from)) {
    fail(scriptLabel, `Guardrail fixture could not find mutation target: ${name}`);
    return source;
  }
  const mutated = source.replace(from, to);
  if (mutated === source) {
    fail(scriptLabel, `Guardrail fixture did not mutate source: ${name}`);
  }
  return mutated;
}

function addObjectKey(name, target, key, value) {
  if (!target || typeof target !== "object" || Array.isArray(target)) {
    fail(scriptLabel, `Guardrail fixture target is not an object: ${name}`);
    return;
  }
  if (Object.prototype.hasOwnProperty.call(target, key)) {
    fail(scriptLabel, `Guardrail fixture mutation target already exists: ${name}`);
    return;
  }
  target[key] = value;
  if (!Object.prototype.hasOwnProperty.call(target, key)) {
    fail(scriptLabel, `Guardrail fixture did not mutate object: ${name}`);
  }
}

function mutateSchemaPath(schema) {
  const target = schema?.cases?.[0]?.paths?.find(
    (rule) => Array.isArray(rule.path) && rule.path[0] === "settings" && rule.path[1] === "ollamaModel",
  );
  if (!target) {
    fail(scriptLabel, "Guardrail fixture could not find schema path mutation target");
    return;
  }
  const before = JSON.stringify(target.path);
  target.path = ["settings", "encryptionKey"];
  if (JSON.stringify(target.path) === before) {
    fail(scriptLabel, "Guardrail fixture did not mutate schema path");
  }
}

function normalizedIdentifier(name) {
  return String(name).replace(/[^A-Za-z0-9]/g, "").toLowerCase();
}

function identifierTokens(name) {
  return String(name)
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2")
    .replace(/([a-z0-9])([A-Z])/g, "$1 $2")
    .split(/[^A-Za-z0-9]+/)
    .map((token) => token.toLowerCase())
    .filter(Boolean);
}

function forbiddenNameReason(name) {
  const normalized = normalizedIdentifier(name);
  if (exactForbiddenNames.has(normalized)) {
    return exactForbiddenNames.get(normalized);
  }

  const tokens = identifierTokens(name);
  const token = tokens.find((candidate) => genericForbiddenTokens.has(candidate));
  if (token) {
    return token;
  }

  if (hasTokenPair(tokens, ["api"], ["key", "keys"])) {
    return "apiKey";
  }
  if (hasTokenPair(tokens, ["provider"], ["key", "keys"])) {
    return "providerKey";
  }
  if (hasTokenPair(tokens, ["oauth"], ["token", "tokens"])) {
    return "oauthToken";
  }
  if (hasTokenPair(tokens, ["access"], ["token", "tokens"])) {
    return "accessToken";
  }
  if (hasTokenPair(tokens, ["refresh"], ["token", "tokens"])) {
    return "refreshToken";
  }
  if (hasTokenPair(tokens, ["calendar"], ["token", "tokens"])) {
    return "calendarToken";
  }
  if (hasTokenPair(tokens, ["encryption"], ["key", "keys"])) {
    return "encryptionKey";
  }

  return null;
}

function hasTokenPair(tokens, firstValues, secondValues) {
  const first = new Set(firstValues);
  const second = new Set(secondValues);
  return tokens.some((token, index) => first.has(token) && second.has(tokens[index + 1]));
}

function lineNumberForIndex(source, index) {
  return source.slice(0, index).split(/\r?\n/).length;
}

function collectCandidate(errors, source, candidate, context, index) {
  const reason = forbiddenNameReason(candidate);
  if (!reason) {
    return;
  }

  const line = typeof index === "number" && index >= 0 ? lineNumberForIndex(source, index) : null;
  const location = line ? `line ${line}: ` : "";
  errors.push(
    `${location}Forbidden plain secret field/key name "${candidate}" in ${context}; future secrets must use OS keychain/equivalent secure storage`,
  );
}

function validateSourceSecretFields(source) {
  const errors = [];
  const seen = new Set();

  function collect(candidate, context, index) {
    const key = `${candidate}\0${context}\0${index}`;
    if (seen.has(key)) {
      return;
    }
    seen.add(key);
    collectCandidate(errors, source, candidate, context, index);
  }

  for (const match of source.matchAll(/(?:^|[{\[(,;\r\n])\s*(?:pub\s+)?([A-Za-z_$][A-Za-z0-9_$]*)\??\s*:/g)) {
    collect(match[1], "field/property declaration", match.index + match[0].indexOf(match[1]));
  }

  for (const match of source.matchAll(/["']([A-Za-z_$][A-Za-z0-9_$-]*)["']\s*:/g)) {
    collect(match[1], "quoted object key", match.index + 1);
  }

  for (const match of source.matchAll(/#\s*\[\s*serde\s*\([^\]\r\n]*\brename\s*=\s*"([^"]+)"/g)) {
    collect(match[1], "serde rename field/key alias", match.index + match[0].lastIndexOf(match[1]));
  }

  for (const match of source.matchAll(/\bconst\s+SETTING_[A-Z0-9_]+\s*:\s*&str\s*=\s*"([^"]+)"/g)) {
    collect(match[1], "persisted app setting key", match.index + match[0].indexOf(match[1]));
  }

  for (const match of source.matchAll(/\b(?:upsert_setting|delete_setting|setting_value)\s*\(\s*"([^"]+)"/g)) {
    collect(match[1], "direct persisted app setting key", match.index + match[0].indexOf(match[1]));
  }

  for (const match of source.matchAll(/app_settings[\s\S]{0,240}?params!\s*\[\s*"([^"]+)"/g)) {
    collect(match[1], "direct app_settings params key", match.index + match[0].lastIndexOf(match[1]));
  }

  return errors;
}

function jsonPathLabel(pathParts) {
  if (pathParts.length === 0) {
    return "$";
  }
  return pathParts
    .map((part) => {
      if (typeof part === "number") {
        return `[${part}]`;
      }
      return `.${part}`;
    })
    .join("")
    .replace(/^\./, "$.");
}

function firstLineForJsonName(source, name) {
  if (!source) {
    return null;
  }
  const index = source.indexOf(JSON.stringify(name));
  return index === -1 ? null : lineNumberForIndex(source, index);
}

function validateJsonSecretFields(value, source = "") {
  const errors = [];

  function collect(candidate, context, pathParts) {
    const reason = forbiddenNameReason(candidate);
    if (!reason) {
      return;
    }
    const line = firstLineForJsonName(source, candidate);
    const location = line ? `line ${line}: ` : "";
    errors.push(
      `${location}Forbidden plain secret field/key name "${candidate}" in ${context} ${jsonPathLabel(pathParts)}; future secrets must use OS keychain/equivalent secure storage`,
    );
  }

  function visit(current, pathParts) {
    if (Array.isArray(current)) {
      if (pathParts[pathParts.length - 1] === "path") {
        current.forEach((item, index) => {
          if (typeof item === "string") {
            collect(item, "contract path entry", [...pathParts, index]);
          }
        });
      }
      current.forEach((item, index) => visit(item, [...pathParts, index]));
      return;
    }

    if (!current || typeof current !== "object") {
      return;
    }

    for (const [key, child] of Object.entries(current)) {
      collect(key, "JSON object key", [...pathParts, key]);
      visit(child, [...pathParts, key]);
    }
  }

  visit(value, []);
  return errors;
}

function expectRejected(name, errors, expectedName) {
  if (errors.length === 0) {
    fail(scriptLabel, `Guardrail did not reject: ${name}`);
    return;
  }

  if (expectedName && !errors.some((error) => error.includes(`"${expectedName}"`))) {
    fail(scriptLabel, `Guardrail rejected ${name}, but not for ${expectedName}`);
  }
}

function expectAccepted(name, errors) {
  if (errors.length > 0) {
    fail(scriptLabel, `Guardrail rejected accepted case: ${name}: ${errors.join("; ")}`);
  }
}

function expectArtifactChecked(label) {
  if (!checkedArtifacts.some((artifact) => artifact.label === label)) {
    fail(scriptLabel, `Guardrail artifact list does not include: ${label}`);
  }
}

function runSelfTests() {
  expectArtifactChecked("crates/app/src/lib.rs");

  const storeSource = readText(artifacts.store);
  expectRejected(
    "apiKey persisted settings key",
    validateSourceSecretFields(
      mutateText(
        "apiKey persisted settings key",
        storeSource,
        'const SETTING_OLLAMA_MODEL: &str = "ollama_model";',
        'const SETTING_OLLAMA_MODEL: &str = "apiKey";',
      ),
    ),
    "apiKey",
  );

  for (const forbiddenName of [
    "openai_api_key",
    "provider_api_key",
    "calendar_access_token",
    "oauth_refresh_token",
    "accessTokenValue",
  ]) {
    expectRejected(
      `${forbiddenName} compound secret field`,
      validateSourceSecretFields(`type X = { ${forbiddenName}: string };\n`),
      forbiddenName,
    );
    expectRejected(
      `${forbiddenName} compound secret JSON key`,
      validateJsonSecretFields({ [forbiddenName]: "plain-text-secret" }),
      forbiddenName,
    );
  }

  expectRejected(
    "serde rename apiKey alias",
    validateSourceSecretFields('#[serde(rename = "apiKey")]\nvalue: String,\n'),
    "apiKey",
  );

  expectRejected(
    "direct upsert_setting api_key key",
    validateSourceSecretFields('self.upsert_setting("api_key", value)?;\n'),
    "api_key",
  );

  expectRejected(
    "direct app_settings params api_key key",
    validateSourceSecretFields(
      'conn.execute("INSERT INTO app_settings (key, value) VALUES (?1, ?2)", params!["api_key", value])?;\n',
    ),
    "api_key",
  );

  expectRejected(
    "inline unquoted apiKey field",
    validateSourceSecretFields('const payload = { apiKey: "x" };\ntype X = { provider: string };\n'),
    "apiKey",
  );

  expectRejected(
    "inline unquoted apiKey type field",
    validateSourceSecretFields('const payload = { provider: "local" };\ntype X = { apiKey: string };\n'),
    "apiKey",
  );

  expectAccepted(
    "ordinary password text mention",
    validateSourceSecretFields('const helpText = "password";\nconst label = "Use password manager copy here.";\n'),
  );

  expectAccepted(
    "legitimate non-secret names",
    validateSourceSecretFields(
      [
        'const tokens = query.split(" ");',
        'const label = "tokens";',
        'const analysis = { provider: "ollama", network_used: false, promptTemplateVersion: "summary-v1" };',
        'type Analysis = { provider: string; network_used: boolean; promptTemplateVersion?: string };',
        '#[serde(rename_all = "camelCase")]',
        'struct AnalysisView { provider: String }',
      ].join("\n"),
    ),
  );

  const adapterSource = readText(artifacts.adapter);
  expectRejected(
    "oauth_token desktop DTO field",
    validateSourceSecretFields(
      mutateText(
        "oauth_token desktop DTO field",
        adapterSource,
        "  ollamaModel: string;",
        "  oauth_token: string;",
      ),
    ),
    "oauth_token",
  );

  const fixture = clone(readJson(artifacts.fixture));
  addObjectKey(
    "encryptionKey contract fixture field",
    fixture?.cases?.["desktop_snapshot.empty"]?.settings,
    "encryptionKey",
    "plain-text-key",
  );
  expectRejected(
    "encryptionKey contract fixture field",
    validateJsonSecretFields(fixture),
    "encryptionKey",
  );

  const schema = clone(readJson(artifacts.schema));
  mutateSchemaPath(schema);
  expectRejected(
    "encryptionKey contract schema path",
    validateJsonSecretFields(schema),
    "encryptionKey",
  );
}

function validateArtifact(artifact) {
  const source = readText(artifact);
  if (!source) {
    return;
  }

  const isJson = artifact.label.endsWith(".json");
  if (isJson) {
    let value;
    try {
      value = JSON.parse(source);
    } catch (error) {
      fail(artifact.label, `Unable to read or parse JSON: ${error.message}`);
      return;
    }
    for (const error of validateJsonSecretFields(value, source)) {
      fail(artifact.label, error);
    }
    return;
  }

  for (const error of validateSourceSecretFields(source)) {
    fail(artifact.label, error);
  }
}

runSelfTests();

for (const artifact of checkedArtifacts) {
  validateArtifact(artifact);
}

if (!ok) {
  process.exit(1);
}

console.log("Plain secret storage gate passed.");
