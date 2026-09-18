import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { dim, bold } from "./colors.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function readVersion() {
  try {
    const pkgPath = path.join(__dirname, "..", "..", "package.json");
    const raw = readFileSync(pkgPath, "utf8");
    return JSON.parse(raw).version || "0.0.0";
  } catch {
    return "0.0.0";
  }
}

/*
 * Half-block wordmark.
 *
 * Renders identically on Termux, Alpine, iTerm,
 * and Windows Terminal. No color-by-default so
 * it doesn't fight the terminal theme.
 */
const WORDMARK = [
  "   ▄▀█ █ █▀▄ █▀▀",
  "   █▀█ █ █▄▀ █▄▄"
];

export function showBanner(stream = process.stdout) {
  const version = readVersion();

  stream.write("\n");

  for (const line of WORDMARK) {
    stream.write(`${bold(line)}\n`);
  }

  stream.write(
    `   ${dim("CLI")}  ${dim("v" + version)}\n`
  );

  stream.write("\n");
}
