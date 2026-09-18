import { apiJson } from "../../api/client.js";
import { table } from "../../ui/format.js";

export async function list() {
  const clients = await apiJson("/clients");

  table(clients, [
    {
      header: "CLIENT ID",
      value: (c) => c.client_id
    },
    {
      header: "NAME",
      value: (c) => c.name
    },
    {
      header: "TYPE",
      value: (c) => (c.public ? "public" : "confidential")
    }
  ]);
}
