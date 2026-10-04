<script lang="ts" module>
  import type { EngineEntry, EnginesResponse } from "$lib/ipc/system";

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
    }));
    if (response.dictation) {
      rows.push({
        id: "whisper",
        name: "whisper.cpp",
        variant: null,
        roleKey: "settings.engines.role_dictation",
        version: response.dictation.version,
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
   * and the dictation engine. The diagnostic copy composes the same facts.
   */
  import { t } from "svelte-i18n";
  import { Cpu, Copy, Check, Activity } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";
  import SettingsSection from "../SettingsSection.svelte";

  interface Props {
    response: EnginesResponse | null;
    loading: boolean;
    error: string | null;
    /** Lines of the diagnostic block that precede the engines. */
    diagnosticHead: string[];
  }

  let { response, loading, error, diagnosticHead }: Props = $props();

  const rows = $derived(engineRows(response));

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

<SettingsSection
  title={$t("settings.engines.list_title")}
  description={$t("settings.engines.list_desc")}
  bodyLayout="flush"
  data-testid="engines-list"
>
  {#snippet icon()}<Cpu size={15} strokeWidth={1.75} />{/snippet}

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
            </div>
            <p class="text-caption text-muted-foreground">{$t(row.roleKey)}</p>
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
