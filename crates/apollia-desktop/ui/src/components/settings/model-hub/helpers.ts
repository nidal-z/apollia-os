/**
 * Model Hub pure helpers - formatting, GGUF grouping, and client-side filter
 * predicates. No IPC, no Svelte: kept side-effect free so the sub-components
 * stay thin and this logic is unit-testable in isolation.
 */
import { CheckCircle, AlertCircle, XCircle, type Icon } from "lucide-svelte";
import type {
  AcceleratorProfile,
  Compatibility,
  CompatIssue,
  DownloadProgress,
  HfFile,
  HfModelCard,
} from "$lib/ipc/modelHub";

const GB = 1_073_741_824;
const MB = 1_048_576;

export function formatSizeBytes(bytes: number): string {
  if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
  if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
  return `${bytes} B`;
}

export function formatBudget(gb: number): string {
  return `${gb.toFixed(1)} GB`;
}

export function formatSpeed(bps: number): string {
  if (bps > 1_000_000) return `${(bps / 1_000_000).toFixed(1)} MB/s`;
  if (bps > 1_000) return `${(bps / 1_000).toFixed(0)} KB/s`;
  return `${bps.toFixed(0)} B/s`;
}

export function formatProgress(p: DownloadProgress): string {
  if (!p.total_bytes) return `${(p.downloaded_bytes / GB).toFixed(2)} GB`;
  const pct = Math.round((p.downloaded_bytes / p.total_bytes) * 100);
  return `${pct}%`;
}

/** Percentage 0-100 for the progress bar; 0 while total is still unknown. */
export function progressPercent(p: DownloadProgress): number {
  if (!p.total_bytes) return 0;
  return Math.round((p.downloaded_bytes / p.total_bytes) * 100);
}

// ── Compatibility verdict ─────────────────────────────────────────────────

export function compatIcon(c: Compatibility): typeof Icon {
  if (c === "fits") return CheckCircle;
  if (c === "might_fit") return AlertCircle;
  return XCircle;
}

/** Token-only text colour for a compatibility verdict. */
export function compatClass(c: Compatibility): string {
  if (c === "fits") return "text-success-a11y";
  if (c === "might_fit") return "text-warning-a11y";
  if (c === "too_large") return "text-danger-a11y";
  return "text-muted-foreground";
}

// ── GGUF file grouping ────────────────────────────────────────────────────

const SPLIT_RE = /^(.*)-(\d{5})-of-(\d{5})\.gguf$/i;
const QUANT_RE = /[-.]([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)*)(?:-\d{5}-of-\d{5})?\.gguf$/i;

export interface GgufGroup {
  quantName: string;
  fullName: string;
  files: HfFile[];
  totalSizeHuman: string;
  totalSizeBytes: number;
  isSplit: boolean;
  compatibility: Compatibility;
}

export interface GroupedFiles {
  models: GgufGroup[];
  projectors: HfFile[];
}

export function extractQuantName(filename: string): string {
  const base = filename.split("/").pop() ?? filename;
  const m = base.match(QUANT_RE);
  return m ? m[1].toUpperCase() : base.replace(/\.gguf$/i, "");
}

export function groupGgufFiles(files: HfFile[]): GroupedFiles {
  const projectors = files.filter((f) =>
    f.filename.toLowerCase().includes("mmproj"),
  );
  const modelFiles = files.filter(
    (f) => !f.filename.toLowerCase().includes("mmproj"),
  );

  const buckets = new Map<string, HfFile[]>();
  for (const file of modelFiles) {
    const basename = file.filename.split("/").pop() ?? file.filename;
    const m = basename.match(SPLIT_RE);
    const key = m ? `split:${m[1]}__of${m[3]}` : `single:${file.filename}`;
    if (!buckets.has(key)) buckets.set(key, []);
    buckets.get(key)!.push(file);
  }

  const models: GgufGroup[] = [...buckets.values()].map((parts) => {
    const sorted = [...parts].sort((a, b) =>
      a.filename.localeCompare(b.filename),
    );
    const isSplit = sorted.length > 1;
    const totalBytes = sorted.reduce((s, f) => s + f.size_bytes, 0);
    const firstName = sorted[0].filename.split("/").pop() ?? sorted[0].filename;
    const m = firstName.match(SPLIT_RE);
    const fullName = isSplit ? `${m![1]}.gguf` : firstName;
    const quantName = extractQuantName(firstName);
    const avgPartBytes = Math.round(totalBytes / sorted.length);
    const totalSizeHuman = isSplit
      ? `${sorted.length} × ${formatSizeBytes(avgPartBytes)} · ${formatSizeBytes(totalBytes)} total`
      : formatSizeBytes(totalBytes);
    return {
      quantName,
      fullName,
      files: sorted,
      totalSizeHuman,
      totalSizeBytes: totalBytes,
      isSplit,
      compatibility: sorted[0].compatibility,
    };
  });

  models.sort((a, b) => a.totalSizeBytes - b.totalSizeBytes);
  return { models, projectors };
}

