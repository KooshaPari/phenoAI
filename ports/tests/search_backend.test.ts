import { describe, expect, it } from "vitest";
import { QdrantBackend } from "../adapters/qdrant";
import { TantivyBackend } from "../adapters/tantivy";

describe("phenoResearchEngine ports", () => {
  it("TantivyBackend.backend", () => {
    expect(new TantivyBackend().backend).toBe("tantivy");
  });
  it("QdrantBackend.backend", () => {
    expect(new QdrantBackend().backend).toBe("qdrant");
  });
  it("TantivyBackend.index no-throw", async () => {
    await new TantivyBackend().index({ id: "x", fields: { title: "t" } });
  });
  it("TantivyBackend.query returns hits", async () => {
    const h = await new TantivyBackend().query("foo", 5);
    expect(h[0].id).toBe("foo");
  });
  it("SearchBackend interface object-safe", () => {
    const _s: import("../search_backend").SearchBackend = new TantivyBackend();
  });
});
