// ─── Memory entries and search ───

/** Unified memory entry (episodic | semantic | procedural). */
export interface MemoryEntry {
  id: string;
  entry_type: "episodic" | "semantic" | "procedural";
  key: string;
  value: string;
  created_at: string;
  expires_at: string | null;
  score: number | null;
}

/** FTS5 memory search result. */
export interface MemorySearchResult {
  id: string;
  entry_type: "episodic" | "semantic";
  content: string;
  score: number;
  relevance: number | null;
  created_at: string;
}
