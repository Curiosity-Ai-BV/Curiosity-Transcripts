const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const mainPath = path.join(root, "apps", "desktop", "src-tauri", "src", "main.rs");
const adapterPath = path.join(root, "apps", "desktop", "src", "commandAdapter.ts");
const mainLabel = "apps/desktop/src-tauri/src/main.rs";
const adapterLabel = "apps/desktop/src/commandAdapter.ts";
const scriptLabel = "scripts/check-tauri-command-surface.js";
const RELEASE_ONLY_COMMAND_ALLOWLIST = [];

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

function extractDesktopSnapshotCommands(source) {
  const match = source.match(/const DESKTOP_SNAPSHOT_COMMANDS = new Set\(\[\s*([\s\S]*?)\s*\]\);/);
  if (!match) {
    return {
      commands: [],
      errors: ["Missing DESKTOP_SNAPSHOT_COMMANDS runtime validation allowlist"],
    };
  }

  return {
    commands: [...match[1].matchAll(/"([^"]+)"/g)].map((entry) => entry[1]),
    errors: [],
  };
}

function extractFrontendCommandLiterals(source) {
  const facadeMatch = source.match(
    /export function createDesktopCommandFacade\(fetchCommand: CommandFetcher\): DesktopCommandFacade \{\s*([\s\S]*?)\n\}/,
  );
  if (!facadeMatch) {
    return {
      snapshotCommands: [],
      fetchCommands: [],
      errors: ["Missing createDesktopCommandFacade command mapping"],
    };
  }

  const facadeSource = facadeMatch[1];
  const snapshotCommands = [...facadeSource.matchAll(/snapshotCommand\("([^"]+)"/g)].map((entry) => entry[1]);
  const fetchCommands = [...facadeSource.matchAll(/fetchCommand(?:<[^>]+>)?\("([^"]+)"/g)].map((entry) => entry[1]);

  return {
    snapshotCommands,
    fetchCommands,
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

function validateReleaseCommandOwnership(
  releaseCommands,
  facadeCommands,
  releaseOnlyAllowlist = RELEASE_ONLY_COMMAND_ALLOWLIST,
) {
  const errors = [];
  const releaseCommandSet = new Set(releaseCommands);
  const facadeCommandSet = new Set(facadeCommands);
  const allowedReleaseOnlyCommands = new Set();

  for (const entry of releaseOnlyAllowlist) {
    const command = entry.command;
    const reason = typeof entry.reason === "string" ? entry.reason.trim() : "";

    if (!reason) {
      errors.push(`RELEASE_ONLY_COMMAND_ALLOWLIST entry ${command} must include a non-empty reason`);
    } else {
      allowedReleaseOnlyCommands.add(command);
    }

    if (!releaseCommandSet.has(command)) {
      errors.push(`RELEASE_ONLY_COMMAND_ALLOWLIST entry ${command} is not registered by the release Tauri handler`);
    }
  }

  for (const command of [...releaseCommandSet].sort()) {
    if (facadeCommandSet.has(command) || allowedReleaseOnlyCommands.has(command)) {
      continue;
    }
    errors.push(
      `Release Tauri handler registers ${command}, but createDesktopCommandFacade does not own it and RELEASE_ONLY_COMMAND_ALLOWLIST has no reason`,
    );
  }

  return errors;
}

function validateFrontendCommandSurface(
  source,
  releaseCommands = [],
  releaseOnlyAllowlist = RELEASE_ONLY_COMMAND_ALLOWLIST,
) {
  const errors = [];
  const snapshotAllowlist = extractDesktopSnapshotCommands(source);
  const frontendCommands = extractFrontendCommandLiterals(source);
  const releaseCommandSet = new Set(releaseCommands);
  const snapshotAllowlistSet = new Set(snapshotAllowlist.commands);

  if (source.includes("seed_dev_fixture")) {
    errors.push("Production command adapter must not expose debug/test-only seed_dev_fixture");
  }

  errors.push(...snapshotAllowlist.errors, ...frontendCommands.errors);

  const allFacadeCommands = new Set([
    ...frontendCommands.snapshotCommands,
    ...frontendCommands.fetchCommands,
  ]);

  for (const command of [...allFacadeCommands].sort()) {
    if (!releaseCommandSet.has(command)) {
      errors.push(`Production command adapter invokes ${command}, but the release Tauri handler does not register it`);
    }
  }

  if (frontendCommands.errors.length === 0) {
    errors.push(
      ...validateReleaseCommandOwnership(
        releaseCommands,
        allFacadeCommands,
        releaseOnlyAllowlist,
      ),
    );
  }

  for (const command of [...new Set(frontendCommands.snapshotCommands)].sort()) {
    if (!snapshotAllowlistSet.has(command)) {
      errors.push(`Snapshot-returning facade command ${command} must be listed in DESKTOP_SNAPSHOT_COMMANDS`);
    }
  }

  return errors;
}

function releaseCommands(source) {
  return extractCommands(
    source,
    /#\[cfg\(not\(any\(test, debug_assertions\)\)\)\]\s*let builder = builder\.invoke_handler\(tauri::generate_handler!\[\s*([\s\S]*?)\s*\]\);/,
    "release",
  );
}

let registeredReleaseCommands = [];

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

  const releaseHandler = releaseCommands(source);
  for (const error of releaseHandler.errors) {
    fail(mainLabel, error);
  }
  registeredReleaseCommands = releaseHandler.commands;
}

if (!fs.existsSync(adapterPath)) {
  fail(adapterLabel, "Missing desktop command adapter");
} else {
  const source = fs.readFileSync(adapterPath, "utf8");
  const exposedFixture = `${source}\nconst leakedDebugCommand = "seed_dev_fixture";\n`;
  const missingReleaseRegistrationFixture = source.replace(
    'fetchCommand<unknown>("search_meetings", { query })',
    'fetchCommand("search_archives", { query })',
  );
  const missingSnapshotValidationFixture = source.replace('"save_analysis_settings",\n', "");

  if (validateFrontendCommandSurface(exposedFixture, registeredReleaseCommands).length === 0) {
    fail(scriptLabel, "Guardrail did not reject: frontend adapter exposes debug fixture command");
  }

  if (missingReleaseRegistrationFixture === source) {
    fail(scriptLabel, "Guardrail fixture did not mutate source: frontend adapter invokes unregistered release command");
  } else {
    const errors = validateFrontendCommandSurface(missingReleaseRegistrationFixture, registeredReleaseCommands);
    if (
      !errors.includes(
        "Production command adapter invokes search_archives, but the release Tauri handler does not register it",
      )
    ) {
      fail(scriptLabel, "Guardrail did not reject: frontend adapter invokes unregistered release command");
    }
  }

  if (missingSnapshotValidationFixture === source) {
    fail(scriptLabel, "Guardrail fixture did not mutate source: snapshot command removed from validation allowlist");
  } else {
    const errors = validateFrontendCommandSurface(missingSnapshotValidationFixture, registeredReleaseCommands);
    if (
      !errors.includes(
        "Snapshot-returning facade command save_analysis_settings must be listed in DESKTOP_SNAPSHOT_COMMANDS",
      )
    ) {
      fail(scriptLabel, "Guardrail did not reject: snapshot command removed from validation allowlist");
    }
  }

  const extraReleaseCommand = "release_diagnostic_probe";
  const extraReleaseCommandErrors = validateFrontendCommandSurface(source, [
    ...registeredReleaseCommands,
    extraReleaseCommand,
  ]);
  if (
    !extraReleaseCommandErrors.includes(
      `Release Tauri handler registers ${extraReleaseCommand}, but createDesktopCommandFacade does not own it and RELEASE_ONLY_COMMAND_ALLOWLIST has no reason`,
    )
  ) {
    fail(scriptLabel, "Guardrail did not reject: release handler registers unowned frontend command");
  }

  const staleReleaseOnlyAllowlistErrors = validateFrontendCommandSurface(source, registeredReleaseCommands, [
    {
      command: "missing_release_command",
      reason: "Example release-only command used to prove stale allowlist entries fail.",
    },
  ]);
  if (
    !staleReleaseOnlyAllowlistErrors.includes(
      "RELEASE_ONLY_COMMAND_ALLOWLIST entry missing_release_command is not registered by the release Tauri handler",
    )
  ) {
    fail(scriptLabel, "Guardrail did not reject: release-only allowlist entry missing from release handler");
  }

  const emptyReasonReleaseOnlyAllowlistErrors = validateFrontendCommandSurface(
    source,
    [...registeredReleaseCommands, extraReleaseCommand],
    [
      {
        command: extraReleaseCommand,
        reason: "",
      },
    ],
  );
  if (
    !emptyReasonReleaseOnlyAllowlistErrors.includes(
      `RELEASE_ONLY_COMMAND_ALLOWLIST entry ${extraReleaseCommand} must include a non-empty reason`,
    )
  ) {
    fail(scriptLabel, "Guardrail did not reject: release-only allowlist entry with empty reason");
  }

  for (const error of validateFrontendCommandSurface(source, registeredReleaseCommands)) {
    fail(adapterLabel, error);
  }
}

if (!ok) {
  process.exit(1);
}

console.log("Tauri command surface gate passed.");
