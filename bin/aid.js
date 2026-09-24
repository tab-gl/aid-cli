#!/usr/bin/env node

import { login } from "../src/commands/login.js";
import { logout } from "../src/commands/logout.js";
import { whoami } from "../src/commands/whoami.js";
import { apps } from "../src/commands/apps/index.js";
import { clients } from "../src/commands/clients/index.js";
import { setup } from "../src/commands/setup.js";
import { failure } from "../src/ui/format.js";
import { dim } from "../src/ui/colors.js";
import { commandList } from "../src/ui/format.js";
import { showBanner } from "../src/ui/banner.js";
import { withScreen } from "../src/ui/screen.js";

const [, , command, ...args] = process.argv;

const commands = { setup, login, logout, whoami, apps, clients };

if (!command || command === "help" || command === "--help") {
  await withScreen(async () => {
    showBanner();
    printHelp();
  });

  process.exit(0);
}

if (command === "-v" || command === "--version") {
  await withScreen(async () => {
    showBanner();
  });

  process.exit(0);
}

if (!commands[command]) {
  failure(`Unknown command: ${command}`);
  printHelp();
  process.exit(1);
}

try {
  await commands[command](args);
} catch (err) {
  failure(err.message);
  process.exit(1);
}

function printHelp() {
  console.log(`  ${dim("USAGE")}\n\n    aid <command> [options]\n`);
  commandList([
    ["setup", "Set up an Ace ID project"],
    ["login", "Sign in via Ace ID"],
    ["logout", "Sign out"],
    ["whoami", "Show signed-in account"],
    ["apps <subcommand>", "Manage applications"],
    ["clients <subcommand>", "Manage OAuth clients"],
    ["version", "Show version"],
    ["help", "Show this message"]
  ]);

  console.log(`  ${dim("SUPPORT")}`);
  commandList([
    ["Docs", "https://docs.ace-base.cc"],
    ["Support", "https://bsky.app/profile/ace-base.cc"]
  ]);
}
