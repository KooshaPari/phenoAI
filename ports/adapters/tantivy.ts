import type { Doc, Hit, SearchBackend } from "../search_backend";

export class TantivyBackend implements SearchBackend {
  readonly backend = "tantivy" as const;
  async index(_d: Doc): Promise<void> {}
  async delete(_id: string): Promise<void> {}
  async query(q: string, limit: number): Promise<readonly Hit[]> {
    return [{ id: q, score: 1.0 }].slice(0, limit);
  }
}
