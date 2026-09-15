import type { Doc, Hit, SearchBackend } from "../search_backend";

export class QdrantBackend implements SearchBackend {
  readonly backend = "qdrant" as const;
  async index(_d: Doc): Promise<void> {}
  async delete(_id: string): Promise<void> {}
  async query(_q: string, _limit: number): Promise<readonly Hit[]> {
    return [];
  }
}