// ── Client-side filters ───────────────────────────────────────────────────

const OPEN_LICENSES = new Set([
  "apache-2.0", "mit", "bsd", "bsd-2-clause", "bsd-3-clause",
  "lgpl-2.0", "lgpl-2.1", "lgpl-3.0", "gpl-2.0", "gpl-3.0",
  "cc-by-4.0", "cc-by-sa-4.0", "cc0-1.0", "unlicense", "openrail",
  "openrail++", "artistic-2.0",
]);
const RESTRICTED_LICENSES = new Set([
  "cc-by-nc-4.0", "cc-by-nc-sa-4.0", "cc-by-nc-nd-4.0",
  "llama3", "llama3.1", "llama3.2", "llama3.3",
  "gemma", "gemma3", "deepseek", "qwen", "mistral",
]);

export type LicenseType = "open" | "restricted" | "unknown";
export type ModelCategory = "instruct" | "base" | "reasoning" | "unknown";

/** Client-side filter selections (the chip row). */
export type LicenseFilter = "any" | "open" | "restricted";
export type ModelTypeFilter = "any" | "instruct" | "base" | "reasoning";
export type GatedFilter = "any" | "open" | "gated";

export function licenseTypeOf(m: HfModelCard): LicenseType {
  const lic = (m.license ?? "").toLowerCase();
  if (lic && OPEN_LICENSES.has(lic)) return "open";
  if (lic && RESTRICTED_LICENSES.has(lic)) return "restricted";
  for (const tag of m.tags) {
    const t = tag.toLowerCase().replace(/^license:/, "");
    if (RESTRICTED_LICENSES.has(t)) return "restricted";
    if (OPEN_LICENSES.has(t)) return "open";
  }
  return "unknown";
}

export function modelCategoryOf(m: HfModelCard): ModelCategory {
  const text = [m.repo_id, ...m.tags].join(" ").toLowerCase();
  if (
    text.includes("think") ||
    text.includes("reason") ||
    text.includes("qwq") ||
    text.includes("r1")
  )
    return "reasoning";
  if (
    text.includes("instruct") ||
    text.includes("chat") ||
    text.includes("-it-") ||
    text.includes("-it.") ||
    text.endsWith("-it")
  )
    return "instruct";
  if (
    text.includes("-base") ||
    text.includes("base-") ||
    text.includes("pretrain")
  )
    return "base";
  return "unknown";
}

export function isBlockingIssue(issue: CompatIssue): boolean {
  return issue === "embedding_model" || issue === "no_gguf_files";
}

export function chipLabel(
  acc: AcceleratorProfile,
  cpuOnlyLabel: string,
): string {
  if (acc.kind === "apple_silicon") return acc.chip ?? "Apple Silicon";
  if (acc.kind === "cuda") return acc.device_name ?? "NVIDIA GPU";
  if (acc.kind === "generic") return acc.device_name ?? "GPU";
  return cpuOnlyLabel;
}

/** True when any active download is writing one of the group's files. */
export function groupIsDownloading(
  group: GgufGroup,
  downloads: DownloadProgress[],
): boolean {
  return group.files.some((f) => {
    const basename = f.filename.split("/").pop();
    return downloads.some(
      (d) =>
        basename !== undefined &&
        d.dest_path.endsWith(basename) &&
        d.status === "in_progress",
    );
  });
}
