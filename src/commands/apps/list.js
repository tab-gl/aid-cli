import { apiJson } from "../../api/client.js";
import { table } from "../../ui/format.js";

export async function list() {
  const apps = await apiJson("/apps");

  table(apps, [
    {
      header: "ID",
      value: (a) => String(a.id)
    },
    {
      header: "NAME",
      value: (a) => a.name || "(unnamed)"
    }
  ]);
}
