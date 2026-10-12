import test from "node:test";
import assert from "node:assert/strict";
import { commandList, fields, table } from "./format.js";

const ansi = /\u001b\[[0-?]*[ -/]*[@-~]/g;

function renderAtWidth(width, callback) {
  const previousColumns = Object.getOwnPropertyDescriptor(process.stdout, "columns");
  const originalLog = console.log;
  let output = "";
  Object.defineProperty(process.stdout, "columns", { configurable: true, value: width });
  console.log = (...args) => { output += args.join(" ") + "\n"; };
  try {
    callback();
  } finally {
    console.log = originalLog;
    if (previousColumns) Object.defineProperty(process.stdout, "columns", previousColumns);
    else delete process.stdout.columns;
  }
  return output.replace(ansi, "");
}

test("table uses compact columns when content fits", () => {
  const output = renderAtWidth(80, () => table(
    [{ id: "app-1", name: "Ace" }],
    [{ header: "ID", value: (row) => row.id }, { header: "NAME", value: (row) => row.name }]
  ));
  assert.match(output, /ID\s+NAME/);
  assert.match(output, /app-1\s+Ace/);
});

test("table switches to a stacked layout on narrow terminals", () => {
  const output = renderAtWidth(28, () => table(
    [{ id: "client_abcdefghijklmnopqrstuvwxyz", name: "A very long application name that must wrap" }],
    [{ header: "CLIENT ID", value: (row) => row.id }, { header: "APPLICATION NAME", value: (row) => row.name }]
  ));
  assert.ok(output.replace(/\s/g, "").includes("client_abcdefghijklmnopqrstuvwxyz"), output);
  assert.ok(output.replace(/\s/g, "").includes("APPLICATIONNAME:"), output);
  assert.ok(output.split("\n").every((line) => line.length <= 32), output);
});

test("fields wrap long values rather than forcing horizontal overflow", () => {
  const output = renderAtWidth(30, () => fields([
    ["Redirect URI", "https://example.test/a/very/long/callback/path?with=long-value"],
    ["Client ID", "client-123"]
  ]));
  assert.ok(output.replace(/\s/g, "").includes("RedirectURI"), output);
  assert.match(output, /client-123/);
  assert.ok(output.split("\n").every((line) => line.length <= 32), output);
});

test("command list wraps descriptions to fit mobile terminal widths", () => {
  const output = renderAtWidth(32, () => commandList([
    ["setup", "Configure the project and install the SDK safely"]
  ]));
  assert.match(output, /Configure the project/);
  assert.ok(output.split("\n").every((line) => line.length <= 34), output);
});

test("table rejects unexpected API response shapes with a useful error", () => {
  assert.throws(() => table({ items: [] }, [{ header: "ID", value: (row) => row.id }]), /unexpected list response/);
});

test("desktop terminals keep long resource IDs in an aligned table", () => {
  const output = renderAtWidth(80, () => table(
    [{ id: "client_".padEnd(72, "x"), name: "Desktop application" }],
    [{ header: "CLIENT ID", value: (row) => row.id }, { header: "NAME", value: (row) => row.name }]
  ));
  assert.match(output, /CLIENT ID\s+NAME/);
  assert.match(output, /Desktop\s+application/);
  assert.doesNotMatch(output, /NAME:/);
  assert.ok(output.split("\n").every((line) => line.length <= 80), output);
});

test("desktop table sizing handles wide Unicode characters without losing values", () => {
  const output = renderAtWidth(80, () => table(
    [{ id: "应用程序🧪", name: "Ace Identity 客户端" }],
    [{ header: "ID", value: (row) => row.id }, { header: "NAME", value: (row) => row.name }]
  ));
  assert.match(output, /应用程序🧪/);
  assert.match(output, /Ace Identity 客户端/);
  assert.match(output, /ID\s+NAME/);
});
