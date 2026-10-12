import {
  bold,
  dim,
  green,
  red,
  yellow
} from "./colors.js";

const ANSI_PATTERN = /\u001b\[[0-?]*[ -/]*[@-~]/g;
const SEGMENTER = typeof Intl.Segmenter === "function"
  ? new Intl.Segmenter(undefined, { granularity: "grapheme" })
  : null;

function terminalWidth() {
  const width = Number(process.stdout.columns);
  return Number.isFinite(width) && width > 0 ? Math.max(8, Math.floor(width)) : 80;
}

function graphemes(value) {
  const text = String(value);
  return SEGMENTER
    ? [...SEGMENTER.segment(text)].map((part) => part.segment)
    : [...text];
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

function visibleLength(value) {
  return graphemes(String(value).replace(ANSI_PATTERN, ""))
    .reduce((width, part) => width + graphemeWidth(part), 0);
}

function padDisplay(value, width) {
  const text = String(value);
  return text + " ".repeat(Math.max(0, width - visibleLength(text)));
}

function truncate(value, width) {
  const chars = graphemes(String(value));
  if (width <= 0) return "";
  if (visibleLength(chars.join("")) <= width) return chars.join("");
  if (width === 1) return "…";

  let output = "";
  let used = 0;
  for (const character of chars) {
    const size = graphemeWidth(character);
    if (used + size > width - 1) break;
    output += character;
    used += size;
  }
  return output + "…";
}

function splitLongWord(word, width) {
  const parts = [];
  let line = "";
  let used = 0;
  for (const character of graphemes(word)) {
    const size = graphemeWidth(character);
    if (line && used + size > width) {
      parts.push(line);
      line = "";
      used = 0;
    }
    line += character;
    used += size;
  }
  if (line) parts.push(line);
  return parts.length ? parts : [""];
}

function wrapText(value, width) {
  const text = String(value ?? "");
  if (width < 1) return [""];
  const lines = [];

  for (const paragraph of text.split(/\r?\n/)) {
    let line = "";
    let lineWidth = 0;

    for (const word of paragraph.split(/\s+/).filter(Boolean)) {
      const wordWidth = visibleLength(word);
      if (wordWidth > width) {
        if (line) {
          lines.push(line);
          line = "";
          lineWidth = 0;
        }
        const pieces = splitLongWord(word, width);
        lines.push(...pieces.slice(0, -1));
        line = pieces[pieces.length - 1];
        lineWidth = visibleLength(line);
      } else if (!line) {
        line = word;
        lineWidth = wordWidth;
      } else if (lineWidth + 1 + wordWidth <= width) {
        line += ` ${word}`;
        lineWidth += 1 + wordWidth;
      } else {
        lines.push(line);
        line = word;
        lineWidth = wordWidth;
      }
    }

    if (line) lines.push(line);
    else if (!paragraph) lines.push("");
  }

  return lines.length ? lines : [""];
}

/* ── Status lines ───────────────────────── */

export function success(message) {
  console.log(`\n${green("✓")} ${message}\n`);
}

export function failure(message) {
  console.error(`\n${red("✗")} ${message}\n`);
}

export function warn(message) {
  console.error(`${yellow("!")} ${message}`);
}

export function info(message) {
  const width = terminalWidth();
  for (const line of wrapText(message, width - 2)) console.log(`  ${line}`);
}

/* ── Headings ───────────────────────────── */

export function heading(text) {
  console.log(`\n${dim(String(text).toUpperCase())}\n`);
}

/* ── Key / value block ──────────────────── */

export function fields(rows) {
  const visibleRows = (Array.isArray(rows) ? rows : []).filter(
    (row) => Array.isArray(row) && row.length >= 2 && row[1] !== undefined && row[1] !== null
  );
  if (visibleRows.length === 0) return;

  const width = terminalWidth();
  const labelWidth = Math.min(
    24,
    Math.max(1, Math.min(
      Math.max(...visibleRows.map(([label]) => visibleLength(label))),
      Math.floor(width / 3)
    ))
  );
  const valueWidth = Math.max(1, width - labelWidth - 6);

  console.log("");
  for (const [rawLabel, rawValue] of visibleRows) {
    const label = String(rawLabel);
    const value = String(rawValue);
    const labelSize = visibleLength(label);
    const valueLines = wrapText(value, valueWidth);

    if (labelSize > labelWidth) {
      for (const line of wrapText(label, labelWidth)) console.log(`  ${dim(line)}`);
      for (const line of wrapText(value, Math.max(1, width - 4))) console.log(`    ${bold(line)}`);
      continue;
    }

    if (visibleLength(value) <= valueWidth && !value.includes("\n")) {
      console.log(`  ${dim(padDisplay(label, labelWidth))}  ${bold(value)}`);
      continue;
    }

    console.log(`  ${dim(label)}`);
    for (const line of valueLines) console.log(`    ${bold(line)}`);
  }
  console.log("");
}

/* ── Responsive table ───────────────────── */

function allocateColumnWidths(naturalWidths, headers, available) {
  const widths = naturalWidths.map((natural, index) =>
    Math.min(natural, Math.max(6, Math.min(18, visibleLength(headers[index]))))
  );
  let remaining = available - widths.reduce((sum, value) => sum + value, 0);

  while (remaining > 0) {
    let target = -1;
    let largestNeed = 0;
    for (let index = 0; index < widths.length; index++) {
      const need = naturalWidths[index] - widths[index];
      if (need > largestNeed) {
        target = index;
        largestNeed = need;
      }
    }
    if (target < 0) break;
    widths[target]++;
    remaining--;
  }

  return widths;
}

function renderWideTable(normalized, headers, naturalWidths, width) {
  const gap = "  ";
  const available = Math.max(1, width - 3 - gap.length * (headers.length - 1));
  const widths = naturalWidths.reduce((sum, value) => sum + value, 0) <= available
    ? naturalWidths
    : allocateColumnWidths(naturalWidths, headers, available);

  const headerLines = headers.map((header, index) => wrapText(header, widths[index]));
  const headerHeight = Math.max(...headerLines.map((lines) => lines.length));
  for (let line = 0; line < headerHeight; line++) {
    console.log(`  ${headerLines.map((lines, index) =>
      dim(padDisplay(lines[line] || "", widths[index]))
    ).join(gap)}`);
  }
  console.log(`  ${widths.map((size) => "─".repeat(size)).join("──")}`);

  for (const row of normalized) {
    const cells = row.map((value, index) => wrapText(value, widths[index]));
    const height = Math.max(...cells.map((lines) => lines.length));
    for (let line = 0; line < height; line++) {
      console.log(`  ${cells.map((lines, index) =>
        padDisplay(lines[line] || "", widths[index])
      ).join(gap)}`);
    }
  }
}

/* ── Table ───────────────────────────────── */

export function table(rows, columns) {
  if (!Array.isArray(rows)) {
    throw new Error("The API returned an unexpected list response. Expected an array.");
  }
  if (!Array.isArray(columns) || columns.length === 0) return;
  if (rows.length === 0) {
    console.log(`\n${dim("(empty")}\n`);
    return;
  }

  const normalized = rows.map((row) => columns.map((column) => {
    let value;
    try {
      value = column.value(row);
    } catch {
      value = null;
    }
    return value === undefined || value === null ? "—" : String(value);
  }));
  const headers = columns.map((column) => String(column.header));
  const naturalWidths = headers.map((header, index) =>
    Math.max(visibleLength(header), ...normalized.map((row) => visibleLength(row[index])))
  );
  const width = terminalWidth();
  const minimumTableWidth = 2 + columns.length * 6 + Math.max(0, columns.length - 1) * 2;

  console.log("");
  if (width >= 72 && width >= minimumTableWidth) {
    renderWideTable(normalized, headers, naturalWidths, width);
  } else {
    // Narrow terminals get a stacked layout rather than a broken wide table.
    const primaryWidth = Math.max(1, width - 4);
    for (const row of normalized) {
      for (const titleLine of wrapText(row[0], primaryWidth)) console.log(`  ${bold(titleLine)}`);
      for (let index = 1; index < columns.length; index++) {
        for (const labelLine of wrapText(`${headers[index]}:`, Math.max(1, width - 4))) {
          console.log(`    ${dim(labelLine)}`);
        }
        for (const line of wrapText(row[index], Math.max(1, width - 6))) console.log(`      ${line}`);
      }
      console.log("");
    }
  }
  console.log("");
}

/* ── Usage ──────────────────────────────── */

export function usage(text) {
  console.log("");
  for (const line of String(text).trim().split("\n")) {
    for (const wrapped of wrapText(line, Math.max(1, terminalWidth() - 2))) console.log(`  ${wrapped}`);
  }
  console.log("");
}

/* ── Responsive command list ───────────── */

export function commandList(rows) {
  if (!Array.isArray(rows) || rows.length === 0) return;

  const width = terminalWidth();
  const indent = "    ";
  const gap = "  ";
  console.log("");

  for (const [rawCommand, rawDescription = ""] of rows) {
    const command = String(rawCommand);
    const description = String(rawDescription);
    if (visibleLength(indent + command + gap) + visibleLength(description) <= width) {
      console.log(`${indent}${bold(command)}${gap}${description}`);
      continue;
    }

    const safeCommand = truncate(command, Math.max(1, width - visibleLength(indent)));
    console.log(`${indent}${bold(safeCommand)}`);
    const descriptionIndent = `${indent}  `;
    for (const line of wrapText(description, Math.max(1, width - visibleLength(descriptionIndent)))) {
      console.log(`${descriptionIndent}${line}`);
    }
  }
  console.log("");
}
