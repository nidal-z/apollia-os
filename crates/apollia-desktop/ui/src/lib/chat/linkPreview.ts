/**
 * Rich link preview client (US-SP42-035, B.46).
 *
 * Delegates the actual HTTP fetch to the Rust `link_preview` Tauri command
 * (keeps us CORS-free + respects the local-first principle), and layers an
 * in-memory short-TTL cache to coalesce repeat requests within a session.
 */
import { invoke } from "@tauri-apps/api/core";

export interface LinkPreview {
  url: string;
  title: string | null;
  description: string | null;
  favicon: string | null;
  image: string | null;
  site_name: string | null;
}

interface CacheEntry {
  value: LinkPreview | null;
  fetchedAt: number;
}

const MEMORY_TTL_MS = 15 * 60_000;
const MAX_MEMORY_ENTRIES = 200;

const memoryCache = new Map<string, CacheEntry>();
const inflight = new Map<string, Promise<LinkPreview | null>>();

function pruneCache(now: number): void {
  for (const [key, entry] of memoryCache) {
    if (now - entry.fetchedAt > MEMORY_TTL_MS) {
      memoryCache.delete(key);
    }
  }
  while (memoryCache.size > MAX_MEMORY_ENTRIES) {
    const oldest = memoryCache.keys().next().value;
    if (oldest === undefined) break;
    memoryCache.delete(oldest);
  }
}

/**
 * Fetch (and cache) the Open Graph metadata for `url`.
 *
 * Returns `null` when the backend returns an error (preview disabled, SSRF
 * refusal, network error) — callers should then fall back to rendering the
 * bare link.
 */
export async function fetchLinkPreview(url: string): Promise<LinkPreview | null> {
  const now = Date.now();
  const cached = memoryCache.get(url);
  if (cached !== undefined && now - cached.fetchedAt <= MEMORY_TTL_MS) {
    return cached.value;
  }
  const existing = inflight.get(url);
  if (existing !== undefined) return existing;

  const promise = (async () => {
    try {
      const preview = await invoke<LinkPreview>("link_preview", { url });
      memoryCache.set(url, { value: preview, fetchedAt: Date.now() });
      pruneCache(Date.now());
      return preview;
    } catch {
      memoryCache.set(url, { value: null, fetchedAt: Date.now() });
      return null;
    } finally {
      inflight.delete(url);
    }
  })();

  inflight.set(url, promise);
  return promise;
}

/**
 * Identify URLs that appear standalone (one URL per non-empty line, no
 * surrounding prose).  Only these trigger a rich preview card — inline links
 * stay bare to avoid cluttering the flow.
 */
export function detectStandaloneLinks(content: string): string[] {
  if (!content) return [];
  const urls: string[] = [];
  const seen = new Set<string>();
  for (const rawLine of content.split("\n")) {
    const line = rawLine.trim();
    if (!/^https?:\/\/\S+$/.test(line)) continue;
    if (seen.has(line)) continue;
    seen.add(line);
    urls.push(line);
  }
  return urls;
}
