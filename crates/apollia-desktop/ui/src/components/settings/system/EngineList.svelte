<script lang="ts" module>
  import type { EngineChannel, EngineEntry, EnginesResponse } from "$lib/ipc/system";

  /** Where an engine stands against the channel it follows. */
  export type EngineUpdate =
    | { kind: "latest" }
    /** A newer build exists; `tag` names it. */
    | { kind: "available"; tag: string };

  /** What a row of the engine list says: which engine, built for what, at which version. */
  export interface EngineRow {
    id: string;
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
  export function engineRows(response: EnginesResponse | null): EngineRow[] {
    if (!response) return [];
    const rows: EngineRow[] = response.engines.map((e, i) => ({
      id: `${e.engine}-${e.variant ?? "any"}-${e.tag ?? e.source}-${i}`,
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
</script>

<script lang="ts">
  /**
   * EngineList - System > Engines: every engine of this machine, embedded or
   * installed by the engine store, with the version it states at its source,
   * the one in service, where each stands against the followed channel, and
   * the dictation engine. One table, one source: the engine list of the
   * runtime. The developer mode adds the channel, the source of each engine
   * and the tag the channel published it under. The copy of these facts is
   * "Copy the information", in the header of System > Hardware.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Activity, RefreshCw } from "lucide-svelte";
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
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    response: EnginesResponse | null;
    loading: boolean;
    error: string | null;
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
    developer,
    checking,
    checkError,
    onchannel,
    oncheck,
  }: Props = $props();

  const rows = $derived(engineRows(response));

  function label(row: EngineRow): string {
    const variant = variantLabel(row.variant, $t("settings.engines.variant_cpu"));
    return variant ? `${row.name} · ${variant}` : row.name;
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
    <ul class="divide-y divide-border/50">
      {#each rows as row (row.id)}
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

</SettingsSection>
