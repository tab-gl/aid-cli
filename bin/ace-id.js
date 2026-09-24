#!/usr/bin/env node

import { setup } from "../src/commands/setup.js";

const [, , command, ...args] = process.argv;

if (!command || command === "init" || command === "setup") {
  try {
    await setup(args);
  } catch (err) {
    console.error("\n✗ " + (err instanceof Error ? err.message : String(err)) + "\n");
    process.exitCode = 1;
  }
  process.exit();
}

if (command === "--help" || command === "help") {
  console.log("\nUsage:\n  ace-id init      Initialize the current project\n  ace-id setup     Alias for init\n\nThe canonical developer CLI is:\n  aid setup\n  aidc setup      Legacy alias\n");
  process.exit(0);
}

console.error(`Unknown command: ${command}`);
console.error("Run `ace-id init` to initialize the current project.");
process.exitCode = 1;
