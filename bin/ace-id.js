#!/usr/bin/env node

import { setup } from "../src/commands/setup.js";
import { failure, usage } from "../src/ui/format.js";
import { showBanner } from "../src/ui/banner.js";
import { withScreen } from "../src/ui/screen.js";

const [, , command, ...args] = process.argv;

if (!command || command === "init" || command === "setup") {
  try {
    await setup(args);
  } catch (err) {
    failure(err instanceof Error ? err.message : String(err));
    process.exitCode = 1;
  }
  process.exit();
}

if (command === "--help" || command === "help") {
  await withScreen(async () => {
    showBanner();
    usage(`
Usage

  ace-id init      Initialize the current project
  ace-id setup     Alias for init

Canonical developer CLI

  aid setup
  aidc setup       Legacy alias
`);
  });
  process.exit(0);
}

console.error(`Unknown command: ${command}`);
console.error("Run `ace-id init` to initialize the current project.");
process.exitCode = 1;
