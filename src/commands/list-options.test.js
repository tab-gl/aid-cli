import test from "node:test";
import assert from "node:assert/strict";
import { filterResources, parseListOptions } from "./list-options.js";

test("list options parse JSON, case-insensitive search, and a positive limit", () => {
  assert.deepEqual(parseListOptions(["--json", "--search", "  WEB  ", "--limit", "3"], "usage"), {
    json: true,
    search: "web",
    limit: 3
  });
});

test("list options reject unknown flags, positionals, and invalid limits", () => {
  assert.throws(() => parseListOptions(["--mystery"], "usage"), /^Error: usage$/);
  assert.throws(() => parseListOptions(["extra"], "usage"), /^Error: usage$/);
  assert.throws(() => parseListOptions(["--limit", "0"], "usage"), /positive integer/);
  assert.throws(() => parseListOptions(["--limit", "2.5"], "usage"), /positive integer/);
});

test("resource filtering searches all selected fields and applies the limit afterward", () => {
  const resources = [
    { id: "web-1", name: "Production web" },
    { id: "mobile-1", name: "Mobile app" },
    { id: "web-2", name: "Preview web" }
  ];

  assert.deepEqual(
    filterResources(resources, "WEB", 1, (resource) => [resource.id, resource.name]),
    [resources[0]]
  );
  assert.deepEqual(
    filterResources(resources, "", undefined, (resource) => [resource.id]),
    resources
  );
});
