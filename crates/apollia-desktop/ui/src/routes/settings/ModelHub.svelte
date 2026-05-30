<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, onDestroy } from "svelte";
  import { t } from "svelte-i18n";
  import {
    Cpu,
    Search,
    Download,
    X,
    XCircle,
    ChevronDown,
    ChevronUp,
    AlertTriangle,
    CheckCircle,
    AlertCircle,
    Info,
    ExternalLink,
    Key,
    Package,
    ShieldAlert,
    Eye,
    EyeOff,
    SlidersHorizontal,
  } from "lucide-svelte";
  import { addToast } from "$lib/components/ui/toast";
  import { Spinner } from "$lib/components/ui/progress";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Button } from "$lib/components/ui/button";

  // ──────────────────────────────────────────────
  // Types
  // ──────────────────────────────────────────────

  interface AcceleratorProfile {
    kind: "none" | "apple_silicon" | "cuda" | "generic";
    chip?: string;
    generation?: number;
    vram_gb?: number;
    device_name?: string;
    compute_capability?: [number, number];
  }

  interface HardwareProfile {
    total_ram_gb: number;
    available_ram_gb: number;
    cpu_model: string;
    cpu_cores: number;
    accelerator: AcceleratorProfile;
    memory_budget_gb: number;
  }

  interface HfFile {
    filename: string;
    size_bytes: number;
    size_human: string;
    compatibility: "fits" | "might_fit" | "too_large" | null;
    download_url: string;
  }

  interface GenerationConfig {
    temperature?: number;
    top_p?: number;
    top_k?: number;
    repetition_penalty?: number;
    max_new_tokens?: number;
  }

  type CompatIssue =
    | "embedding_model"
    | "unknown_architecture"
    | "no_gguf_files"
    | null;

  interface HfModelCard {
    repo_id: string;
    author: string | null;
    downloads: number;
    likes: number;
    license: string | null;
    tags: string[];
    gated: boolean;
    total_size_bytes: number | null;
    gguf_files: HfFile[];
    generation_config: GenerationConfig | null;
    compatibility_issue: CompatIssue;
    model_type: string | null;
  }

  interface DownloadProgress {
    id: string;
    downloaded_bytes: number;
    total_bytes: number | null;
    speed_bps: number;
    dest_path: string;
    status: "in_progress" | "completed" | "cancelled" | "failed";
  }

  // ──────────────────────────────────────────────
  // State
  // ──────────────────────────────────────────────

  let hardware = $state<HardwareProfile | null>(null);
  let hardwareLoading = $state(true);

  let searchQuery = $state("");
  let searchResults = $state<HfModelCard[]>([]);
  let searching = $state(false);
  let searchError = $state<string | null>(null);
  let nextCursor = $state<string | null>(null);
  let hasMore = $state(false);

  let expandedModel = $state<string | null>(null);
  let modelDetail = $state<HfModelCard | null>(null);
  let modelDetailLoading = $state(false);

  let activeDownloads = $state<Map<string, DownloadProgress>>(new Map());

  let hfToken = $state("");
  let showTokenForm = $state(false);

  // ── Filter state ────────────────────────────────────────────────────────────
  let sortBy = $state("downloads");
  let langFilter = $state("");
  let licenseFilter = $state<"any" | "open" | "restricted">("any");
  let modelTypeFilter = $state<"any" | "instruct" | "base" | "reasoning">("any");
  let gatedFilter = $state<"any" | "open" | "gated">("any");
  let showIncompatible = $state(false);

  let unlisten: (() => void) | null = null;

  // ──────────────────────────────────────────────
  // Lifecycle
  // ──────────────────────────────────────────────

  onMount(async () => {
    await loadHardware();
    await loadInitialModels();

    unlisten = await listen<DownloadProgress>("model-download-progress", (event) => {
      const p = event.payload;
      if (p.status === "completed" || p.status === "cancelled" || p.status === "failed") {
        const next = new Map(activeDownloads);
        next.delete(p.id);
        activeDownloads = next;
        if (p.status === "completed") {
          addToast($t("settings.model_hub.downloads.completed_toast", { values: { path: p.dest_path } }), "success");
        }
      } else {
        const next = new Map(activeDownloads);
        next.set(p.id, p);
        activeDownloads = next;
      }
    });
  });

  onDestroy(() => {
    unlisten?.();
  });

  async function loadHardware() {
    hardwareLoading = true;
    try {
      hardware = await invoke<HardwareProfile>("get_hardware_profile");
    } catch (e) {
      console.error("hardware detection failed", e);
    } finally {
      hardwareLoading = false;
    }
  }

  const PAGE_LIMIT = 50;

  interface SearchPage {
    models: HfModelCard[];
    next_cursor: string | null;
  }

  async function loadInitialModels() {
    await search();
  }

  async function search(append = false) {
    if (!append) {
      nextCursor = null;
    }
    searching = true;
    searchError = null;
    try {
      const result = await invoke<SearchPage>("search_hf_models", {
        params: {
          query: searchQuery,
          limit: PAGE_LIMIT,
          sort: sortBy,
          pipeline_tag: "text-generation",
          language: langFilter || null,
          // On passe le curseur HF pour la page suivante, null pour une nouvelle recherche.
          next_cursor: append ? nextCursor : null,
          hf_token: hfToken || null,
        },
      });
      if (append) {
        searchResults = [...searchResults, ...result.models];
      } else {
        searchResults = result.models;
      }
      nextCursor = result.next_cursor;
      hasMore = result.next_cursor !== null;
    } catch (e) {
      searchError = String(e);
    } finally {
      searching = false;
    }
  }

  async function loadMore() {
    await search(true);
  }

  async function expandModel(repoId: string) {
    if (expandedModel === repoId) {
      expandedModel = null;
      modelDetail = null;
      return;
    }
    expandedModel = repoId;
    modelDetail = null;
    modelDetailLoading = true;
    try {
      modelDetail = await invoke<HfModelCard>("get_hf_model", {
        repoId,
        hfToken: hfToken || null,
      });
    } catch (e) {
      addToast(String(e), "error");
      expandedModel = null;
    } finally {
      modelDetailLoading = false;
    }
  }

  async function startDownload(file: HfFile) {
    // Gated model check
    const card = modelDetail ?? searchResults.find((m) => m.repo_id === expandedModel);
    if (card?.gated && !hfToken) {
      showTokenForm = true;
      return;
    }
    try {
      const _id = await invoke<string>("start_model_download", {
        request: {
          url: file.download_url,
          filename: file.filename,
          hf_token: hfToken || null,
          repo_id: card?.repo_id ?? null,
        },
      });
      addToast($t("settings.model_hub.downloads.downloading_toast", { values: { filename: file.filename } }), "info");
    } catch (e) {
      addToast(String(e), "error");
    }
  }

  async function cancelDownload(id: string) {
    try {
      await invoke("cancel_model_download", { downloadId: id });
    } catch (e) {
      addToast(String(e), "error");
    }
  }

  // ──────────────────────────────────────────────
  // Helpers
  // ──────────────────────────────────────────────

  function formatBudget(gb: number): string {
    return `${gb.toFixed(1)} GB`;
  }

  // ── Compat badge ────────────────────────────────────────────────────────────

  type CompatIcon = typeof CheckCircle;

  function compatIcon(c: HfFile["compatibility"]): CompatIcon {
    if (c === "fits") return CheckCircle;
    if (c === "might_fit") return AlertCircle;
    return XCircle;
  }

  function compatClass(c: HfFile["compatibility"]): string {
    if (c === "fits") return "text-green-600 dark:text-green-400";
    if (c === "might_fit") return "text-amber-600 dark:text-amber-400";
    if (c === "too_large") return "text-destructive";
    return "text-muted-foreground";
  }

  function compatTooltip(c: HfFile["compatibility"]): string {
    if (c === "fits") return $t("settings.model_hub.detail.compat_fits");
    if (c === "might_fit") return $t("settings.model_hub.detail.compat_might_fit");
    if (c === "too_large") return $t("settings.model_hub.detail.compat_too_large");
    return "";
  }

  // ── GGUF file grouping ──────────────────────────────────────────────────────

  const SPLIT_RE = /^(.*)-(\d{5})-of-(\d{5})\.gguf$/i;
  // Matches the last quantization token (Q4_K_M, IQ2_XS, F16, BF16…)
  const QUANT_RE = /[-.]([A-Z][A-Z0-9]*(?:_[A-Z0-9]+)*)(?:-\d{5}-of-\d{5})?\.gguf$/i;

  interface GgufGroup {
    quantName: string;       // "Q4_K_M", "F16" — extracted quant label
    fullName: string;        // full base filename for tooltip
    files: HfFile[];
    totalSizeHuman: string;  // "3 × 44.0 GB · 132.0 GB total" or "17.5 GB"
    totalSizeBytes: number;
    isSplit: boolean;
    compatibility: HfFile["compatibility"];
  }

  interface GroupedFiles {
    models: GgufGroup[];
    projectors: HfFile[];    // mmproj-*.gguf — vision projection matrices
  }

  function formatSizeBytes(bytes: number): string {
    const GB = 1_073_741_824;
    const MB = 1_048_576;
    if (bytes >= GB) return `${(bytes / GB).toFixed(1)} GB`;
    if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
    return `${bytes} B`;
  }

  function extractQuantName(filename: string): string {
    const base = filename.split("/").pop() ?? filename;
    const m = base.match(QUANT_RE);
    return m ? m[1].toUpperCase() : base.replace(/\.gguf$/i, "");
  }

  function groupGgufFiles(files: HfFile[]): GroupedFiles {
    const projectors = files.filter((f) => f.filename.toLowerCase().includes("mmproj"));
    const modelFiles = files.filter((f) => !f.filename.toLowerCase().includes("mmproj"));

    const buckets = new Map<string, HfFile[]>();
    for (const file of modelFiles) {
      const basename = file.filename.split("/").pop() ?? file.filename;
      const m = basename.match(SPLIT_RE);
      const key = m ? `split:${m[1]}__of${m[3]}` : `single:${file.filename}`;
      if (!buckets.has(key)) buckets.set(key, []);
      buckets.get(key)!.push(file);
    }

    const models: GgufGroup[] = [...buckets.values()].map((parts) => {
      const sorted = [...parts].sort((a, b) => a.filename.localeCompare(b.filename));
      const isSplit = sorted.length > 1;
      const totalBytes = sorted.reduce((s, f) => s + f.size_bytes, 0);
      const firstName = sorted[0].filename.split("/").pop() ?? sorted[0].filename;
      const m = firstName.match(SPLIT_RE);
      const fullName = isSplit ? `${m![1]}.gguf` : firstName;
      const quantName = extractQuantName(firstName);
      const avgPartBytes = Math.round(totalBytes / sorted.length);
      const partSize = formatSizeBytes(avgPartBytes);
      const totalSize = formatSizeBytes(totalBytes);
      const totalSizeHuman = isSplit
        ? `${sorted.length} × ${partSize} · ${totalSize} total`
        : totalSize;
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

  async function downloadGroup(group: GgufGroup): Promise<void> {
    for (const file of group.files) {
      await startDownload(file);
    }
  }

  function formatSpeed(bps: number): string {
    if (bps > 1_000_000) return `${(bps / 1_000_000).toFixed(1)} MB/s`;
    if (bps > 1_000) return `${(bps / 1_000).toFixed(0)} KB/s`;
    return `${bps.toFixed(0)} B/s`;
  }

  function formatProgress(p: DownloadProgress): string {
    if (!p.total_bytes) return `${(p.downloaded_bytes / 1_073_741_824).toFixed(2)} GB`;
    const pct = Math.round((p.downloaded_bytes / p.total_bytes) * 100);
    return `${pct}%`;
  }

  // ── Client-side filter helpers ───────────────────────────────────────────────

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

  function licenseTypeOf(m: HfModelCard): "open" | "restricted" | "unknown" {
    const lic = (m.license ?? "").toLowerCase();
    if (lic && OPEN_LICENSES.has(lic)) return "open";
    if (lic && RESTRICTED_LICENSES.has(lic)) return "restricted";
    for (const tag of m.tags) {
      // HF tags use "license:apache-2.0" format — strip the prefix
      const t = tag.toLowerCase().replace(/^license:/, "");
      if (RESTRICTED_LICENSES.has(t)) return "restricted";
      if (OPEN_LICENSES.has(t)) return "open";
    }
    return "unknown";
  }

  function modelCategoryOf(m: HfModelCard): "instruct" | "base" | "reasoning" | "unknown" {
    const text = [m.repo_id, ...m.tags].join(" ").toLowerCase();
    if (text.includes("think") || text.includes("reason") || text.includes("qwq") || text.includes("r1")) return "reasoning";
    if (text.includes("instruct") || text.includes("chat") || text.includes("-it-") || text.includes("-it.") || text.endsWith("-it")) return "instruct";
    if (text.includes("-base") || text.includes("base-") || text.includes("pretrain")) return "base";
    return "unknown";
  }

  function isBlockingIssue(issue: CompatIssue): boolean {
    return issue === "embedding_model" || issue === "no_gguf_files";
  }

  const filteredResults = $derived(
    searchResults
      .filter((m) => showIncompatible || !isBlockingIssue(m.compatibility_issue))
      .filter((m) => licenseFilter === "any" || licenseTypeOf(m) === licenseFilter)
      .filter((m) => {
        if (modelTypeFilter === "any") return true;
        const cat = modelCategoryOf(m);
        // "Base" filter includes models with no strong instruct/reasoning signal
        if (modelTypeFilter === "base") return cat === "base" || cat === "unknown";
        return cat === modelTypeFilter;
      })
      .filter((m) =>
        gatedFilter === "any" ? true : gatedFilter === "open" ? !m.gated : m.gated
      )
  );

  function chipLabel(acc: AcceleratorProfile): string {
    if (acc.kind === "apple_silicon") return acc.chip ?? "Apple Silicon";
    if (acc.kind === "cuda") return acc.device_name ?? "NVIDIA GPU";
    if (acc.kind === "generic") return acc.device_name ?? "GPU";
    return $t("settings.model_hub.hardware.cpu_only");
  }
</script>

<div class="flex flex-col gap-6">

  <!-- ── Header ── -->
  <div>
    <h2 class="text-lg font-semibold text-foreground">{$t("settings.model_hub.title")}</h2>
    <p class="text-sm text-muted-foreground">{$t("settings.model_hub.subtitle")}</p>
  </div>

  <!-- ── Hardware profile ── -->
  <section class="rounded-xl border border-border bg-muted/40 p-4">
    <div class="mb-3 flex items-center gap-2">
      <Cpu class="h-4 w-4 text-muted-foreground" />
      <span class="text-sm font-medium text-foreground">{$t("settings.model_hub.hardware.label")}</span>
    </div>

    {#if hardwareLoading}
      <div class="flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner class="h-4 w-4" />
        {$t("settings.model_hub.hardware.detecting")}
      </div>
    {:else if hardware}
      <div class="grid grid-cols-2 gap-x-8 gap-y-2 text-sm sm:grid-cols-4">
        <div>
          <div class="text-muted-foreground">{$t("settings.model_hub.hardware.ram")}</div>
          <div class="font-medium text-foreground">{hardware.total_ram_gb.toFixed(0)} GB</div>
        </div>
        <div>
          <div class="text-muted-foreground">{$t("settings.model_hub.hardware.available")}</div>
          <div class="font-medium text-foreground">{hardware.available_ram_gb.toFixed(1)} GB</div>
        </div>
        <div>
          <div class="text-muted-foreground">{$t("settings.model_hub.hardware.gpu")}</div>
          <div class="font-medium text-foreground">{chipLabel(hardware.accelerator)}</div>
        </div>
        <div>
          <div class="text-muted-foreground">{$t("settings.model_hub.hardware.budget")}</div>
          <div class="font-medium text-green-600 dark:text-green-400">{formatBudget(hardware.memory_budget_gb)}</div>
        </div>
      </div>
      <div class="mt-2 text-xs text-muted-foreground/70">{$t("settings.model_hub.hardware.cpu_info", { values: { cpu: hardware.cpu_model, cores: hardware.cpu_cores } })}</div>
    {/if}
  </section>

  <!-- ── Active downloads ── -->
  {#if activeDownloads.size > 0}
    <section class="rounded-xl border border-primary/20 bg-primary/5 p-4">
      <div class="mb-3 flex items-center gap-2">
        <Download class="h-4 w-4 text-primary" />
        <span class="text-sm font-medium text-foreground">{$t("settings.model_hub.downloads.active")}</span>
      </div>
      <div class="flex flex-col gap-2">
        {#each [...activeDownloads.values()] as dl}
          <div class="flex items-center gap-3">
            <div class="flex-1">
              <div class="text-xs text-muted-foreground font-mono truncate">{dl.dest_path.split("/").pop()}</div>
              <div class="mt-1 h-1.5 w-full rounded-full bg-muted">
                <div
                  class="h-full rounded-full bg-primary transition-all"
                  style="width: {dl.total_bytes ? Math.round((dl.downloaded_bytes / dl.total_bytes) * 100) : 0}%"
                ></div>
              </div>
            </div>
            <div class="text-xs text-muted-foreground w-20 text-right">
              {formatProgress(dl)} · {formatSpeed(dl.speed_bps)}
            </div>
            <Button variant="ghost" size="sm"
              onclick={() => cancelDownload(dl.id)}
              class="rounded p-1 text-muted-foreground hover:text-destructive transition-colors"
              aria-label={$t("settings.model_hub.downloads.cancel_aria")}
            >
              <X class="h-3 w-3" />
            </Button>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- ── HF Token (optional) ── -->
  {#if showTokenForm}
    <section class="rounded-xl border border-amber-500/30 bg-amber-500/5 p-4">
      <div class="mb-3 flex items-center gap-2">
        <Key class="h-4 w-4 text-amber-600 dark:text-amber-400" />
        <span class="text-sm font-medium text-foreground">{$t("settings.model_hub.token.label")}</span>
      </div>
      <p class="mb-3 text-xs text-muted-foreground">
        {$t("settings.model_hub.token.description")}
        <a
          href="https://huggingface.co/settings/tokens"
          target="_blank"
          rel="noopener noreferrer"
          class="text-primary hover:underline inline-flex items-center gap-1"
        >
          huggingface.co/settings/tokens <ExternalLink class="h-3 w-3" />
        </a>
      </p>
      <div class="flex gap-2">
        <Input
          type="password"
          bind:value={hfToken}
          placeholder="hf_••••••••••••••••"
          class="flex h-9 flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-sm text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
         />
        <Button variant="ghost" size="sm"
          onclick={() => (showTokenForm = false)}
          class="rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
        >
          {$t("settings.model_hub.token.done")}
        </Button>
      </div>
    </section>
  {/if}

  <!-- ── Search ── -->
  <section>
    <div class="flex gap-2">
      <div class="relative flex-1">
        <Search class="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          type="text"
          bind:value={searchQuery}
          onkeydown={(e) => e.key === "Enter" && search()}
          placeholder={$t("settings.model_hub.search.placeholder")}
          class="flex h-9 w-full rounded-md border border-border bg-background py-2 pl-9 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/40"
        />
      </div>
      <Button variant="ghost" size="sm"
        onclick={() => search()}
        disabled={searching}
        class="flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors"
      >
        {#if searching}
          <Spinner class="h-4 w-4" />
        {:else}
          <Search class="h-4 w-4" />
        {/if}
        {$t("settings.model_hub.search.button")}
      </Button>
      <Button variant="ghost" size="sm"
        onclick={() => (showTokenForm = !showTokenForm)}
        class="flex items-center gap-1 rounded-md border border-border px-3 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
        title={$t("settings.model_hub.token.title_button")}
      >
        <Key class="h-4 w-4" />
        {hfToken ? $t("settings.model_hub.token.set") : $t("settings.model_hub.token.btn")}
      </Button>
    </div>

    {#if searchError}
      <div class="mt-3 flex items-center gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-sm text-destructive">
        <AlertTriangle class="h-4 w-4 shrink-0" />
        {searchError}
      </div>
    {/if}

    <!-- ── Filter bar ── -->
    <div class="mt-3 flex flex-wrap items-center gap-2">
      <SlidersHorizontal class="h-3.5 w-3.5 text-muted-foreground shrink-0" />

      <!-- Sort (server-side) -->
      <Select
        bind:value={sortBy}
        onchange={() => search()}
        class="h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring/40"
      >
        <option value="downloads">{$t("settings.model_hub.filters.sort_downloads")}</option>
        <option value="likes">{$t("settings.model_hub.filters.sort_likes")}</option>
        <option value="trending">{$t("settings.model_hub.filters.sort_trending")}</option>
        <option value="createdAt">{$t("settings.model_hub.filters.sort_newest")}</option>
      </Select>

      <!-- Language (server-side) -->
      <Select
        bind:value={langFilter}
        onchange={() => search()}
        class="h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring/40"
      >
        <option value="">{$t("settings.model_hub.filters.lang_any")}</option>
        <option value="en">{$t("settings.model_hub.filters.lang_en")}</option>
        <option value="fr">{$t("settings.model_hub.filters.lang_fr")}</option>
        <option value="zh">{$t("settings.model_hub.filters.lang_zh")}</option>
        <option value="de">{$t("settings.model_hub.filters.lang_de")}</option>
        <option value="es">{$t("settings.model_hub.filters.lang_es")}</option>
        <option value="ja">{$t("settings.model_hub.filters.lang_ja")}</option>
        <option value="ko">{$t("settings.model_hub.filters.lang_ko")}</option>
        <option value="ar">{$t("settings.model_hub.filters.lang_ar")}</option>
        <option value="ru">{$t("settings.model_hub.filters.lang_ru")}</option>
        <option value="pt">{$t("settings.model_hub.filters.lang_pt")}</option>
      </Select>

      <!-- License (client-side) -->
      <Select
        bind:value={licenseFilter}
        class="h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring/40"
      >
        <option value="any">{$t("settings.model_hub.filters.license_any")}</option>
        <option value="open">{$t("settings.model_hub.filters.license_open")}</option>
        <option value="restricted">{$t("settings.model_hub.filters.license_restricted")}</option>
      </Select>

      <!-- Model type (client-side) -->
      <Select
        bind:value={modelTypeFilter}
        class="h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring/40"
      >
        <option value="any">{$t("settings.model_hub.filters.type_any")}</option>
        <option value="instruct">{$t("settings.model_hub.filters.type_instruct")}</option>
        <option value="base">{$t("settings.model_hub.filters.type_base")}</option>
        <option value="reasoning">{$t("settings.model_hub.filters.type_reasoning")}</option>
      </Select>

      <!-- Gated (client-side) -->
      <Select
        bind:value={gatedFilter}
        class="h-7 rounded-md border border-border bg-background px-2 text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-ring/40"
      >
        <option value="any">{$t("settings.model_hub.filters.gated_any")}</option>
        <option value="open">{$t("settings.model_hub.filters.gated_open_only")}</option>
        <option value="gated">{$t("settings.model_hub.filters.gated_only")}</option>
      </Select>

      <!-- Show incompatible toggle (Option C: hidden by default) -->
      <Button variant="ghost" size="sm"
        onclick={() => (showIncompatible = !showIncompatible)}
        class="flex h-7 items-center gap-1.5 rounded-md border px-2 text-xs transition-colors
          {showIncompatible
            ? 'border-amber-500/40 bg-amber-500/10 text-amber-700 dark:text-amber-400'
            : 'border-border text-muted-foreground hover:text-foreground hover:bg-muted/60'}"
        title={showIncompatible ? $t("settings.model_hub.filters.hide_incompatible_title") : $t("settings.model_hub.filters.show_incompatible_title")}
      >
        {#if showIncompatible}
          <Eye class="h-3 w-3" />
        {:else}
          <EyeOff class="h-3 w-3" />
        {/if}
        {$t("settings.model_hub.filters.incompatible")}
      </Button>

      <!-- Result count -->
      {#if searchResults.length > 0}
        <span class="ml-auto text-xs text-muted-foreground">
          {$t("settings.model_hub.filters.result_count", { values: { filtered: filteredResults.length, total: searchResults.length } })}
        </span>
      {/if}
    </div>
  </section>

  <!-- ── Results ── -->
  {#if filteredResults.length > 0}
    <div class="flex flex-col gap-2">
      {#each filteredResults as model}
        {@const isExpanded = expandedModel === model.repo_id}
        <div class="rounded-lg border border-border bg-background overflow-hidden">
          <!-- Model header (always visible) -->
          <Button variant="ghost" size="sm"
            onclick={() => expandModel(model.repo_id)}
            class="flex w-full items-center gap-3 p-4 text-left hover:bg-muted/40 transition-colors"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-sm font-medium text-foreground truncate">{model.repo_id}</span>
                {#if model.gated}
                  <span class="rounded px-1.5 py-0.5 text-xs bg-amber-500/10 text-amber-700 dark:text-amber-400 flex items-center gap-1">
                    <Key class="h-3 w-3" /> {$t("settings.model_hub.card.gated")}
                  </span>
                {/if}
                {#if model.compatibility_issue === "embedding_model"}
                  <span class="rounded px-1.5 py-0.5 text-xs bg-destructive/10 text-destructive flex items-center gap-1" title={$t("settings.model_hub.card.embedding_only_title")}>
                    <ShieldAlert class="h-3 w-3" /> {$t("settings.model_hub.card.embedding_only")}
                  </span>
                {:else if model.compatibility_issue === "no_gguf_files"}
                  <span class="rounded px-1.5 py-0.5 text-xs bg-destructive/10 text-destructive flex items-center gap-1" title={$t("settings.model_hub.card.no_gguf_title")}>
                    <ShieldAlert class="h-3 w-3" /> {$t("settings.model_hub.card.no_gguf")}
                  </span>
                {:else if model.compatibility_issue === "unknown_architecture"}
                  <span class="rounded px-1.5 py-0.5 text-xs bg-amber-500/10 text-amber-700 dark:text-amber-400 flex items-center gap-1" title={$t("settings.model_hub.card.arch_unknown_title")}>
                    <ShieldAlert class="h-3 w-3" /> {$t("settings.model_hub.card.arch_unknown")}
                  </span>
                {/if}
                {#if model.license}
                  <span class="rounded px-1.5 py-0.5 text-xs bg-muted text-muted-foreground">{model.license}</span>
                {/if}
              </div>
              <div class="mt-1 flex items-center gap-4 text-xs text-muted-foreground">
                <span>{$t("settings.model_hub.card.downloads_count", { values: { count: (model.downloads / 1000).toFixed(0) } })}</span>
                {#if model.gguf_files.length > 0}
                  <span>{$t("settings.model_hub.card.gguf_files_count", { values: { count: model.gguf_files.length } })}</span>
                {/if}
              </div>
            </div>
            <div class="text-muted-foreground shrink-0">
              {#if isExpanded}
                <ChevronUp class="h-4 w-4" />
              {:else}
                <ChevronDown class="h-4 w-4" />
              {/if}
            </div>
          </Button>

          <!-- Expanded detail -->
          {#if isExpanded}
            <div class="border-t border-border p-4">
              {#if modelDetailLoading}
                <div class="flex items-center gap-2 text-sm text-muted-foreground">
                  <Spinner class="h-4 w-4" />
                  {$t("settings.model_hub.detail.loading_files")}
                </div>
              {:else if modelDetail && modelDetail.repo_id === model.repo_id}
                <!-- Generation config -->
                {#if modelDetail.generation_config}
                  {@const cfg = modelDetail.generation_config}
                  <div class="mb-4 rounded-lg bg-muted/40 p-3">
                    <div class="mb-2 text-xs font-medium text-muted-foreground">{$t("settings.model_hub.detail.recommended_params")}</div>
                    <div class="grid grid-cols-3 gap-2 sm:grid-cols-5 text-xs">
                      {#if cfg.temperature !== undefined}
                        <div>
                          <div class="text-muted-foreground">{$t("settings.model_hub.detail.param_temperature")}</div>
                          <div class="text-foreground font-mono">{cfg.temperature}</div>
                        </div>
                      {/if}
                      {#if cfg.top_p !== undefined}
                        <div>
                          <div class="text-muted-foreground">{$t("settings.model_hub.detail.param_top_p")}</div>
                          <div class="text-foreground font-mono">{cfg.top_p}</div>
                        </div>
                      {/if}
                      {#if cfg.top_k !== undefined}
                        <div>
                          <div class="text-muted-foreground">{$t("settings.model_hub.detail.param_top_k")}</div>
                          <div class="text-foreground font-mono">{cfg.top_k}</div>
                        </div>
                      {/if}
                      {#if cfg.repetition_penalty !== undefined}
                        <div>
                          <div class="text-muted-foreground">{$t("settings.model_hub.detail.param_rep_penalty")}</div>
                          <div class="text-foreground font-mono">{cfg.repetition_penalty}</div>
                        </div>
                      {/if}
                      {#if cfg.max_new_tokens !== undefined}
                        <div>
                          <div class="text-muted-foreground">{$t("settings.model_hub.detail.param_max_tokens")}</div>
                          <div class="text-foreground font-mono">{cfg.max_new_tokens}</div>
                        </div>
                      {/if}
                    </div>
                  </div>
                {/if}

                <!-- GGUF files (grouped) -->
                {#if modelDetail.gguf_files.length > 0}
                  {@const grouped = groupGgufFiles(modelDetail.gguf_files)}
                  {#if grouped.models.length > 0}
                    <div class="text-xs font-medium text-muted-foreground mb-2">
                      {grouped.models.length === 1
                        ? $t("settings.model_hub.detail.variants_one", { values: { count: grouped.models.length } })
                        : $t("settings.model_hub.detail.variants_other", { values: { count: grouped.models.length } })}
                    </div>
                    <div class="flex flex-col gap-1.5">
                      {#each grouped.models as group}
                        {@const isDownloading = group.files.some((f) =>
                          [...activeDownloads.values()].some(
                            (d) => d.dest_path.endsWith(f.filename.split("/").pop()!) && d.status === "in_progress"
                          )
                        )}
                        {@const Icon = group.compatibility ? compatIcon(group.compatibility) : null}
                        <div class="flex items-center gap-3 rounded-md bg-muted/40 px-3 py-2.5">
                          <div class="flex-1 min-w-0">
                            <div class="flex items-center gap-1.5">
                              <span
                                class="text-xs font-mono font-medium text-foreground truncate"
                                title={group.fullName}
                              >{group.quantName}</span>
                              {#if group.isSplit}
                                <span class="inline-flex items-center gap-0.5 rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground shrink-0">
                                  <Package class="h-2.5 w-2.5" />
                                  {$t("settings.model_hub.detail.parts", { values: { count: group.files.length } })}
                                </span>
                              {/if}
                            </div>
                            <div class="text-xs text-muted-foreground mt-0.5 font-mono">{group.totalSizeHuman}</div>
                          </div>
                          {#if group.compatibility && Icon}
                            <span title={compatTooltip(group.compatibility)} class={compatClass(group.compatibility)}>
                              <Icon class="h-4 w-4" />
                            </span>
                          {/if}
                          <Button variant="ghost" size="sm"
                            onclick={() => downloadGroup(group)}
                            disabled={isDownloading}
                            class="flex shrink-0 items-center gap-1 rounded-md bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors"
                          >
                            {#if isDownloading}
                              <Spinner class="h-3 w-3" />
                            {:else}
                              <Download class="h-3 w-3" />
                            {/if}
                            {group.isSplit ? $t("settings.model_hub.detail.files_count", { values: { count: group.files.length } }) : $t("settings.model_hub.detail.download")}
                          </Button>
                        </div>
                      {/each}
                    </div>
                  {/if}

                  <!-- Vision projector files (mmproj) -->
                  {#if grouped.projectors.length > 0}
                    <div class="mt-3 rounded-md border border-dashed border-border/60 p-2.5">
                      <div class="mb-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                        <Info class="h-3 w-3 shrink-0" />
                        {$t("settings.model_hub.detail.vision_projector")}
                      </div>
                      {#each grouped.projectors as f}
                        <div class="flex items-center gap-2 text-xs">
                          <span class="flex-1 truncate font-mono text-foreground">{f.filename.split("/").pop()}</span>
                          <span class="text-muted-foreground">{f.size_human || "—"}</span>
                          <Button variant="ghost" size="sm"
                            onclick={() => startDownload(f)}
                            class="flex items-center gap-1 rounded bg-muted px-2 py-0.5 text-muted-foreground hover:text-foreground hover:bg-muted/80 transition-colors"
                          >
                            <Download class="h-3 w-3" /> {$t("settings.model_hub.detail.download")}
                          </Button>
                        </div>
                      {/each}
                    </div>
                  {/if}
                {:else}
                  <div class="text-sm text-muted-foreground">{$t("settings.model_hub.detail.no_gguf_in_repo")}</div>
                {/if}
              {/if}
            </div>
          {/if}
        </div>
      {/each}

      <!-- Load more -->
      {#if hasMore}
        <div class="flex justify-center pt-2">
          <button
            onclick={loadMore}
            disabled={searching}
            class="flex items-center gap-2 rounded-md border border-border px-5 py-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted/60 disabled:opacity-50 transition-colors"
          >
            {#if searching}
              <Spinner class="h-4 w-4" />
              {$t("settings.model_hub.results.loading")}
            {:else}
              {$t("settings.model_hub.results.load_more")}
            {/if}
          </button>
        </div>
      {/if}
    </div>
  {:else if !searching && searchResults.length > 0}
    <div class="rounded-lg border border-border bg-muted/40 p-8 text-center text-sm text-muted-foreground">
      {$t("settings.model_hub.results.no_match")}
      <Button variant="ghost" size="sm" onclick={() => { licenseFilter = "any"; modelTypeFilter = "any"; gatedFilter = "any"; showIncompatible = true; }}
        class="ml-2 text-primary hover:underline">{$t("settings.model_hub.results.clear_filters")}</Button>
    </div>
  {:else if !searching && searchQuery}
    <div class="rounded-lg border border-border bg-muted/40 p-8 text-center text-sm text-muted-foreground">
      {$t("settings.model_hub.results.no_results", { values: { query: searchQuery } })}
    </div>
  {/if}
</div>
