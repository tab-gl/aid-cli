import {
  bold,
  dim,
  green,
  red,
  yellow
} from "./colors.js";

const ANSI_PATTERN = /\u001b\[[0-?]*[ -/]*[@-~]/g;

function terminalWidth() {
  const width = Number(process.stdout.columns);
  return Number.isFinite(width) && width > 0 ? Math.max(8, Math.floor(width)) : 80;
}

function visibleLength(value) {
  return [...String(value).replace(ANSI_PATTERN, "")].length;
}

function truncate(value, width) {
  const chars = [...String(value)];
  if (width <= 0) return "";
  if (chars.length <= width) return chars.join("");
  if (width === 1) return "…";
  return chars.slice(0, width - 1).join("") + "…";
}

function wrapText(value, width) {
  const text = String(value ?? "");
  if (width < 1) return [""];
  const lines = [];
  for (const paragraph of text.split(/\r?\n/)) {
    let line = "";
    for (const word of paragraph.split(/\s+/).filter(Boolean)) {
      if (!line) {
        if ([...word].length <= width) line = word;
        else {
          const chars = [...word];
          while (chars.length > width) lines.push(chars.splice(0, width).join(""));
          line = chars.join("");
        }
      } else if ([...line].length + 1 + [...word].length <= width) {
        line += ` ${word}`;
      } else {
        lines.push(line);
        if ([...word].length <= width) line = word;
        else {
          const chars = [...word];
          while (chars.length > width) lines.push(chars.splice(0, width).join(""));
          line = chars.join("");
        }
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
    Math.max(1, Math.min(Math.max(...visibleRows.map(([label]) => String(label).length)), Math.floor(width / 3)))
  );
  const valueWidth = Math.max(8, width - labelWidth - 6);

  console.log("");
  for (const [rawLabel, rawValue] of visibleRows) {
    const label = String(rawLabel);
    const value = String(rawValue);
    const inlineWidth = width - labelWidth - 4;
    const valueLines = wrapText(value, valueWidth);
    if (label.length > labelWidth) {
      for (const line of wrapText(label, labelWidth)) console.log(`  ${dim(line)}`);
      for (const line of wrapText(value, width - 4)) console.log(`    ${bold(line)}`);
      continue;
    }
    if (visibleLength(value) <= inlineWidth && !value.includes("\\n")) {
      console.log(`  ${dim(label.padEnd(labelWidth))}  ${bold(value)}`);
      continue;
    }
    console.log(`  ${dim(label)}`);
    for (const line of valueLines) console.log(`    ${bold(line)}`);
  }
  console.log("");
}

/* ── Responsive table ───────────────────── */

export function table(rows, columns) {
  if (!Array.isArray(rows)) {
    throw new Error("The API returned an unexpected list response. Expected an array.");
  }
  if (!Array.isArray(columns) || columns.length === 0) return;
  if (rows.length === 0) {
    console.log(`\n${dim("(empty)")}\n`);
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
    Math.max(header.length, ...normalized.map((row) => row[index].length))
  );
  const requiredWidth = 2 + naturalWidths.reduce((sum, width) => sum + width, 0) + 2 * (columns.length - 1);
  const width = terminalWidth();

  console.log("");
  if (requiredWidth <= width && naturalWidths.every((value) => value <= width)) {
    const pad = (value, size) => String(value).padEnd(size);
    console.log(`  ${headers.map((header, index) => dim(pad(header, naturalWidths[index]))).join("  ")}`);
    console.log(`  ${dim(naturalWidths.map((size) => "─".repeat(size)).join("──"))}`);
    for (const row of normalized) {
      console.log(`  ${row.map((value, index) => pad(value, naturalWidths[index])).join("  ")}`);
    }
  } else {
    // Narrow terminals get a stacked layout rather than a broken wide table.
    const primaryWidth = Math.max(8, width - 4);
    for (const row of normalized) {
      for (const titleLine of wrapText(row[0], primaryWidth)) console.log(`  ${bold(titleLine)}`);
      for (let index = 1; index < columns.length; index++) {
        for (const labelLine of wrapText(`${headers[index]}:`, Math.max(4, width - 4))) {
          console.log(`    ${dim(labelLine)}`);
        }
        for (const line of wrapText(row[index], Math.max(4, width - 6))) console.log(`      ${line}`);
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
    for (const wrapped of wrapText(line, terminalWidth() - 2)) console.log(`  ${wrapped}`);
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
    if (indent.length + command.length + gap.length + visibleLength(description) <= width) {
      console.log(`${indent}${bold(command)}${gap}${description}`);
      continue;
    }

    const safeCommand = truncate(command, Math.max(8, width - indent.length));
    console.log(`${indent}${bold(safeCommand)}`);
    const descriptionIndent = `${indent}  `;
    for (const line of wrapText(description, Math.max(8, width - descriptionIndent.length))) {
      console.log(`${descriptionIndent}${line}`);
    }
  }
  console.log("");
}
