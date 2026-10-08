#!/usr/bin/env node

import { login } from "../src/commands/login.js";
import { logout } from "../src/commands/logout.js";
import { whoami } from "../src/commands/whoami.js";
import { apps } from "../src/commands/apps/index.js";
import { clients } from "../src/commands/clients/index.js";
import { setup } from "../src/commands/setup.js";
import { link } from "../src/commands/link.js";
import { sync } from "../src/commands/sync.js";
import { failure, commandList } from "../src/ui/format.js";
import { dim } from "../src/ui/colors.js";
import { showBanner } from "../src/ui/banner.js";
import { withScreen } from "../src/ui/screen.js";

const [, , ...argv] = process.argv;
const commands = { setup, login, logout, whoami, apps, clients, link, sync };
const chains = splitCommandChain(argv);

if (chains.length > 1) {
  try {
    for (const chain of chains) await runCommand(chain);
    process.exit(0);
  } catch (err) {
    failure(err.message);
    process.exit(1);
  }
}

const [rawCommand, ...args] = chains[0] ?? [];
await runCommand([rawCommand, ...args]);

if (!command || command === "help" || command === "--help" || command === "-h") {
  await withScreen(async () => { showBanner(); printHelp(); });
  process.exit(0);
}

if (command === "version" || command === "-v" || command === "--version") {
  await withScreen(async () => { showBanner(); });
  process.exit(0);
}

if (!commands[command]) {
  failure(`Unknown command: ${command}`);
  printHelp();
  process.exit(1);
}

async function runCommand(argv) {
  const [rawCommand, ...args] = argv;
  const normalized = normalizeCommand(rawCommand, args);
  const command = normalized.command;
  const commandArgs = normalized.args;

  if (!command || command === "help" || command === "--help" || command === "-h") {
    await withScreen(async () => { showBanner(); printHelp(); });
    return;
  }

  if (command === "version" || command === "-v" || command === "--version") {
    await withScreen(async () => { showBanner(); });
    return;
  }

  if (!commands[command]) throw new Error(`Unknown command: ${command}`);
  await commands[command](commandArgs);
}

function splitCommandChain(argv) {
  const chains = [];
  let current = [];

  for (const token of argv) {
    if (token === ";") {
      if (current.length) chains.push(current);
      current = [];
    } else {
      current.push(token);
    }
  }

  if (current.length) chains.push(current);
  return chains.length ? chains : [[]];
}

function printHelp() {
  console.log(`  ${dim("USAGE")}\n\n    aid <command> [options]\n`);
  console.log(`  ${dim("COMMANDS")}`);
  commandList([
    ["setup", "Set up an Ace ID project"], ["init", "Alias for setup"],
    ["login", "Sign in via Ace ID"], ["logout", "Sign out"],
    ["whoami", "Show signed-in account"], ["apps", "Manage applications"],
    ["link", "Link an application to this folder and write .aid.json"],
    ["sync", "Refresh the linked application in this folder"],
    ["clients", "Manage OAuth clients"], ["version", "Show CLI version"], ["help", "Show this message"]
  ]);
  console.log(`\n  ${dim("SETUP OPTIONS")}`);
  commandList([
    ["--interactive, -i", "Run the interactive setup wizard"], ["--wizard", "Alias for --interactive"],
    ["--name <name>", "Application/project name"], ["--redirect <uri>", "OAuth redirect URI"],
    ["--scope <scope>...", "OAuth scope; repeat for multiple scopes"], ["--force, -f", "Replace an existing .aid.json"]
  ]);
  console.log(`\n  ${dim("APPS SUBCOMMANDS")}`);
  commandList([
    ["apps list", "List applications"], ["apps create --name <name>", "Create an application"],
    ["apps get <id>", "Show an application"], ["apps update <id> --name <name>", "Update application name"],
    ["apps delete <id>", "Delete an application"], ["app <subcommand>", "Compatibility alias for apps"]
  ]);
  console.log(`\n  ${dim("PROJECT LINKING")}`);
  commandList([
    ["link <app_id>", "Link an existing app to the current folder"],
    ["link <app_id> --client <client_id>", "Link an app and record an OAuth client"],
    ["sync", "Refresh the app recorded in .aid.json"], ["sync <app_id>", "Replace the current link with an app"]
  ]);
  console.log(`\n  ${dim("SMART APP SHORTHANDS")}`);
  commandList([
    ["list", "→ apps list"], ["create ...", "→ apps create ..."], ["get <id>", "→ apps get <id>"],
    ["update <id> ...", "→ apps update <id> ..."], ["delete <id>", "→ apps delete <id>"]
  ]);
  console.log(`\n  ${dim("CLIENTS SUBCOMMANDS")}`);
  commandList([
    ["clients list", "List OAuth clients"], ["clients create --name <name> --redirect <uri>", "Create a client"],
    ["clients get <client_id>", "Show a client"], ["clients delete <client_id>", "Delete a client"],
    ["clients rotate-secret <client_id>", "Rotate a client secret"]
  ]);
  console.log(`\n  ${dim("HELP / VERSION FLAGS")}`);
  commandList([["--help, -h", "Show help"], ["--version, -v", "Show CLI version"], ["help", "Show help"], ["version", "Show CLI version"]]);
  console.log(`\n  ${dim("ALIASES")}`);
  commandList([["aid", "Canonical command"], ["ace-id", "Compatibility alias"], ["aidc", "Compatibility alias"], ["app", "Compatibility alias for apps"]]);
  console.log(`\n  ${dim("COMMAND CHAINING")}`);
  commandList([
    ["aid apps list ; apps get <id>", "Run commands sequentially"],
    ["aid login ; apps list", "Stop immediately if a command fails"]
  ]);
  console.log(`\n  ${dim("SUPPORT")}`);
  commandList([["Docs", "https://docs.ace-base.cc"], ["Support", "https://bsky.app/profile/ace-base.cc"]]);
}

function normalizeCommand(rawCommand, rawArgs) {
  if (rawCommand === "init") return { command: "setup", args: rawArgs };
  if (rawCommand === "app") return { command: "apps", args: rawArgs };
  const shorthand = new Set(["list", "create", "get", "update", "delete"]);
  if (shorthand.has(rawCommand)) return { command: "apps", args: [rawCommand, ...rawArgs] };
  return { command: rawCommand, args: rawArgs };
}
