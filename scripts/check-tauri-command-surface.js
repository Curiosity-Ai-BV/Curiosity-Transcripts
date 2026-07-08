const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const mainPath = path.join(root, "apps", "desktop", "src-tauri", "src", "main.rs");
const adapterPath = path.join(root, "apps", "desktop", "src", "commandAdapter.ts");
const mainLabel = "apps/desktop/src-tauri/src/main.rs";
const adapterLabel = "apps/desktop/src/commandAdapter.ts";
const scriptLabel = "scripts/check-tauri-command-surface.js";

let ok = true;

function fail(file, message) {
  console.error(`::error file=${file}::${message}`);
  ok = false;
}

function extractCommands(source, cfgPattern, label) {
  const match = source.match(cfgPattern);
  if (!match) {
    return {
      commands: [],
      errors: [`Missing ${label} Tauri invoke handler`],
    };
  }

  return {
    commands: match[1]
      .split(",")
      .map((command) => command.trim())
      .filter(Boolean),
    errors: [],
  };
}

function validateCommandSurface(source) {
  const errors = [];
  const debugHandler = extractCommands(
    source,
    /#\[cfg\(any\(test, debug_assertions\)\)\]\s*let builder = builder\.invoke_handler\(tauri::generate_handler!\[\s*([\s\S]*?)\s*\]\);/,
    "debug/test",
  );
  const releaseHandler = extractCommands(
    source,
    /#\[cfg\(not\(any\(test, debug_assertions\)\)\)\]\s*let builder = builder\.invoke_handler\(tauri::generate_handler!\[\s*([\s\S]*?)\s*\]\);/,
    "release",
  );

  errors.push(...debugHandler.errors, ...releaseHandler.errors);

  if (!debugHandler.commands.includes("seed_dev_fixture")) {
    errors.push("Debug/test invoke handler must keep seed_dev_fixture available for deterministic harnesses");
  }

  if (releaseHandler.commands.includes("seed_dev_fixture")) {
    errors.push("Release invoke handler must not register debug/test-only seed_dev_fixture");
  }

  if (
    !/#\[cfg\(any\(test, debug_assertions\)\)\]\s*#\[tauri::command\]\s*fn seed_dev_fixture\(/.test(
      source,
    )
  ) {
    errors.push("seed_dev_fixture command must stay guarded by #[cfg(any(test, debug_assertions))]");
  }

  return errors;
}

function validateFrontendCommandSurface(source) {
  const errors = [];

  if (source.includes("seed_dev_fixture")) {
    errors.push("Production command adapter must not expose debug/test-only seed_dev_fixture");
  }

  return errors;
}

if (!fs.existsSync(mainPath)) {
  fail(mainLabel, "Missing Tauri main source");
} else {
  const source = fs.readFileSync(mainPath, "utf8");
  const releaseFixture = source.replace(
    "        cancel_summary\n    ]);",
    "        cancel_summary,\n        seed_dev_fixture\n    ]);",
  );
  const missingDebugFixture = source.replace("        seed_dev_fixture\n    ]);", "    ]);");
  const unguardedCommandFixture = source.replace(
    "#[cfg(any(test, debug_assertions))]\n#[tauri::command]\nfn seed_dev_fixture(",
    "#[tauri::command]\nfn seed_dev_fixture(",
  );

  const rejectionCases = [
    {
      name: "release handler registers debug/test fixture command",
      source: releaseFixture,
    },
    {
      name: "debug/test handler drops deterministic fixture command",
      source: missingDebugFixture,
    },
    {
      name: "fixture command loses debug/test cfg guard",
      source: unguardedCommandFixture,
    },
  ];

  for (const testCase of rejectionCases) {
    if (testCase.source === source) {
      fail(scriptLabel, `Guardrail fixture did not mutate source: ${testCase.name}`);
      continue;
    }

    if (validateCommandSurface(testCase.source).length === 0) {
      fail(scriptLabel, `Guardrail did not reject: ${testCase.name}`);
    }
  }

  for (const error of validateCommandSurface(source)) {
    fail(mainLabel, error);
  }
}

if (!fs.existsSync(adapterPath)) {
  fail(adapterLabel, "Missing desktop command adapter");
} else {
  const source = fs.readFileSync(adapterPath, "utf8");
  const exposedFixture = `${source}\nconst leakedDebugCommand = "seed_dev_fixture";\n`;

  if (validateFrontendCommandSurface(exposedFixture).length === 0) {
    fail(scriptLabel, "Guardrail did not reject: frontend adapter exposes debug fixture command");
  }

  for (const error of validateFrontendCommandSurface(source)) {
    fail(adapterLabel, error);
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Tauri command surface gate passed.");
