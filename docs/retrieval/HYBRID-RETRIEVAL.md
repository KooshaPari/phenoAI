# Hybrid Retrieval — phenoResearchEngine

This document describes the unified retrieval layer that combines a lexical
backend (Tantivy, which uses SQLite FTS5 under the hood) with a vector backend
(Qdrant) using **Reciprocal Rank Fusion (RRF)**.

## Why hybrid?

Lexical search is excellent for exact keywords, identifier matches, and
short, well-defined queries. Vector search is excellent for semantic
similarity and natural-language paraphrases. Neither alone is sufficient for
production retrieval:

- Pure lexical misses synonyms, paraphrases, and translations.
- Pure vector misses exact identifiers, codes, and rare keywords.

Hybrid retrieval gives the best of both — lexical for precision, vector for
recall — and fuses them into a single ranked list.

## Architecture

```
                 ┌──────────────────┐
                 │   Retriever      │
                 │  (interface)     │
                 └────────┬─────────┘
                          │
            ┌─────────────┴──────────────┐
            │                            │
   ┌────────▼──────────┐         ┌────────▼──────────┐
   │ ReciprocalRank    │         │ LexicalOnly       │
   │ Fusion (hybrid)   │         │ (degraded)        │
   └────────┬──────────┘         └────────┬──────────┘
            │                            │
   ┌────────┴────────┐                   │
   │                 │                   │
┌──▼────────┐  ┌─────▼─────┐      ┌───────▼────────┐
│ Tantivy   │  │ Qdrant    │      │ Tantivy        │
│ (FTS5)    │  │ (vector)  │      │ (FTS5)         │
└───────────┘  └───────────┘      └────────────────┘
```

The `Retriever` interface (`ports/retriever.ts`) defines:

```ts
interface Retriever {
  readonly mode: "lexical" | "vector" | "hybrid";
  query(q: RetrieverQuery): Promise<readonly RetrieverResult[]>;
}
```

`ReciprocalRankFusion` implements the hybrid mode and is the default
production implementation.

## Reciprocal Rank Fusion

RRF is a rank-based combiner that does not require score calibration between
backends. For a document `d` with per-backend ranks `rank_b(d)`, the fused
score is:

```
score_RRF(d) = Σ over backends b of  1 / (k + rank_b(d))
```

The default constant is `k = 60` (per the original RRF paper: Cormack et al.,
2009). A small `k` makes top ranks dominate; a large `k` flattens the curve
and gives more equal weight to lower-ranked hits.

Properties:

- **Rank-based, not score-based.** BM25 (FTS5) and cosine (Qdrant) scores
  are not directly comparable; RRF sidesteps this by using ranks.
- **Symmetric.** Either backend may be missing without breaking the
  combiner — the contribution from the missing backend is zero.
- **Tunable.** Adjust `k` per use case; `k = 0` degenerates to a sum of
  inverse ranks (`1`, `0.5`, `0.333…`, …).

## Adapter selection rules

The retriever takes a `lexical` backend (required) and an optional `vector`
backend:

| `vector` arg   | Behavior                                            |
| -------------- | --------------------------------------------------- |
| provided       | Hybrid RRF; both backends queried and fused.        |
| `null`         | Lexical-only pass-through.                          |
| `undefined`    | Lexical-only pass-through (via `buildRetriever`).   |

Use the `buildRetriever` factory to pick the most capable retriever
available in a given environment:

```ts
const r = buildRetriever({ lexical, vector });
// r.mode === "hybrid" if vector is truthy, else "lexical"
```

## Score monotonicity

For a single backend, RRF score is **strictly decreasing** in rank:

```
score(rank=1) = 1/(k+1)
score(rank=2) = 1/(k+2)
...
```

For two backends, a document that appears in both at rank 1 receives
`2/(k+1)`, which is **strictly greater** than any document that appears in
only one backend at rank 1 (`1/(k+1)`). This is the core "promotion" property
of hybrid retrieval: cross-source agreement outweighs single-source
confidence.

## Limits and candidate selection

Each backend is queried with `perBackendLimit` candidates (default 50).
After fusion, the final list is truncated to `limit` (default 10). Limits
apply in this order:

1. **Per-backend cap** — limits the size of each backend's response.
2. **Final cap** — limits the size of the fused list.

Limits are overridable per query:

```ts
await retriever.query({ text: "foo", perBackendLimit: 100, limit: 20 });
```

## Edge cases

| Case                              | Behavior                                       |
| --------------------------------- | ---------------------------------------------- |
| Empty lexical, populated vector    | Vector results included with `lexical` unset.  |
| Populated lexical, empty vector    | Lexical results included with `vector` unset.  |
| Both empty                        | Empty result.                                  |
| Same id in both backends           | Scores summed; `contributions` populated both. |
| `perBackendLimit = 0`             | No candidates; empty result.                   |
| `limit > candidate count`         | Returns all candidates.                        |

## Testing

Unit tests live in `ports/tests/retriever.test.ts`. The retriever has
**100 %** line, branch, and function coverage as measured by `vitest
--coverage`:

```bash
npx vitest run --coverage ports/tests/retriever.test.ts
```

The 37-test suite covers:

- Lexical-only fallback (empty, single hit, limits, monotonicity)
- Hybrid RRF (overlap, single-source, null vector, k=0 vs k=60, ties)
- Score monotonicity and determinism
- Input validation (missing lexical backend)
- `LexicalOnlyRetriever` (empty, RRF-shaped, limits, mode)
- `buildRetriever` factory (with/without vector)
- Edge cases (zero limit, oversized limit, self-overlap, RRF rank equivalence)

## References

- Cormack, G. V., Clarke, C. L. A., & Büttcher, S. (2009). *Reciprocal Rank
  Fusion outperforms Condorcet and individual Rank Learning Methods.*
  SIGIR '09.
- phenoResearchEngine architecture: `docs/architecture/ARCHITECTURE.md`
- Existing search backend: `ports/search_backend.ts`
- Vector adapter: `ports/adapters/qdrant.ts`
- Lexical adapter: `ports/adapters/tantivy.ts`
