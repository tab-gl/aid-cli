#!/usr/bin/env node

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

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

try {
  for (const chain of splitCommandChain(argv)) await runCommand(chain);
} catch (err) {
  failure(err instanceof Error ? err.message : String(err));
  process.exitCode = 1;
}

async function runCommand(argv) {
  const [rawCommand, ...rawArgs] = argv;
  const { command, args } = normalizeCommand(rawCommand, rawArgs);

  if (!command || ["help", "h", "--help", "-h"].includes(command)) {
    await withScreen(async () => { showBanner(); printHelp(); });
    return;
  }

  if (["version", "-v", "--version"].includes(command)) {
    const packagePath = fileURLToPath(new URL("../package.json", import.meta.url));
    const pkg = JSON.parse(await readFile(packagePath, "utf8"));
    console.log(`ace-id ${pkg.version}`);
    return;
  }

  if (!Object.hasOwn(commands, command)) throw new Error(`Unknown command: ${command}`);
  await commands[command](args);
}

function splitCommandChain(argv) {
  const chains = [];
  let current = [];

  for (const token of argv) {
    if (token === ";" || token === ";;" || token === "\\;") {
      if (current.length) chains.push(stripCliPrefix(current));
      current = [];
      continue;
    }
    current.push(token);
  }

  if (current.length) chains.push(stripCliPrefix(current));
  return chains.length ? chains : [[]];
}

function stripCliPrefix(argv) {
  return ["aid", "ace-id", "aidc"].includes(argv[0]) ? argv.slice(1) : argv;
}

function normalizeCommand(rawCommand, rawArgs) {
  if (rawCommand === "init") return { command: "setup", args: rawArgs };
  if (rawCommand === "app") return { command: "apps", args: rawArgs };

  if (["list", "create", "get", "update", "delete"].includes(rawCommand)) {
    const args = rawArgs[0] === "app" ? rawArgs.slice(1) : rawArgs;
    return { command: "apps", args: [rawCommand, ...args] };
  }
  return { command: rawCommand, args: rawArgs };
}

function printHelp() {
  console.log("  " + dim("USAGE"));
  console.log("");
  console.log("    aid <command> [options]");
  console.log("    aid <command> [options] \\; aid <command> [options]");
  console.log("");
  console.log("  " + dim("COMMANDS"));
  commandList([
    ["setup", "Set up an Ace ID project"], ["init", "Alias for setup"],
    ["login", "Sign in via Ace ID"], ["logout", "Sign out"],
    ["whoami", "Show signed-in account"], ["apps", "Manage applications"],
    ["link", "Link an application to this folder and write .aid.json"],
    ["sync", "Refresh the linked application in this folder"],
    ["clients", "Manage OAuth clients"], ["version", "Show CLI version"], ["help", "Show this message"]
  ]);
  console.log("");
  console.log("  " + dim("SHORT FLAGS"));
  commandList([
    ["-h", "Alias for --help"], ["-i", "Alias for --interactive / --id by command"], ["-n", "Alias for --name"],
    ["-r", "Alias for --redirect"], ["-s", "Alias for --scope"], ["-d", "Alias for --description"],
    ["-c", "Alias for --client"], ["-p", "Alias for --public"], ["-f", "Alias for --force"], ["-v", "Alias for --version"]
  ]);
  console.log("");
  console.log("  " + dim("APPS SUBCOMMANDS"));
  commandList([
    ["apps list", "List applications"], ["apps create --name <name>", "Create an application"],
    ["apps get <id>", "Show an application"], ["apps update <id> --name <name>", "Update application name"],
    ["apps delete <id>...", "Delete one or more applications"], ["app <subcommand>", "Compatibility alias for apps"],
    ["create app ...", "Compatibility shorthand for apps create ..."]
  ]);
  console.log("");
  console.log("  " + dim("PROJECT LINKING"));
  commandList([
    ["link <app_id>", "Link an existing app to the current folder"], ["link <app_id> --client <client_id>", "Link an app and record an OAuth client"],
    ["sync", "Refresh the app recorded in .aid.json"], ["sync <app_id>", "Replace the current link with an app"]
  ]);
  console.log("");
  console.log("  " + dim("CLIENTS SUBCOMMANDS"));
  commandList([
    ["clients list", "List clients"], ["clients create --name <n> --redirect <uri>", "Create a client"],
    ["clients get <client_id>", "Show one client"], ["clients delete <client_id>", "Delete a client"],
    ["clients rotate-secret <client_id>", "Rotate a client secret"]
  ]);
  console.log("");
  console.log("  " + dim("COMMAND CHAINING"));
  commandList([
    ["aid setup -i \\; aid create app -n my-app", "Run full commands sequentially"],
    ["aid login \\; aid apps list", "Stop immediately if a command fails"]
  ]);
  console.log("");
  console.log("  " + dim("ALIASES"));
  commandList([["aid", "Canonical command"], ["ace-id", "Compatibility alias"], ["aidc", "Compatibility alias"], ["app", "Compatibility alias for apps"]]);
  console.log("");
  console.log("  " + dim("SUPPORT"));
  commandList([["Docs", "https://docs.ace-base.cc"], ["Support", "https://bsky.app/profile/ace-base.cc"]]);
}
