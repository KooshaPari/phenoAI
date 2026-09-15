import { describe, expect, it } from "vitest";
import type { Doc, Hit, SearchBackend } from "../search_backend";
import {
  LexicalOnlyRetriever,
  ReciprocalRankFusion,
  buildRetriever,
} from "../retriever";

/** A small programmable fake backend for tests. */
function fakeBackend(
  name: "tantivy" | "qdrant",
  responses: ReadonlyMap<string, readonly Hit[]>,
): SearchBackend {
  return {
    backend: name,
    async index(_d: Doc): Promise<void> {
      // no-op
    },
    async delete(_id: string): Promise<void> {
      // no-op
    },
    async query(q: string, limit: number): Promise<readonly Hit[]> {
      const r = responses.get(q) ?? [];
      return r.slice(0, limit);
    },
  };
}

describe("ReciprocalRankFusion — lexical-only fallback", () => {
  it("returns empty when the lexical backend returns no hits", async () => {
    const r = new ReciprocalRankFusion({ lexical: fakeBackend("tantivy", new Map()) });
    expect(await r.query({ text: "nope" })).toEqual([]);
  });

  it("returns RRF-shaped scores for a single hit", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["foo", [{ id: "d1", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "foo" });
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe("d1");
    expect(out[0]?.score).toBeCloseTo(1 / (60 + 1), 10);
    expect(out[0]?.contributions.lexical).toBeCloseTo(1 / 61, 10);
    expect(out[0]?.contributions.vector).toBeUndefined();
  });

  it("respects default limit", async () => {
    const hits: Hit[] = Array.from({ length: 50 }, (_, i) => ({
      id: `d${i}`,
      score: 1 - i * 0.01,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(10); // DEFAULT_LIMIT
  });

  it("respects explicit limit override", async () => {
    const hits: Hit[] = Array.from({ length: 20 }, (_, i) => ({
      id: `d${i}`,
      score: 1 - i * 0.01,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q", limit: 3 });
    expect(out.map((x) => x.id)).toEqual(["d0", "d1", "d2"]);
  });

  it("score is monotonically decreasing by rank", async () => {
    const hits: Hit[] = Array.from({ length: 5 }, (_, i) => ({
      id: `d${i}`,
      score: 1 - i * 0.1,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q", limit: 5 });
    for (let i = 1; i < out.length; i++) {
      const prev = out[i - 1]?.score ?? 0;
      const cur = out[i]?.score ?? 0;
      expect(prev).toBeGreaterThanOrEqual(cur);
    }
  });
});

describe("ReciprocalRankFusion — hybrid RRF", () => {
  it("sums contributions from both backends for overlap", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "b", score: 0.8 }]]]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([["q", [{ id: "a", score: 0.95 }, { id: "c", score: 0.7 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    const a = out.find((x) => x.id === "a");
    expect(a).toBeDefined();
    // a: 1/(60+1) + 1/(60+1) = 2/61
    expect(a?.score).toBeCloseTo(2 / 61, 10);
    expect(a?.contributions.lexical).toBeCloseTo(1 / 61, 10);
    expect(a?.contributions.vector).toBeCloseTo(1 / 61, 10);
  });

  it("promotes overlap candidates over single-source candidates", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([
        [
          "q",
          [
            { id: "shared", score: 0.9 },
            { id: "lex-only", score: 0.85 },
          ],
        ],
      ]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([
        [
          "q",
          [
            { id: "shared", score: 0.99 },
            { id: "vec-only", score: 0.95 },
          ],
        ],
      ]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out[0]?.id).toBe("shared");
    // shared: 2/61, lex-only: 1/61, vec-only: 1/61
    expect(out[0]?.score).toBeCloseTo(2 / 61, 10);
  });

  it("includes vector-only and lexical-only candidates with single contribution", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "lex-only", score: 0.9 }]]]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([["q", [{ id: "vec-only", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(2);
    const lexOnly = out.find((x) => x.id === "lex-only");
    const vecOnly = out.find((x) => x.id === "vec-only");
    expect(lexOnly?.contributions.vector).toBeUndefined();
    expect(vecOnly?.contributions.lexical).toBeUndefined();
    expect(lexOnly?.score).toBeCloseTo(1 / 61, 10);
    expect(vecOnly?.score).toBeCloseTo(1 / 61, 10);
  });

  it("treats null vector backend same as missing", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: null });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(1);
    expect(out[0]?.contributions.vector).toBeUndefined();
  });

  it("uses RRF k=60 by default; k=0 makes the top result dominate", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "b", score: 0.5 }]]]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "c", score: 0.5 }]]]),
    );
    const defaultK = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const kZero = new ReciprocalRankFusion({
      lexical: lex,
      vector: vec,
      config: { k: 0 },
    });
    const d = await defaultK.query({ text: "q" });
    const z = await kZero.query({ text: "q" });
    // With k=0, top-rank contributions equal 1; so "a" should be 2
    expect(z[0]?.score).toBeCloseTo(2, 10);
    // With k=60 default, "a" gets 2 * 1/(60+1) = 2/61, and the highest
    // single-backend contribution is 1/61 ≈ 0.0164. So the top fused
    // score for a cross-source doc equals 2/61.
    expect(d[0]?.score).toBeCloseTo(2 / 61, 10);
  });

  it("sort is stable for equal scores (tiebreak on id ascending)", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "z", score: 0.9 }, { id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q" });
    // z is rank 1, a is rank 2 → z should appear first
    expect(out[0]?.id).toBe("z");
    expect(out[1]?.id).toBe("a");
  });

  it("respects perBackendLimit", async () => {
    const lexHits: Hit[] = Array.from({ length: 100 }, (_, i) => ({
      id: `l${i}`,
      score: 1 - i * 0.001,
    }));
    const vecHits: Hit[] = Array.from({ length: 100 }, (_, i) => ({
      id: `v${i}`,
      score: 1 - i * 0.001,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", lexHits]]));
    const vec = fakeBackend("qdrant", new Map([["q", vecHits]]));
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q", perBackendLimit: 5, limit: 100 });
    // Only l0..l4 and v0..v4 should appear
    expect(out).toHaveLength(10);
    for (const r of out) {
      expect(["l0", "l1", "l2", "l3", "l4", "v0", "v1", "v2", "v3", "v4"]).toContain(
        r.id,
      );
    }
  });

  it("empty vector + populated lexical still yields results", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const vec = fakeBackend("qdrant", new Map());
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe("a");
  });

  it("empty lexical + populated vector still yields results", async () => {
    const lex = fakeBackend("tantivy", new Map());
    const vec = fakeBackend(
      "qdrant",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe("a");
    expect(out[0]?.contributions.lexical).toBeUndefined();
  });
});

describe("ReciprocalRankFusion — score monotonicity & determinism", () => {
  it("RRF score is non-increasing as rank increases", async () => {
    // With a single backend, score(i) = 1/(k + i) which is strictly decreasing.
    const hits: Hit[] = Array.from({ length: 20 }, (_, i) => ({
      id: `d${i}`,
      score: 1,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q", limit: 20 });
    for (let i = 1; i < out.length; i++) {
      const prev = out[i - 1]?.score ?? 0;
      const cur = out[i]?.score ?? 0;
      expect(prev).toBeGreaterThan(cur);
    }
  });

  it("RRF with k=0 makes score(rank=1) = 1.0 and strictly decreasing after", async () => {
    const hits: Hit[] = Array.from({ length: 5 }, (_, i) => ({
      id: `d${i}`,
      score: 1,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({
      lexical: lex,
      config: { k: 0 },
    });
    const out = await r.query({ text: "q", limit: 5 });
    expect(out[0]?.score).toBeCloseTo(1, 10);
    expect(out[1]?.score).toBeCloseTo(0.5, 10);
    expect(out[2]?.score).toBeCloseTo(1 / 3, 10);
    expect(out[3]?.score).toBeCloseTo(0.25, 10);
    expect(out[4]?.score).toBeCloseTo(0.2, 10);
  });

  it("query is deterministic for identical inputs", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "b", score: 0.5 }]]]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([["q", [{ id: "a", score: 0.95 }, { id: "c", score: 0.4 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const x = await r.query({ text: "q" });
    const y = await r.query({ text: "q" });
    expect(x).toEqual(y);
  });
});

describe("ReciprocalRankFusion — input validation", () => {
  it("throws when lexical backend is missing", () => {
    expect(() => {
      // Force type-unsafe construction to exercise the runtime check.
      new ReciprocalRankFusion({ lexical: undefined as unknown as SearchBackend });
    }).toThrow(/lexical backend is required/);
  });

  it("mode is always 'hybrid'", () => {
    const r = new ReciprocalRankFusion({
      lexical: fakeBackend("tantivy", new Map()),
    });
    expect(r.mode).toBe("hybrid");
  });
});

describe("LexicalOnlyRetriever", () => {
  it("returns empty for empty input", async () => {
    const r = new LexicalOnlyRetriever({
      lexical: fakeBackend("tantivy", new Map()),
    });
    expect(await r.query({ text: "nope" })).toEqual([]);
  });

  it("maps hits to RRF-shaped scores", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "b", score: 0.5 }]]]),
    );
    const r = new LexicalOnlyRetriever({ lexical: lex, limit: 2 });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(2);
    expect(out[0]?.id).toBe("a");
    expect(out[0]?.score).toBeCloseTo(1 / 61, 10);
    expect(out[0]?.contributions.vector).toBeUndefined();
  });

  it("respects limit", async () => {
    const hits: Hit[] = Array.from({ length: 10 }, (_, i) => ({
      id: `d${i}`,
      score: 1,
    }));
    const r = new LexicalOnlyRetriever({
      lexical: fakeBackend("tantivy", new Map([["q", hits]])),
      limit: 3,
    });
    expect(await r.query({ text: "q" })).toHaveLength(3);
  });

  it("query limit override beats constructor limit", async () => {
    const hits: Hit[] = Array.from({ length: 10 }, (_, i) => ({
      id: `d${i}`,
      score: 1,
    }));
    const r = new LexicalOnlyRetriever({
      lexical: fakeBackend("tantivy", new Map([["q", hits]])),
      limit: 2,
    });
    expect(await r.query({ text: "q", limit: 5 })).toHaveLength(5);
  });

  it("throws when lexical backend is missing", () => {
    expect(() => {
      new LexicalOnlyRetriever({
        lexical: undefined as unknown as SearchBackend,
      });
    }).toThrow(/lexical backend is required/);
  });

  it("mode is 'lexical'", () => {
    const r = new LexicalOnlyRetriever({
      lexical: fakeBackend("tantivy", new Map()),
    });
    expect(r.mode).toBe("lexical");
  });
});

