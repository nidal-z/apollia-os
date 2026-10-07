<script lang="ts">
  /**
   * ModelDevSections - the two sections of the panel of a model that exist
   * in developer mode only: the type chosen over the detected one, and
   * another weights file of the same model to load in its place. With the
   * developer mode off, nothing of them is rendered.
   */
  import { locale, t } from "svelte-i18n";
  import { Check } from "lucide-svelte";
  import type { ModelDetail } from "$lib/ipc/modelDetail";
  import type { ModelCategory } from "$lib/ipc/modelHub";
  import { developerMode } from "$lib/stores/developerMode";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { Toggle } from "$lib/components/ui/toggle";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { cn } from "$lib/utils";
  import { gigabytes } from "./aiModels";
  import {
    TYPE_CHOICES,
    devSections,
    overrideOf,
    sourceToggle,
    sourceView,
    typeChoiceOf,
    typeExplanationKey,
    typeLabelKey,
  } from "./modelDetail";

  interface Props {
    detail: ModelDetail | null;
    working: boolean;
    ontype: (category: ModelCategory | null) => void;
    onsource: (path: string | null) => void;
  }

  let { detail, working, ontype, onsource }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const sections = $derived(devSections($developerMode));
  const typeChoice = $derived(typeChoiceOf(detail));
  const source = $derived(sourceView(detail));

  const OPTION =
    "flex w-full items-center justify-between gap-2 rounded-md border px-2 py-1.5 text-left text-caption transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:opacity-50";

  function toggleSource(on: boolean): void {
    if (!detail) return;
    const path = sourceToggle(detail, on);
    if (path !== undefined) onsource(path);
  }
</script>

{#each sections as section (section)}
  {#if section === "type"}
    <Disclosure testid="ai-panel-dev-type">
      {#snippet summary()}
        <span class="font-medium text-foreground">{$t("dev.ai.panel.type.title")}</span>
      {/snippet}
      <div class="flex flex-col gap-1.5" role="radiogroup" aria-label={$t("dev.ai.panel.type.title")}>
        <p class="m-0 text-caption text-muted-foreground">{$t("dev.ai.panel.type.hint")}</p>
        {#each TYPE_CHOICES as choice (choice)}
          <Tooltip content={$t(typeExplanationKey(choice))} side="left">
            {#snippet trigger(props)}
              <button
                {...props}
                type="button"
                role="radio"
                aria-checked={typeChoice === choice}
                class={cn(
                  OPTION,
                  typeChoice === choice
                    ? "border-primary bg-primary/10 text-foreground"
                    : "border-border text-foreground hover:bg-muted",
                )}
                disabled={working || detail === null}
                onclick={() => ontype(overrideOf(choice))}
                data-testid="ai-panel-dev-type-{choice}"
              >
                <span>{$t(typeLabelKey(choice))}</span>
                {#if typeChoice === choice}
                  <Check size={13} aria-hidden="true" />
                {/if}
              </button>
            {/snippet}
          </Tooltip>
        {/each}
      </div>
    </Disclosure>
  {:else}
    <Disclosure testid="ai-panel-dev-source">
      {#snippet summary()}
        <span class="font-medium text-foreground">{$t("dev.ai.panel.source.title")}</span>
      {/snippet}
      <div class="flex flex-col gap-2">
        <p class="m-0 text-caption text-muted-foreground">{$t("dev.ai.panel.source.hint")}</p>
        <div class="flex items-center justify-between gap-2">
          <label for="ai-panel-dev-source-enable" class="text-caption font-medium text-foreground">
            {$t("dev.ai.panel.source.enable")}
          </label>
          <Toggle
            id="ai-panel-dev-source-enable"
            size="sm"
            checked={source.enabled}
            onchange={toggleSource}
            disabled={working || !source.available}
            aria-label={$t("dev.ai.panel.source.enable")}
            data-testid="ai-panel-dev-source-enable"
          />
        </div>
        {#if !source.available}
          <p class="m-0 text-caption text-muted-foreground" data-testid="ai-panel-dev-source-empty">
            {$t("dev.ai.panel.source.empty")}
          </p>
        {:else if source.enabled && detail}
          <ul class="m-0 flex list-none flex-col gap-1.5 p-0" role="radiogroup" aria-label={$t("dev.ai.panel.source.title")}>
            {#each detail.source_candidates as candidate (candidate.path)}
              <li>
                <button
                  type="button"
                  role="radio"
                  aria-checked={source.chosen === candidate.path}
                  class={cn(
                    OPTION,
                    source.chosen === candidate.path
                      ? "border-primary bg-primary/10 text-foreground"
                      : "border-border text-foreground hover:bg-muted",
                  )}
                  disabled={working}
                  onclick={() => onsource(candidate.path)}
                  data-testid="ai-panel-dev-source-file"
                >
                  <span class="min-w-0 break-all">{candidate.name}</span>
                  <span class="shrink-0 text-muted-foreground">
                    {$t("ai.installed.size", { values: { size: gigabytes(candidate.size_bytes, lang) } })}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </Disclosure>
  {/if}
{/each}
