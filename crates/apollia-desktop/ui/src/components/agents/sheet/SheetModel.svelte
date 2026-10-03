<script lang="ts">
  /**
   * SheetModel - the model the agent runs on and where it runs: its own
   * assignment when the user gave it one, otherwise the default model it
   * follows. When the assigned model was not available, the notice says
   * when, and which model served instead.
   */
  import { t, locale } from "svelte-i18n";
  import { AlertTriangle, Cloud, Cpu } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { DeveloperOnly } from "$lib/components/shared";
  import { formatRelativeTime } from "$lib/utils";
  import SheetSection from "./SheetSection.svelte";
  import type { ModelView } from "./agentSheet";

  interface Props {
    view: ModelView;
    fallback: { requested: string; used: string; at: string } | null;
  }

  let { view, fallback }: Props = $props();

  const placeLabel = $derived(
    view.place === "server"
      ? $t("agent_sheet.model_place_server", { values: { server: view.server ?? "" } })
      : view.place === "local"
        ? $t("agent_sheet.model_place_local")
        : $t("agent_sheet.model_place_default"),
  );
</script>

<SheetSection
  title={$t("agent_sheet.model_title")}
  legend={$t("agent_sheet.model_legend")}
  testid="agent-sheet-model"
>
  <div class="flex items-start gap-3">
    {#if view.place === "server"}
      <Cloud size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
    {:else}
      <Cpu size={16} class="mt-0.5 shrink-0 text-muted-foreground" />
    {/if}
    <div class="min-w-0 flex-1">
      <div class="text-body-sm text-foreground" data-testid="agent-sheet-model-name">
        {view.name ?? $t("agent_sheet.model_default_unknown")}
      </div>
      <div class="text-caption text-muted-foreground" data-testid="agent-sheet-model-place">
        {placeLabel}
      </div>
    </div>
    {#if view.preset !== null}
      <DeveloperOnly>
        <Badge size="sm" variant="neutral">{$t("agent_sheet.model_preset", { values: { id: view.preset } })}</Badge>
      </DeveloperOnly>
    {/if}
  </div>
  {#if fallback}
    <div
      class="mt-3 flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2"
      role="status"
      data-testid="agent-sheet-model-fallback"
    >
      <AlertTriangle size={14} class="mt-0.5 shrink-0 text-warning-a11y" />
      <p class="m-0 text-caption text-foreground/85">
        {$t("agent_sheet.model_fallback", {
          values: {
            when: formatRelativeTime(fallback.at, $locale ?? "fr"),
            requested: fallback.requested,
            used: fallback.used,
          },
        })}
      </p>
    </div>
  {/if}
</SheetSection>
