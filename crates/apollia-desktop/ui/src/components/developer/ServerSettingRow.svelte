<script lang="ts">
  /**
   * One row of the settings of the local server: the name of the setting,
   * the "?" that explains it on hover, its control, and, when the
   * organization policy imposes it, the mention that says so.
   *
   * The explanation is required: a row cannot be drawn without its text.
   */
  import { t } from "svelte-i18n";
  import type { Snippet } from "svelte";
  import { CircleHelp, Lock } from "lucide-svelte";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { cn } from "$lib/utils";
  import { SERVER_SETTINGS_TEXTS } from "./serverSettings";

  interface Props {
    /** The name of the setting, already translated. */
    label: string;
    /** The explanation shown on hover of the "?", already translated. */
    help: string;
    /** The `data-testid` of the row; its parts derive from it. */
    testid: string;
    /** The id of the control the name labels. */
    for: string;
    /** The organization policy imposes the value. */
    locked?: boolean;
    /** The row cannot be set right now. */
    dimmed?: boolean;
    /** The row sits under the one before it. */
    under?: boolean;
    /** The control, on the right. */
    control: Snippet;
    /** What the row carries under its name. */
    detail?: Snippet;
  }

  let {
    label,
    help,
    testid,
    for: controlId,
    locked = false,
    dimmed = false,
    under = false,
    control,
    detail,
  }: Props = $props();
</script>

<div
  class={cn("flex flex-col gap-1.5 px-3 py-2.5", under ? "pt-0" : "border-t border-border/60 first:border-t-0")}
  data-testid={testid}
  data-locked={locked}
>
  <div class="flex items-center gap-3">
    <label
      for={controlId}
      class={cn(
        "min-w-0 flex-1",
        under ? "text-xs text-muted-foreground" : "text-sm text-foreground",
        dimmed && "opacity-60",
      )}
    >
      {label}
    </label>
    <Tooltip
      content={help}
      side="bottom"
      class="max-w-xs"
      triggerLabel={$t(SERVER_SETTINGS_TEXTS.helpLabel, { values: { setting: label } })}
      triggerTestid={`${testid}-help`}
      triggerClass="shrink-0 cursor-help rounded-full text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    >
      <CircleHelp class="h-3.5 w-3.5" aria-hidden="true" />
    </Tooltip>
    <div class="flex shrink-0 items-center gap-2">
      {@render control()}
    </div>
  </div>
  {#if locked}
    <Tooltip
      content={$t(SERVER_SETTINGS_TEXTS.policyLockedHint)}
      side="bottom"
      class="max-w-xs"
      triggerTestid={`${testid}-policy`}
      triggerClass="w-fit cursor-help gap-1.5 text-xs text-muted-foreground"
    >
      <Lock class="h-3 w-3" aria-hidden="true" />
      {$t(SERVER_SETTINGS_TEXTS.policyLocked)}
    </Tooltip>
  {/if}
  {@render detail?.()}
</div>