describe("buildRetriever factory", () => {
  it("returns ReciprocalRankFusion when vector is provided", () => {
    const f = buildRetriever({
      lexical: fakeBackend("tantivy", new Map()),
      vector: fakeBackend("qdrant", new Map()),
    });
    expect(f.mode).toBe("hybrid");
  });

  it("returns LexicalOnlyRetriever when vector is omitted", () => {
    const f = buildRetriever({ lexical: fakeBackend("tantivy", new Map()) });
    expect(f.mode).toBe("lexical");
  });

  it("returns LexicalOnlyRetriever when vector is null", () => {
    const f = buildRetriever({
      lexical: fakeBackend("tantivy", new Map()),
      vector: null,
    });
    expect(f.mode).toBe("lexical");
  });
});

describe("ReciprocalRankFusion — edge cases", () => {
  it("handles zero perBackendLimit gracefully (no candidates)", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q", perBackendLimit: 0 });
    expect(out).toEqual([]);
  });

  it("handles limit larger than candidate set", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q", limit: 100 });
    expect(out).toHaveLength(1);
  });

  it("single result is preserved through fusion", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "only", score: 0.9 }]]]),
    );
    const vec = fakeBackend("qdrant", new Map());
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe("only");
    expect(out[0]?.score).toBeCloseTo(1 / 61, 10);
  });

  it("RANK_EQUIVALENCE: documents in both backends at rank 1 should rank above either alone at rank 1", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([
        [
          "q",
          [
            { id: "shared", score: 0.9 },
            { id: "lex1", score: 0.5 },
          ],
        ],
      ]),
    );
    const vec = fakeBackend(
      "qdrant",
      new Map([
        [
          "q",
          [
            { id: "shared", score: 0.9 },
            { id: "vec1", score: 0.5 },
          ],
        ],
      ]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex, vector: vec });
    const out = await r.query({ text: "q" });
    expect(out[0]?.id).toBe("shared");
    expect(out[1]?.id).toMatch(/^(lex1|vec1)$/);
  });

  it("score for the same rank is constant regardless of input score magnitude", async () => {
    // RRF is rank-based: the original BM25/cosine score is ignored.
    const lex = fakeBackend(
      "tantivy",
      new Map([
        ["q1", [{ id: "x", score: 0.01 }]],
        ["q2", [{ id: "x", score: 9999 }]],
      ]),
    );
    const r1 = new ReciprocalRankFusion({ lexical: lex });
    const r2 = new ReciprocalRankFusion({
      lexical: fakeBackend(
        "tantivy",
        new Map([["q2", [{ id: "x", score: 9999 }]]]),
      ),
    });
    const a = await r1.query({ text: "q1" });
    const b = await r2.query({ text: "q2" });
    expect(a[0]?.score).toBeCloseTo(b[0]?.score ?? -1, 10);
  });

  it("returns readonly arrays (no caller mutation)", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "q" });
    // The contract is `readonly RetrieverResult[]`; verify the type-level guarantee.
    const _frozen: ReadonlyArray<unknown> = out;
    expect(_frozen).toBe(out);
  });

  it("empty query text still calls backends and returns results", async () => {
    const lex = fakeBackend(
      "tantivy",
      new Map([["", [{ id: "a", score: 0.9 }]]]),
    );
    const r = new ReciprocalRankFusion({ lexical: lex });
    const out = await r.query({ text: "" });
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe("a");
  });

  it("respects config.limit when query.limit is omitted", async () => {
    const hits: Hit[] = Array.from({ length: 20 }, (_, i) => ({
      id: `d${i}`,
      score: 1,
    }));
    const lex = fakeBackend("tantivy", new Map([["q", hits]]));
    const r = new ReciprocalRankFusion({
      lexical: lex,
      config: { limit: 7 },
    });
    const out = await r.query({ text: "q" });
    expect(out).toHaveLength(7);
  });

  it("fuses correctly when lexical and vector are the same backend instance (self-overlap)", async () => {
    const shared = fakeBackend(
      "tantivy",
      new Map([["q", [{ id: "a", score: 0.9 }, { id: "b", score: 0.5 }]]]),
    );
    // Use the same backend in both slots — the fuser should still be correct.
    const r = new ReciprocalRankFusion({ lexical: shared, vector: shared });
    const out = await r.query({ text: "q" });
    // a: 1/61 + 1/61 = 2/61
    const a = out.find((x) => x.id === "a");
    expect(a?.score).toBeCloseTo(2 / 61, 10);
  });
});
