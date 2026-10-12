import { green, dim, yellow } from "./colors.js";

/**
 * Step-based progress that stays readable in desktop, mobile, and scrollback
 * terminals. Each update ends with a newline instead of rewriting the same
 * line, which keeps interactive prompts usable in Termux and other terminals.
 */
export function createStepProgress(steps, { stream = process.stdout, enabled = stream.isTTY } = {}) {
  if (!Array.isArray(steps) || steps.length === 0) {
    throw new TypeError("Progress requires at least one step.");
  }

  let completed = 0;
  let active = -1;

  function render(index, status, message) {
    if (!enabled) return;
    const total = steps.length;
    const progress = status === "active" || status === "failed"
      ? Math.min(total, completed + 0.5)
      : Math.min(total, completed);
    const percent = Math.round((progress / total) * 100);
    const width = 18;
    const filled = Math.max(status === "active" ? 1 : 0, Math.round((progress / total) * width));
    const bar = `${"█".repeat(filled)}${"░".repeat(width - filled)}`;
    const label = message || steps[index] || "Setup";
    const number = `${Math.min(index + 1, total)}/${total}`;
    const marker = status === "complete" ? green("✓") : status === "failed" ? yellow("!") : status === "skipped" ? yellow("–") : dim("›");
    stream.write(`  ${marker} [${bar}] ${String(percent).padStart(3)}%  ${number}  ${label}\n`);
  }

  return {
    start(index, message) {
      if (!Number.isInteger(index) || index < 0 || index >= steps.length) {
        throw new RangeError("Progress step index is out of range.");
      }
      active = index;
      render(index, "active", message);
    },
    complete(index, message) {
      if (!Number.isInteger(index) || index < 0 || index >= steps.length) {
        throw new RangeError("Progress step index is out of range.");
      }
      completed = Math.max(completed, index + 1);
      active = -1;
      render(index, "complete", message);
    },
    skip(index, message) {
      if (!Number.isInteger(index) || index < 0 || index >= steps.length) {
        throw new RangeError("Progress step index is out of range.");
      }
      completed = Math.max(completed, index + 1);
      active = -1;
      render(index, "skipped", message || `${steps[index]} skipped`);
    },
    fail(message) {
      if (active >= 0) render(active, "failed", message || `${steps[active]} failed`);
      active = -1;
    }
  };
}
