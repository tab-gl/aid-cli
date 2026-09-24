import { list } from "./list.js";
import { create } from "./create.js";
import { get } from "./get.js";
import { update } from "./update.js";
import { del } from "./delete.js";
import { withScreen } from "../../ui/screen.js";
import { usage } from "../../ui/format.js";
import { showBanner } from "../../ui/banner.js";

const subcommands = { list, create, get, update, delete: del };

export async function apps(args = []) {
  const [sub, ...rest] = args;

  if (!sub || sub === "help" || sub === "--help") {
    await withScreen(async () => {
      showBanner();
      usage(`
USAGE

  aid apps <subcommand> [options]

SUBCOMMANDS

  list                                 List applications
  create --name <name>                 Create an application
    [--description <text>]
  get <id>                              Show one application
  update <id> [--name <name>]           Update an application
    [--description <text>]
  delete <id>                           Delete an application
`);
    });

    return;
  }

  const handler = subcommands[sub];

  if (!handler) {
    throw new Error(`Unknown apps subcommand: ${sub}`);
  }

  await handler(rest);
}
