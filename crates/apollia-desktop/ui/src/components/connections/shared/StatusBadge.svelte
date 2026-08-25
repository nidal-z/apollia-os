<script lang="ts">
  /**
   * StatusBadge - unified status pill for native + MCP connectors.
   *
   * Wraps the design-system `Badge` with the status-driven variant, a `StatusDot`
   * glyph, and the localized short label. Text plus dot (never colour alone), so
   * the state stays legible for colour-vision-deficient users.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { StatusDot } from "$lib/components/operator";
  import type { ConnectionStatus } from "$lib/connections/status";
  import {
    statusAccent,
    statusBadgeVariant,
    statusLabelKey,
  } from "$lib/connections/status";

  interface Props {
    status: ConnectionStatus;
    /** Override the label (e.g. active-account count on native connectors). */
    label?: string;
    "data-testid"?: string;
  }

  let { status, label, "data-testid": testid }: Props = $props();
</script>

<Badge
  variant={statusBadgeVariant(status)}
  size="sm"
  outline={status === "idle"}
  data-testid={testid}
>
  {#snippet icon()}
    <StatusDot color={statusAccent(status)} glow={status === "active" || status === "attention"} />
  {/snippet}
  {label ?? $t(statusLabelKey(status))}
</Badge>
