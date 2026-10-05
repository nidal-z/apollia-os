<script lang="ts" module>
  import type { EngineChannel, EngineEntry, EnginesResponse } from "$lib/ipc/system";

  /** Where an engine stands against the channel it follows. */
  export type EngineUpdate =
    | { kind: "latest" }
    /** A newer build exists; `tag` names it. */
    | { kind: "available"; tag: string };

  /** What an engine is for: the kinds the type filter offers. */
  export type EngineType = "inference" | "dictation" | "tools";

  /** The i18n key of each type's name. */
  export const ENGINE_TYPE_LABEL_KEYS: Record<EngineType, string> = {
    inference: "settings.engines.type_inference",
    dictation: "settings.engines.type_dictation",
    tools: "settings.engines.type_tools",
  };

  /** What the three filters of the list hold. */
  export interface EngineFilters {
    /** Free text, matched against the name, the variant and the version. */
    query: string;
    /** True to keep only the engines this machine can run: the default. */
    compatibleOnly: boolean;
    /** One type, or `"all"`. */
    type: EngineType | "all";
  }

  /** The filters a page opens on. */
  export const DEFAULT_ENGINE_FILTERS: EngineFilters = {
    query: "",
    compatibleOnly: true,
    type: "all",
  };

  /** What a row of the engine list says: which engine, built for what, at which version. */
  export interface EngineRow {
    id: string;
    /** What the engine is for. */
    type: EngineType;
    /** False when this machine has not the hardware the build is made for. */
    compatible: boolean;
    /** Upstream project name, a proper noun, never translated. */
    name: string;
    /** `metal`, `cuda`, `vulkan`, `cpu`, or `null` when the build does not say. */
    variant: EngineEntry["variant"];
    /** What the engine is for, as an i18n key. */
    roleKey: string;
    /** The version the engine states at its source; `null` when it says nothing. */
    version: string | null;
    /** Embedded, downloaded or imported; `null` for the dictation engine. */
    source: EngineEntry["source"] | null;
    /** The tag the channel published this build under; `null` when embedded. */
    tag: string | null;
    /** True when the stated version is not the one the tag names. */
    tagMismatch: boolean;
    /** True for the build the runtime starts. */
    inService: boolean;
    /** What the followed channel says of this build; `null` when it says nothing. */
    update: EngineUpdate | null;
  }

  /** The i18n key of each source. */
  export const ENGINE_SOURCE_KEYS: Record<EngineEntry["source"], string> = {
    embedded: "settings.engines.source_embedded",
    downloaded: "settings.engines.source_downloaded",
    imported: "settings.engines.source_imported",
  };

  /**
   * Whether the version an engine states is the one its tag names.
   *
   * A tag carries the upstream prefix (`b7001`, `v0.9.2`) and a banner may
   * carry a commit after the number (`7001 (0ff1ce0)`): the leading number of
   * each is compared, prefix dropped.
   */
  export function versionMatchesTag(version: string, tag: string): boolean {
    const core = (text: string): string =>
      (text.trim().split(/[\s(]/)[0] ?? "").replace(/^[bv]/i, "");
    return core(version) === core(tag);
  }

  /** What the verified channel says of the build `e`. */
  export function engineUpdate(e: EngineEntry, response: EnginesResponse): EngineUpdate | null {
    if (e.tag === null) return null;
    const known = response.updates.find(
      (u) => u.engine === e.engine && u.variant === e.variant && u.installed_tag === e.tag,
    );
    if (!known) return null;
    return known.update_available
      ? { kind: "available", tag: known.latest_tag }
      : { kind: "latest" };
  }

  const ENGINE_NAMES: Record<EngineEntry["engine"], string> = {
    "llama-server": "llama.cpp",
    "vllm-mlx": "vllm-mlx",
    nodejs: "Node.js",
  };

  const ENGINE_TYPES: Record<EngineEntry["engine"], EngineType> = {
    "llama-server": "inference",
    "vllm-mlx": "inference",
    nodejs: "tools",
  };

  /**
   * Whether this machine can run the build `e`, given the `accelerator.kind`
   * of the hardware probe. The engine shipped with the application is made
   * for this machine. A build of the store needs what its variant names: an
   * NVIDIA card for CUDA, an Apple chip for Metal and for the MLX engine, a
   * graphics card for Vulkan. An unknown machine excludes nothing.
   */
  export function engineCompatible(e: EngineEntry, acceleratorKind: string | null): boolean {
    if (acceleratorKind === null || e.source === "embedded") return true;
    if (e.engine === "vllm-mlx") return acceleratorKind === "apple_silicon";
    switch (e.variant) {
      case "cuda":
        return acceleratorKind === "cuda";
      case "metal":
        return acceleratorKind === "apple_silicon";
      case "vulkan":
        return acceleratorKind === "cuda" || acceleratorKind === "generic";
      default:
        return true;
    }
  }

  const ENGINE_ROLES: Record<EngineEntry["engine"], string> = {
    "llama-server": "settings.engines.role_gguf",
    "vllm-mlx": "settings.engines.role_mlx",
    nodejs: "settings.engines.role_node",
  };

  /**
   * One row per engine of the answer, in its order, then the dictation engine.
   * The version is the one the runtime read from the engine itself; nothing
   * here supplies one.
   */
  export function engineRows(
    response: EnginesResponse | null,
    acceleratorKind: string | null = null,
  ): EngineRow[] {
    if (!response) return [];
    const rows: EngineRow[] = response.engines.map((e, i) => ({
      id: `${e.engine}-${e.variant ?? "any"}-${e.tag ?? e.source}-${i}`,
      type: ENGINE_TYPES[e.engine],
      compatible: engineCompatible(e, acceleratorKind),
      name: ENGINE_NAMES[e.engine],
      variant: e.variant,
      roleKey: ENGINE_ROLES[e.engine],
      version: e.version,
      source: e.source,
      tag: e.tag,
      tagMismatch:
        e.tag !== null && e.version !== null && !versionMatchesTag(e.version, e.tag),
      inService: e.in_service,
      update: engineUpdate(e, response),
    }));
    if (response.dictation) {
      rows.push({
        id: "whisper",
        type: "dictation",
        compatible: true,
        name: "whisper.cpp",
        variant: null,
        roleKey: "settings.engines.role_dictation",
        version: response.dictation.version,
        source: null,
        tag: null,
        tagMismatch: false,
        inService: false,
        update: null,
      });
    }
    return rows;
  }

  /** Display name of a variant: a backend name, or the processor. */
  export function variantLabel(variant: EngineEntry["variant"], cpuLabel: string): string | null {
    switch (variant) {
      case "metal":
        return "Metal";
      case "cuda":
        return "CUDA";
      case "vulkan":
        return "Vulkan";
      case "cpu":
        return cpuLabel;
      default:
        return null;
    }
  }

  /** The types `rows` carry, in the order the filter lists them. */
  export function presentEngineTypes(rows: EngineRow[]): EngineType[] {
    const order: EngineType[] = ["inference", "dictation", "tools"];
    return order.filter((type) => rows.some((row) => row.type === type));
  }

  /**
   * The rows the filters keep: those this machine can run unless every engine
   * is asked for, of the type asked for, whose name, variant or version
   * carries the text searched, case ignored.
   */
  export function filterEngineRows(rows: EngineRow[], filters: EngineFilters): EngineRow[] {
    const query = filters.query.trim().toLowerCase();
    return rows.filter((row) => {
      if (filters.compatibleOnly && !row.compatible) return false;
      if (filters.type !== "all" && row.type !== filters.type) return false;
      if (query === "") return true;
      const text = [row.name, row.variant ?? "", row.version ?? "", row.tag ?? ""]
        .join(" ")
        .toLowerCase();
      return text.includes(query);
    });
  }
</script>

<script lang="ts">
  /**
   * EngineList - System > Engines: every engine of this machine, embedded or
   * installed by the engine store, with the version it states at its source,
   * the one in service, where each stands against the followed channel, and
   * the dictation engine. One table, one source: the engine list of the
   * runtime. The developer mode adds the channel, the source of each engine
   * and the tag the channel published it under. Three filters narrow the
   * table: a search, the engines this machine can run or all of them, and
   * the type of engine. The diagnostic copy composes the same facts, filters
   * aside.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Copy, Check, Activity, RefreshCw, Search } from "lucide-svelte";
  import { Input } from "$lib/components/ui/input";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Select } from "$lib/components/ui/select";
  import SettingsFieldRow from "../SettingsFieldRow.svelte";
  import {
    ENGINE_CHANNELS,
    ENGINE_CHANNEL_LABEL_KEYS,
    isEngineChannel,
  } from "$lib/settings/engineChannel";
  import { Spinner } from "$lib/components/ui/progress";
  import { ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    response: EnginesResponse | null;
    loading: boolean;
    error: string | null;
    /** `accelerator.kind` of the hardware probe; `null` until it answers. */
    acceleratorKind?: string | null;
    /** The filters the list opens on. */
    initialFilters?: EngineFilters;
    /** Lines of the diagnostic block that precede the engines. */
    diagnosticHead: string[];
    /** Developer mode: the channel, the sources and the channel tags are shown. */
    developer: boolean;
    /** True while the channel is written or checked. */
    checking: boolean;
    /** What the last check or channel change refused, or `null`. */
    checkError: string | null;
    onchannel: (channel: EngineChannel) => void;
    oncheck: () => void;
  }

  let {
    response,
    loading,
    error,
    acceleratorKind = null,
    initialFilters = DEFAULT_ENGINE_FILTERS,
    diagnosticHead,
    developer,
    checking,
    checkError,
    onchannel,
    oncheck,
  }: Props = $props();

  const rows = $derived(engineRows(response, acceleratorKind));

  // The filters belong to this visit of the page: they are not a setting.
  // svelte-ignore state_referenced_locally
  let query = $state(initialFilters.query);
  // svelte-ignore state_referenced_locally
  let compatibleOnly = $state(initialFilters.compatibleOnly);
  // svelte-ignore state_referenced_locally
  let type = $state<EngineType | "all">(initialFilters.type);

  const types = $derived(presentEngineTypes(rows));
  const shown = $derived(filterEngineRows(rows, { query, compatibleOnly, type }));

  function label(row: EngineRow): string {
    const variant = variantLabel(row.variant, $t("settings.engines.variant_cpu"));
    return variant ? `${row.name} · ${variant}` : row.name;
  }

  const diagnostic = $derived(
    [
      ...diagnosticHead,
      ...rows.map(
        (r) => `${label(r)}: ${r.version ?? $t("settings.engines.version_unknown")}`,
      ),
    ].join("\n"),
  );

  let copied = $state(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;

  async function copyDiagnostic(): Promise<void> {
    try {
      await navigator.clipboard.writeText(diagnostic);
      copied = true;
      addToast($t("settings.about.copy_success"), "success");
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 1500);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }
</script>

{#if developer && response}
  <SettingsSection
    title={$t("settings.engines.channel_title")}
    bodyLayout="flush"
    data-testid="engines-channel"
  >
    <SettingsFieldRow
      label={$t("settings.engines.channel_label")}
      hint={$t("settings.engines.channel_hint")}
      for="engines-channel-select"
      border={false}
    >
      {#snippet control()}
        <Select
          id="engines-channel-select"
          size="sm"
          value={response.channel}
          disabled={checking}
          onchange={(e) => {
            const chosen = e.currentTarget.value;
            if (isEngineChannel(chosen)) onchannel(chosen);
          }}
          aria-label={$t("settings.engines.channel_label")}
          data-testid="engines-channel-select"
        >
          {#each ENGINE_CHANNELS as channel (channel)}
            <option value={channel}>{$t(ENGINE_CHANNEL_LABEL_KEYS[channel])}</option>
          {/each}
        </Select>
      {/snippet}
    </SettingsFieldRow>
  </SettingsSection>
{/if}

<SettingsSection
  title={$t("settings.engines.list_title")}
  description={$t("settings.engines.list_desc")}
  bodyLayout="flush"
  data-testid="engines-list"
>
  {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    <Button
      variant="outline"
      size="sm"
      loading={checking}
      disabled={!response}
      onclick={oncheck}
      data-testid="engines-check-updates"
    >
      {#snippet icon()}<RefreshCw size={13} />{/snippet}
      {$t("settings.engines.check_updates")}
    </Button>
  {/snippet}

  {#if checkError}
    <ErrorBanner message={checkError} data-testid="engines-check-error" />
  {/if}

  {#if loading && !response}
    <div class="flex items-center gap-2 py-3 text-body-sm text-muted-foreground">
      <Spinner class="h-4 w-4" />
      {$t("settings.engines.loading")}
    </div>
  {:else if error && !response}
    <ErrorBanner message={error} data-testid="engines-list-error" />
  {:else if rows.length === 0}
    <p class="py-3 text-body-sm text-muted-foreground" data-testid="engines-list-empty">
      {$t("settings.engines.empty")}
    </p>
  {:else}
    <div class="flex flex-wrap items-center gap-2 pb-2" data-testid="engines-filters">
      <div class="min-w-0 flex-1">
        <Input
          size="sm"
          icon={Search}
          bind:value={query}
          placeholder={$t("settings.engines.search_placeholder")}
          aria-label={$t("settings.engines.search_placeholder")}
          data-testid="engines-search"
        />
      </div>
      <Select
        size="sm"
        value={compatibleOnly ? "compatible" : "all"}
        onchange={(e) => (compatibleOnly = e.currentTarget.value !== "all")}
        aria-label={$t("settings.engines.filter_compat_label")}
        data-testid="engines-filter-compat"
      >
        <option value="compatible">{$t("settings.engines.filter_compatible")}</option>
        <option value="all">{$t("settings.engines.filter_all")}</option>
      </Select>
      <Select
        size="sm"
        value={type}
        onchange={(e) => {
          const chosen = e.currentTarget.value;
          type = types.find((known) => known === chosen) ?? "all";
        }}
        aria-label={$t("settings.engines.filter_type_label")}
        data-testid="engines-filter-type"
      >
        <option value="all">{$t("settings.engines.type_all")}</option>
        {#each types as known (known)}
          <option value={known}>{$t(ENGINE_TYPE_LABEL_KEYS[known])}</option>
        {/each}
      </Select>
    </div>
    {#if shown.length === 0}
      <p class="py-3 text-body-sm text-muted-foreground" data-testid="engines-list-no-match">
        {$t("settings.engines.no_match")}
      </p>
    {/if}
    <ul class="divide-y divide-border/50">
      {#each shown as row (row.id)}
        <li class="flex items-start gap-3 py-3" data-testid="engine-row">
          <Activity size={15} strokeWidth={1.75} class="mt-0.5 shrink-0 text-muted-foreground" />
          <div class="min-w-0 flex-1">
            <div class="flex flex-wrap items-center gap-2">
              <span class="text-body-sm font-medium text-foreground" data-testid="engine-row-name">
                {label(row)}
              </span>
              {#if row.version}
                <Badge variant="neutral" size="sm" data-testid="engine-row-version">
                  {row.version}
                </Badge>
              {:else}
                <Badge variant="warning" outline size="sm" data-testid="engine-row-version-unknown">
                  {$t("settings.engines.version_unknown")}
                </Badge>
              {/if}
              {#if row.inService}
                <Badge variant="success" outline size="sm" data-testid="engine-row-in-service">
                  {$t("settings.engines.in_service")}
                </Badge>
              {/if}
              {#if row.update?.kind === "available"}
                <Badge variant="info" outline size="sm" data-testid="engine-row-update-available">
                  {$t("settings.engines.update_available", { values: { tag: row.update.tag } })}
                </Badge>
              {:else if row.update?.kind === "latest"}
                <Badge variant="neutral" outline size="sm" data-testid="engine-row-update-latest">
                  {$t("settings.engines.update_latest")}
                </Badge>
              {/if}
              {#if developer && row.source}
                <Badge variant="neutral" outline size="sm" data-testid="engine-row-source">
                  {$t(ENGINE_SOURCE_KEYS[row.source])}
                </Badge>
              {/if}
            </div>
            <p class="text-caption text-muted-foreground">{$t(row.roleKey)}</p>
            {#if developer && row.tag}
              {#if row.tagMismatch}
                <p class="text-caption text-warning" data-testid="engine-row-tag-mismatch">
                  {$t("settings.engines.tag_mismatch", {
                    values: { tag: row.tag, version: row.version ?? "" },
                  })}
                </p>
              {:else}
                <p class="text-caption text-muted-foreground" data-testid="engine-row-tag">
                  {$t("settings.engines.channel_tag", { values: { tag: row.tag } })}
                </p>
              {/if}
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}

  {#snippet footer()}
    <Button variant="ghost" size="sm" onclick={copyDiagnostic} data-testid="about-copy-diagnostics">
      {#snippet icon()}
        {#if copied}
          <Check size={14} strokeWidth={2.2} class="text-success" />
        {:else}
          <Copy size={14} strokeWidth={1.9} />
        {/if}
      {/snippet}
      {$t("settings.about.copy_diagnostics")}
    </Button>
    <span class="ml-auto text-caption text-muted-foreground">
      {$t("settings.about.copy_hint")}
    </span>
  {/snippet}
</SettingsSection>
