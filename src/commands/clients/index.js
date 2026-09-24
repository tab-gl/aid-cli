import { list } from "./list.js";
import { create } from "./create.js";
import { get } from "./get.js";
import { del } from "./delete.js";
import { rotate } from "./rotate.js";
import { withScreen } from "../../ui/screen.js";
import { usage } from "../../ui/format.js";
import { showBanner } from "../../ui/banner.js";

const subcommands = {
  list,
  create,
  get,
  delete: del,
  "rotate-secret": rotate
};

export async function clients(args = []) {
  const [sub, ...rest] = args;

  if (!sub || sub === "help" || sub === "--help") {
    await withScreen(async () => {
      showBanner();
      usage(`
USAGE

  aid clients <subcommand> [options]

SUBCOMMANDS

  list                                 List clients
  create --name <n> --redirect <uri>    Create a client
    [--redirect <uri>...]
    [--scope <s>...]
    [--id <client_id>]
    [--public]
  get <client_id>                       Show one client
  delete <client_id>                    Delete a client
  rotate-secret <client_id>             Rotate the secret
`);
    });

    return;
  }

  const handler = subcommands[sub];

  if (!handler) {
    throw new Error(`Unknown clients subcommand: ${sub}`);
  }

  await handler(rest);
}
