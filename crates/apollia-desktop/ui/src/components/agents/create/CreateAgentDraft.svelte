<script lang="ts">
  /**
   * CreateAgentDraft - the draft of the sheet, beside the conversation that
   * writes it: one row per field, filled, being written or still to come.
   * Read only: the draft is adjusted in the form, and nothing here is stored.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle, Circle, Loader2 } from "lucide-svelte";
  import { Card } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { readableName } from "../sheet/agentSheet";
  import {
    ACCESS_NOTE_LABEL,
    DRAFT_FIELD_LABEL,
    UNAVAILABLE_LABEL,
    draftFields,
    unavailableViews,
    type DraftField,
    type ProposedAgent,
  } from "./agentDraft";

  interface Props {
    draft: ProposedAgent | null;
    /** Whether Apollia is answering. */
    answering: boolean;
  }

  let { draft, answering }: Props = $props();

  const fields = $derived(draftFields(draft, answering));
  const missing = $derived(unavailableViews(draft?.unavailable ?? []));

  function line(field: DraftField, value: string): string {
    if (field === "access") return readableName(value);
    if (field === "sources" && value === "profile") return $t("agent_sheet.source_profile");
    if (field === "sources" && value === "memory") return $t("agent_sheet.source_memory");
    return value;
  }

  function note(field: DraftField, value: string): string | null {
    if (field !== "access") return null;
    const access = draft?.access.find((a) => a.id === value);
    if (access?.sensitive) return ACCESS_NOTE_LABEL.sensitive;
    if (access?.by_default) return ACCESS_NOTE_LABEL.by_default;
    return null;
  }
</script>

<section class="space-y-2" data-testid="agent-create-draft" aria-label={$t("agent_create.draft_title")}>
  <div class="flex items-center justify-between gap-2">
    <h3 class="m-0 text-body-sm font-semibold text-foreground">{$t("agent_create.draft_title")}</h3>
    <Badge size="sm" variant="neutral" outline>{$t("agent_create.draft_unsaved")}</Badge>
  </div>
  <Card class="p-3.5">
    <ul class="m-0 list-none space-y-3 p-0">
      {#each fields as view (view.field)}
        <li
          class="flex items-start gap-2.5"
          data-testid="agent-create-draft-field"
          data-field={view.field}
          data-state={view.state}
        >
          <span class="mt-0.5 shrink-0">
            {#if view.state === "filled"}
              <CheckCircle size={14} class="text-success-a11y" aria-hidden="true" />
            {:else if view.state === "writing"}
              <Loader2 size={14} class="animate-spin text-muted-foreground" aria-hidden="true" />
            {:else}
              <Circle size={14} class="text-muted-foreground/50" aria-hidden="true" />
            {/if}
          </span>
          <div class="min-w-0 flex-1">
            <div class="text-caption font-medium text-foreground">{$t(DRAFT_FIELD_LABEL[view.field])}</div>
            {#if view.state === "writing"}
              <p class="m-0 text-body-xs text-muted-foreground">{$t("agent_create.field_writing")}</p>
            {:else if view.state === "upcoming"}
              <p class="m-0 text-body-xs text-muted-foreground/70">{$t("agent_create.field_upcoming")}</p>
            {:else if view.defaultModel}
              <p class="m-0 text-body-xs text-foreground/85">{$t("agent_adjust.model_default")}</p>
            {:else}
              <ul class="m-0 list-none space-y-0.5 p-0">
                {#each view.lines as value (value)}
                  {@const label = note(view.field, value)}
                  <li class="flex flex-wrap items-center gap-1.5 text-body-xs text-foreground/85">
                    <span class="min-w-0 break-words">{line(view.field, value)}</span>
                    {#if label}
                      <Badge size="sm" variant={label === ACCESS_NOTE_LABEL.sensitive ? "warning" : "neutral"}>
                        {$t(label)}
                      </Badge>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
    {#if missing.length > 0}
      <div class="mt-3 border-t border-border/60 pt-3" data-testid="agent-create-unavailable">
        <div class="text-caption font-medium text-foreground">{$t("agent_create.unavailable_title")}</div>
        <ul class="m-0 mt-1 list-none space-y-0.5 p-0">
          {#each missing as entry (entry.kind + entry.name)}
            <li class="text-body-xs text-muted-foreground">
              {$t(UNAVAILABLE_LABEL[entry.kind], { values: { name: entry.name } })}
            </li>
          {/each}
        </ul>
        <p class="m-0 mt-1 text-caption text-muted-foreground">{$t("agent_create.unavailable_hint")}</p>
      </div>
    {/if}
  </Card>
</section>
