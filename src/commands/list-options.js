import { parseArgs } from "node:util";

export function parseListOptions(args, usage) {
  let parsed;
  try {
    parsed = parseArgs({
      args,
      options: {
        json: { type: "boolean" },
        search: { type: "string" },
        limit: { type: "string" }
      },
      strict: true,
      allowPositionals: false
    });
  } catch {
    throw new Error(usage);
  }

  let limit;
  if (parsed.values.limit !== undefined) {
    if (!/^\d+$/.test(parsed.values.limit)) throw new Error("--limit must be a positive integer.");
    limit = Number(parsed.values.limit);
    if (!Number.isSafeInteger(limit) || limit < 1) {
      throw new Error("--limit must be a positive integer.");
    }
  }

  return {
    json: parsed.values.json === true,
    search: parsed.values.search?.trim().toLocaleLowerCase() || "",
    limit
  };
}

export function filterResources(resources, searchText, limit, searchableValues) {
  const filtered = searchText
    ? resources.filter((resource) =>
        searchableValues(resource).some((value) =>
          String(value ?? "").toLocaleLowerCase().includes(searchText)
        )
      )
    : resources;

  return limit === undefined ? filtered : filtered.slice(0, limit);
}
