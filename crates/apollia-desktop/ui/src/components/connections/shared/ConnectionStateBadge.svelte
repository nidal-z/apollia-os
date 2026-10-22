<script lang="ts">
  /**
   * ConnectionStateBadge - the state of one connection as the runtime found
   * it: the word of its state on a badge, then why it needs repair and how
   * long ago it was found, in a caption. In developer mode the caption ends
   * with the technical detail of the reason. A connection never checked
   * offers Check on its badge when `oncheck` is given.
   */
  import { t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { developerMode } from "$lib/stores/developerMode";
  import { stateBadge } from "$lib/connections/connectionBadge";
  import type { ConnectionReason, ConnectionStatusKey } from "$lib/types";

  interface Props {
    status: ConnectionStatusKey;
    reason: ConnectionReason | null;
    checkedAt: string | null;
    /** Lay the badge and its caption on one line rather than stacked to the right. */
    inline?: boolean;
    /** Check the connection now; the action of a badge that offers one. */
    oncheck?: () => void;
    /** A check is under way. */
    checking?: boolean;
    "data-testid"?: string;
  }

  let { status, reason, checkedAt, inline = false, oncheck, checking = false, "data-testid": testid }: Props =
    $props();

  /** Check without choosing the row the badge sits on. */
  function check(event: Event): void {
    event.stopPropagation();
    oncheck?.();
  }

  const badge = $derived(stateBadge(status, reason, checkedAt));
  const caption = $derived(
    [
      badge.reasonKey ? $t(badge.reasonKey) : null,
      badge.when ? $t(badge.when.key, { values: badge.when.values }) : null,
      $developerMode && reason?.detail ? reason.detail : null,
    ]
      .filter((part): part is string => part !== null)
      .join(" · "),
  );
</script>

<span
  class="flex min-w-0 gap-1 {inline ? 'flex-row flex-wrap items-center gap-2' : 'flex-col items-end'}"
  data-testid={testid}
  data-status={status}
>
  <Badge size="sm" variant={badge.variant} outline={status === "paused"}>{$t(badge.labelKey)}</Badge>
  {#if caption}
    <span class="truncate text-caption text-muted-foreground" data-testid={testid ? `${testid}-caption` : undefined}>
      {caption}
    </span>
  {/if}
  {#if badge.actionKey && oncheck}
    <Button
      variant="outline"
      size="auto"
      class="h-6 px-2 text-caption"
      loading={checking}
      onclick={check}
      onkeydown={(event: KeyboardEvent) => event.stopPropagation()}
      data-testid={testid ? `${testid}-check` : undefined}
    >
      {$t(badge.actionKey)}
    </Button>
  {/if}
</span>
