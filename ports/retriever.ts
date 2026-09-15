/**
 * T77: phenoResearchEngine hexagonal port — Retriever (hybrid FTS5 + Qdrant via RRF).
 *
 * The Retriever combines a lexical backend (Tantivy, which uses SQLite FTS5
 * under the hood) and a vector backend (Qdrant) using Reciprocal Rank Fusion
 * (RRF) to produce a single ranked list. The hybrid score for a document `d`
 * given a per-backend ranking `rank_b(d)` is:
 *
 *     score_RRF(d) = sum over backends b of  1 / (k + rank_b(d))
 *
 * where `k` is a small constant (default 60) that dampens the contribution of
 * very high ranks. RRF is rank-based and does not require score calibration
 * between backends, which makes it ideal for combining heterogeneous signals
 * such as BM25 (FTS5) and cosine similarity (Qdrant).
 *
 * Adapter selection rules:
 *  - The lexical backend is the only required input. If the vector backend is
 *    omitted, Retriever degrades to a pure lexical pass-through.
 *  - If both backends are present, scores are fused via RRF. A document that
 *    appears only in one list still receives that list's contribution.
 *  - Limits are applied after fusion so the output cardinality is bounded.
 *
 * Public surface:
 *  - `Retriever` — the interface, accepts a `RetrieverQuery` and resolves
 *    to a `RetrieverResult[]`.
 *  - `ReciprocalRankFusion` — the default hybrid implementation.
 *  - `LexicalOnlyRetriever` — convenience wrapper that ignores the vector
 *    backend (handy for tests and for environments without a vector store).
 */
import type { Hit, SearchBackend } from "./search_backend";

/** A single ranked result returned by a `Retriever`. */
export interface RetrieverResult {
  /** Document id (stable across backends). */
  readonly id: string;
  /** Fused RRF score. Larger = more relevant. */
  readonly score: number;
  /** Per-backend contribution to the fused score. */
  readonly contributions: {
    readonly lexical?: number;
    readonly vector?: number;
  };
}

/** Configuration for the RRF combiner. */
export interface RetrieverConfig {
  /** RRF `k` constant. Default 60 (per the original RRF paper). */
  readonly k?: number;
  /** Maximum number of candidates to consider from each backend. */
  readonly perBackendLimit?: number;
  /** Final result cap applied after fusion. */
  readonly limit?: number;
}

export interface RetrieverQuery {
  readonly text: string;
  /** Optional override of the per-backend candidate limit. */
  readonly perBackendLimit?: number;
  /** Optional override of the final result cap. */
  readonly limit?: number;
}

/** Unified retrieval interface. */
export interface Retriever {
  readonly mode: "lexical" | "vector" | "hybrid";
  query(q: RetrieverQuery): Promise<readonly RetrieverResult[]>;
}

const DEFAULT_K = 60;
const DEFAULT_PER_BACKEND_LIMIT = 50;
const DEFAULT_LIMIT = 10;

/**
 * Hybrid retriever that fuses a lexical `SearchBackend` (e.g. Tantivy/FTS5)
 * and a vector `SearchBackend` (e.g. Qdrant) via Reciprocal Rank Fusion.
 *
 * The retriever is pure / side-effect free beyond the (idempotent) backend
 * `query` calls. It is safe to use concurrently; the implementation contains
 * no shared mutable state.
 */
export class ReciprocalRankFusion implements Retriever {
  readonly mode = "hybrid" as const;
  private readonly lexical: SearchBackend;
  private readonly vector: SearchBackend | null;
  private readonly cfg: Required<Omit<RetrieverConfig, never>>;

  constructor(opts: {
    lexical: SearchBackend;
    vector?: SearchBackend | null;
    config?: RetrieverConfig;
  }) {
    if (!opts.lexical) {
      throw new Error("ReciprocalRankFusion: lexical backend is required");
    }
    this.lexical = opts.lexical;
    this.vector = opts.vector ?? null;
    this.cfg = {
      k: opts.config?.k ?? DEFAULT_K,
      perBackendLimit:
        opts.config?.perBackendLimit ?? DEFAULT_PER_BACKEND_LIMIT,
      limit: opts.config?.limit ?? DEFAULT_LIMIT,
    };
  }

  async query(q: RetrieverQuery): Promise<readonly RetrieverResult[]> {
    const perBackend = q.perBackendLimit ?? this.cfg.perBackendLimit;
    const limit = q.limit ?? this.cfg.limit;

    const lex = await this.lexical.query(q.text, perBackend);
    const fused = new Map<string, RetrieverResult>();

    this.accumulate(fused, lex, "lexical");
    if (this.vector) {
      const vec = await this.vector.query(q.text, perBackend);
      this.accumulate(fused, vec, "vector");
    }

    const out = Array.from(fused.values()).sort(
      (a, b) => b.score - a.score || a.id.localeCompare(b.id),
    );
    return out.slice(0, limit);
  }

  private accumulate(
    acc: Map<string, RetrieverResult>,
    hits: readonly Hit[],
    lane: "lexical" | "vector",
  ): void {
    for (let i = 0; i < hits.length; i++) {
      const h = hits[i];
      const rank = i + 1;
      const contrib = 1 / (this.cfg.k + rank);
      const existing = acc.get(h.id);
      if (existing) {
        acc.set(h.id, {
          id: existing.id,
          score: existing.score + contrib,
          contributions: {
            ...existing.contributions,
            [lane]: contrib,
          },
        });
      } else {
        acc.set(h.id, {
          id: h.id,
          score: contrib,
          contributions: { [lane]: contrib },
        });
      }
    }
  }
}

/**
 * Convenience retriever that wraps a single lexical backend and discards any
 * vector input. Useful for unit tests and for degraded environments.
 */
export class LexicalOnlyRetriever implements Retriever {
  readonly mode = "lexical" as const;
  private readonly lexical: SearchBackend;
  private readonly cfg: Required<Pick<RetrieverConfig, "limit">>;

  constructor(opts: { lexical: SearchBackend; limit?: number }) {
    if (!opts.lexical) {
      throw new Error("LexicalOnlyRetriever: lexical backend is required");
    }
    this.lexical = opts.lexical;
    this.cfg = { limit: opts.limit ?? DEFAULT_LIMIT };
  }

  async query(q: RetrieverQuery): Promise<readonly RetrieverResult[]> {
    const limit = q.limit ?? this.cfg.limit;
    const hits = await this.lexical.query(q.text, limit);
    return hits.map((h, i) => ({
      id: h.id,
      score: 1 / (DEFAULT_K + (i + 1)),
      contributions: { lexical: 1 / (DEFAULT_K + (i + 1)) },
    }));
  }
}

/** Convenience factory: pick the most capable retriever available. */
export function buildRetriever(opts: {
  lexical: SearchBackend;
  vector?: SearchBackend | null;
  config?: RetrieverConfig;
}): Retriever {
  if (opts.vector) {
    return new ReciprocalRankFusion(opts);
  }
  return new LexicalOnlyRetriever({ lexical: opts.lexical });
}
