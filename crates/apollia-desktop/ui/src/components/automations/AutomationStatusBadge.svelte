<script lang="ts">
  /**
   * Status pill for an automation card: active / paused / error.
   *
   * `active` glows to match the status-bar accent used on the card.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Tooltip } from "$lib/components/ui/tooltip";

  type Status = "active" | "paused" | "error";

  interface Props {
    status: Status;
    errorTooltip?: string;
  }

  let { status, errorTooltip }: Props = $props();

  const variant = $derived(
    status === "active" ? "gradient-success" :
    status === "error" ? "gradient-destructive" :
    "neutral",
  );

  const labelKey = $derived(
    status === "active" ? "automations.status.active" :
    status === "error" ? "automations.status.error" :
    "automations.status.paused",
  );
</script>

{#if status === "error" && errorTooltip}
  <Tooltip content={errorTooltip}>
    <Badge {variant} data-testid="automation-status-{status}">
      {$t(labelKey)}
    </Badge>
  </Tooltip>
{:else}
  <Badge
    {variant}
    class={status === "active" ? "shadow-[0_0_12px_rgba(16,185,129,0.45)]" : ""}
    data-testid="automation-status-{status}"
  >
    {$t(labelKey)}
  </Badge>
{/if}
