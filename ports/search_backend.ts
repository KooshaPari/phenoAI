/** T76: phenoResearchEngine hexagonal port — SearchBackend. 3 adapters: Tantivy, Meili, Qdrant. */
export interface Doc {
  readonly id: string;
  readonly fields: Readonly<Record<string, string>>;
}
export interface Hit {
  readonly id: string;
  readonly score: number;
}
export interface SearchBackend {
  readonly backend: "tantivy" | "meili" | "qdrant";
  index(d: Doc): Promise<void>;
  delete(id: string): Promise<void>;
  query(q: string, limit: number): Promise<readonly Hit[]>;
}
