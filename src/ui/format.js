import {
  bold,
  dim,
  green,
  red,
  yellow
} from "./colors.js";

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

/* ── Headings ───────────────────────────── */

export function heading(text) {
  console.log(`\n${dim(text.toUpperCase())}\n`);
}

/* ── Key / value block ──────────────────── */

/*
 * Prints aligned key-value rows:
 *
 *   Client ID    c_h2q2yjdkoy5w
 *   Name         OAuth Test
 *   Type         public
 *
 * Labels are dim, values are bold. Pads to the
 * longest label so colon-free output still
 * reads as a table.
 */
export function fields(rows) {
  if (rows.length === 0) return;

  const labelWidth = Math.max(
    ...rows.map(([k]) => k.length)
  );

  console.log("");

  for (const [label, value] of rows) {
    if (value === undefined || value === null) continue;

    const padded = label.padEnd(labelWidth);
    console.log(`  ${dim(padded)}  ${bold(String(value))}`);
  }

  console.log("");
}

/* ── Table ──────────────────────────────── */

/*
 * Simple two-column table. Column widths are
 * computed from the data, not hardcoded, so
 * long client_ids or names don't wrap on
 * narrow terminals.
 */
export function table(rows, columns) {
  if (rows.length === 0) {
    console.log(`\n${dim("(empty)")}\n`);
    return;
  }

  const widths = columns.map((col) =>
    Math.max(
      col.header.length,
      ...rows.map((r) => String(col.value(r)).length)
    )
  );

  const pad = (s, w) => String(s).padEnd(w);

  console.log("");

  // Header row
  const header = columns
    .map((col, i) => dim(pad(col.header, widths[i])))
    .join("  ");
  console.log(`  ${header}`);

  // Divider
  const divider = widths.map((w) => "─".repeat(w)).join("──");
  console.log(`  ${dim(divider)}`);

  // Body
  for (const row of rows) {
    const line = columns
      .map((col, i) => pad(col.value(row), widths[i]))
      .join("  ");
    console.log(`  ${line}`);
  }

  console.log("");
}

/* ── Usage ──────────────────────────────── */

export function usage(text) {
  console.log("");
  for (const line of text.trim().split("\n")) {
    console.log(`  ${line}`);
  }
  console.log("");
}
