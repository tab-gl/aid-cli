import { green, dim, yellow } from "./colors.js";

const ANSI_PATTERN = /\u001b\[[0-?]*[ -/]*[@-~]/g;
const SEGMENTER = typeof Intl.Segmenter === "function"
  ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
  : null;

function graphemes(value) {
  const text = String(value);
  return SEGMENTER ? [...SEGMENTER.segment(text)].map((part) => part.segment) : [...text];
}

function graphemeWidth(grapheme) {
  if (!grapheme || /^[\p{Mark}\u200d\uFE0E\uFE0F]+$/u.test(grapheme)) return 0;
  for (const character of grapheme) {
    const code = character.codePointAt(0);
    if (
      (code >= 0x1100 && code <= 0x11ff) ||
      (code >= 0x2e80 && code <= 0xa4cf) ||
      (code >= 0xac00 && code <= 0xd7af) ||
      (code >= 0xf900 && code <= 0xfaff) ||
      (code >= 0xfe10 && code <= 0xfe6f) ||
      (code >= 0xff00 && code <= 0xff60) ||
      (code >= 0xffe0 && code <= 0xffe6) ||
      (code >= 0x1f000 && code <= 0x1faff) ||
      (code >= 0x20000 && code <= 0x3ffff)
    ) return 2;
  }
  return 1;
}

function visibleWidth(value) {
  return graphemes(String(value).replace(ANSI_PATTERN, ""))
    .reduce((width, part) => width + graphemeWidth(part), 0);
}

function wrapLabel(value, width) {
  const text = String(value);
  if (width < 1) return [""];
  const lines = [];
  let line = "";
  let used = 0;

  for (const word of text.split(/\s+/).filter(Boolean)) {
    const chars = graphemes(word);
    const wordWidth = chars.reduce((sum, char) => sum + graphemeWidth(char), 0);
    if (wordWidth > width) {
      if (line) lines.push(line);
      line = "";
      used = 0;
      for (const char of chars) {
        const size = graphemeWidth(char);
        if (line && used + size > width) {
          lines.push(line);
          line = "";
          used = 0;
        }
        line += char;
        used += size;
      }
    } else if (!line) {
      line = word;
      used = wordWidth;
    } else if (used + 1 + wordWidth <= width) {
      line += ` ${word}`;
      used += 1 + wordWidth;
    } else {
      lines.push(line);
      line = word;
      used = wordWidth;
    }
  }

  if (line) lines.push(line);
  return lines.length ? lines : [""];
}

/**
 * Step-based progress designed for both wide desktop terminals and narrow
 * mobile terminals. Updates use separate lines to preserve terminal scrollback.
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
    const columns = Number(stream.columns);
    const terminalColumns = Number.isFinite(columns) && columns > 0
      ? Math.max(8, Math.floor(columns))
      : 80;
    const barWidth = Math.max(3, Math.min(18, terminalColumns - 26));
    const filled = Math.max(status === "active" ? 1 : 0, Math.round((progress / total) * barWidth));
    const bar = `${"█".repeat(filled)}${"░".repeat(Math.max(0, barWidth - filled))}`;
    const label = String(message || steps[index] || "Setup");
    const number = `${Math.min(index + 1, total)}/${total}`;
    const marker = status === "complete" ? green("✓")
      : status === "failed" ? yellow("!")
      : status === "skipped" ? yellow("–")
      : dim("›");
    const prefix = `  ${marker} [${bar}] ${String(percent).padStart(3)}%  ${number}  `;
    const labelWidth = Math.max(1, terminalColumns - visibleWidth(prefix));
    const lines = wrapLabel(label, labelWidth);
    stream.write(`${prefix}${lines[0]}\n`);

    if (lines.length > 1) {
      const indent = " ".repeat(Math.min(4, Math.max(1, terminalColumns - 1)));
      const continuationWidth = Math.max(1, terminalColumns - visibleWidth(indent));
      for (const line of lines.slice(1)) {
        for (const wrapped of wrapLabel(line, continuationWidth)) {
          stream.write(`${indent}${wrapped}\n`);
        }
      }
    }
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
