#!/usr/bin/env node

import { login } from "../src/commands/login.js";
import { logout } from "../src/commands/logout.js";
import { whoami } from "../src/commands/whoami.js";
import { apps } from "../src/commands/apps/index.js";
import { clients } from "../src/commands/clients/index.js";
import { setup } from "../src/commands/setup.js";
import { link } from "../src/commands/link.js";
import { sync } from "../src/commands/sync.js";
import { failure } from "../src/ui/format.js";
import { dim } from "../src/ui/colors.js";
import { commandList } from "../src/ui/format.js";
import { showBanner } from "../src/ui/banner.js";
import { withScreen } from "../src/ui/screen.js";

const [, , rawCommand, ...args] = process.argv;
const normalized = normalizeCommand(rawCommand, args);
const command = normalized.command;
const commandArgs = normalized.args;

const commands = { setup, login, logout, whoami, apps, clients, link, sync };

if (!command || command === "help" || command === "--help" || command === "-h") {
  await withScreen(async () => {
    showBanner();
    printHelp();
  });

  process.exit(0);
}

if (command === "version" || command === "-v" || command === "--version") {
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
  await commands[command](commandArgs);
} catch (err) {
  failure(err.message);
  process.exit(1);
}

function printHelp() {
  console.log(`  ${dim("USAGE")}\n\n    aid <command> [options]\n`);

  console.log(`  ${dim("COMMANDS")}`);
  commandList([
    ["setup", "Set up an Ace ID project"],
    ["init", "Alias for setup"],
    ["login", "Sign in via Ace ID"],
    ["logout", "Sign out"],
    ["whoami", "Show signed-in account"],
    ["apps", "Manage applications"],
    ["link", "Link an application to this folder and write .aid.json"],
    ["sync", "Refresh the linked application in this folder"],
    ["clients", "Manage OAuth clients"],
    ["version", "Show CLI version"],
    ["help", "Show this message"]
  ]);

  console.log(`\n  ${dim("SETUP OPTIONS")}`);
  commandList([
    ["--interactive, -i", "Run the interactive setup wizard"],
    ["--wizard", "Alias for --interactive"],
    ["--name <name>", "Application/project name"],
    ["--redirect <uri>", "OAuth redirect URI"],
    ["--scope <scope>...", "OAuth scope; repeat for multiple scopes"],
    ["--force, -f", "Replace an existing .aid.json"]
  ]);

  console.log(`\n  ${dim("APPS SUBCOMMANDS")}`);
  commandList([
    ["apps list", "List applications"],
    ["apps create --name <name>", "Create an application"],
    ["apps create --name <name> --description <text>", "Create with description"],
    ["apps get <id>", "Show an application"],
    ["apps update <id> --name <name>", "Update application name"],
    ["apps update <id> --description <text>", "Update application description"],
    ["apps delete <id>", "Delete an application"],
    ["app <subcommand>", "Compatibility alias for apps"]
  ]);

  console.log(`\n  ${dim("PROJECT LINKING")}`);\n  commandList([\n    ["link <app_id>", "Link an existing app to the current folder"],\n    ["link <app_id> --client <client_id>", "Link an app and record an OAuth client"],\n    ["sync", "Refresh the app recorded in .aid.json"],\n    ["sync <app_id>", "Replace the current link with an app"]\n  ]);\n\n  console.log(`\n  ${dim("CLIENTS SUBCOMMANDS")}`);
  commandList([
    ["clients list", "List OAuth clients"],
    ["clients create --name <name> --redirect <uri>", "Create a client"],
    ["--redirect <uri>...", "Repeat to add multiple redirect URIs"],
    ["--scope <scope>...", "Repeat to add OAuth scopes"],
    ["--id <id>", "Set a specific client ID"],
    ["--public", "Create a public client"],
    ["clients get <client_id>", "Show a client"],
    ["clients delete <client_id>", "Delete a client"],
    ["clients rotate-secret <client_id>", "Rotate a client secret"]
  ]);

  console.log(`\n  ${dim("HELP / VERSION FLAGS")}`);
  commandList([
    ["--help, -h", "Show help"],
    ["--version, -v", "Show CLI version"],
    ["help", "Show help"],
    ["version", "Show CLI version"]
  ]);

  console.log(`\n  ${dim("ALIASES")}`);
  commandList([
    ["aid", "Canonical command"],
    ["ace-id", "Compatibility alias"],
    ["aidc", "Compatibility alias"]
  ]);

  console.log(`\n  ${dim("SUPPORT")}`);
  commandList([
    ["Docs", "https://docs.ace-base.cc"],
    ["Support", "https://bsky.app/profile/ace-base.cc"]
  ]);
}

function normalizeCommand(rawCommand, rawArgs) {
  if (rawCommand === "init") return { command: "setup", args: rawArgs };
  if (rawCommand === "app") return { command: "apps", args: rawArgs };

  const shorthand = new Set(["list", "create", "get", "update", "delete"]);
  if (shorthand.has(rawCommand)) {
    return { command: "apps", args: [rawCommand, ...rawArgs] };
  }

  return { command: rawCommand, args: rawArgs };
}
