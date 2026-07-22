import { describe, expect, it } from "vitest";
import { dbTableIdentityKey, dbTableIdentityLabel, parseDbStableObjectKey } from "../inventory/dbIdentity";
import { columnRefFromNodeId, dbColumnNodeId, dbTableNodeId, tableKeyFromDbNodeId } from "./nodeIds";

describe("database node identity", () => {
  it("keeps ordinary ids stable", () => {
    const tableKey = dbTableIdentityKey("public", "orders");
    expect(tableKey).toBe("public.orders");
    expect(dbTableNodeId(tableKey)).toBe("db:table:public.orders");
    expect(dbColumnNodeId(tableKey, "customer_id")).toBe("db:column:public.orders:customer_id");
  });

  it("round-trips delimiter characters without identity collisions", () => {
    const tableKey = dbTableIdentityKey("audit.2026", "order:events");
    const otherTableKey = dbTableIdentityKey("audit", "2026.order:events");
    const columnId = dbColumnNodeId(tableKey, "value:raw%text");

    expect(tableKey).toBe("audit%2E2026.order%3Aevents");
    expect(tableKey).not.toBe(otherTableKey);
    expect(dbTableIdentityLabel(tableKey)).toBe("audit.2026.order:events");
    expect(tableKeyFromDbNodeId(columnId)).toBe(tableKey);
    expect(columnRefFromNodeId(columnId)).toEqual({ tableKey, columnName: "value:raw%text" });
  });

  it("rejects malformed encoded column ids", () => {
    expect(columnRefFromNodeId("db:column:public.orders:value%2Fraw")).toBeNull();
  });

  it("parses versioned engine keys without losing reserved characters", () => {
    expect(parseDbStableObjectKey("v2:postgres:prod%3Awest:app:public:table:order%3Aevents")).toMatchObject({
      database: "app",
      schema: "public",
      kind: "table",
      objectName: "order:events",
    });
    expect(parseDbStableObjectKey("v2:postgres:prod:app:public:table:orders%2Farchive")).toBeNull();
  });
});
