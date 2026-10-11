import { green, dim, yellow } from "./colors.js";

/**
 * A line-oriented progress display that behaves well in mobile terminals.
 * Each update ends with a newline, so prompts and terminal scrollback never
 * fight over a carriage-return animation.
 */
export function createStepProgress(steps, { stream = process.stdout, enabled = stream.isTTY } = {}) {
  let completed = 0;
  let active = -1;

  function render(index, status, message) {
    if (!enabled) return;
    const total = steps.length;
    const count = Math.max(0, Math.min(total, status === "complete" ? index + 1 : completed));
    const width = 16;
    const filled = Math.round((count / total) * width);
    const bar = `${"█".repeat(filled)}${"░".repeat(width - filled)}`;
    const label = message || steps[index] || "Setup";
    const number = `${Math.min(index + 1, total)}/${total}`;
    const marker = status === "complete" ? green("✓") : status === "failed" ? yellow("!") : dim("›");
    stream.write(`  ${marker} [${bar}] ${number}  ${label}\n`);
  }

  return {
    start(index, message) {
      active = index;
      render(index, "active", message);
    },
    complete(index, message) {
      completed = Math.max(completed, index + 1);
      active = -1;
      render(index, "complete", message);
    },
    fail(message) {
      if (active >= 0) render(active, "failed", message || `${steps[active]} failed`);
      active = -1;
    }
  };
}
