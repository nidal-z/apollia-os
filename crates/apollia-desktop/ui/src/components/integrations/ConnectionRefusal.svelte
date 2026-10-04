<script lang="ts">
  /**
   * The one box that says a connection was refused: who refused it (the
   * organisation's policy or the person's own permissions), why, that nothing
   * was installed, and where to go next.
   */
  import { t } from "svelte-i18n";
  import { ShieldAlert } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { REFUSAL_TEXT_KEYS, type Refusal } from "$lib/integrations/addConnection";

  interface Props {
    refusal: Refusal;
    /** Open the place where the refusing rule is read. */
    onopen: () => void;
  }

  let { refusal, onopen }: Props = $props();

  const title = $derived(
    refusal.by === "organisation"
      ? $t("connections.refusal.organisation_title")
      : $t("connections.refusal.permissions_title"),
  );
</script>

<div
  class="space-y-2 rounded-lg border border-destructive/30 bg-destructive/5 p-4"
  role="alert"
  data-testid="connection-refusal"
  data-refusal={refusal.code}
>
  <div class="flex items-center gap-2">
    <ShieldAlert size={16} class="shrink-0 text-destructive" aria-hidden="true" />
    <p class="text-sm font-semibold text-destructive">{title}</p>
  </div>
  <p class="text-sm text-foreground" data-testid="connection-refusal-reason">
    {$t(REFUSAL_TEXT_KEYS[refusal.code].body)}
  </p>
  <p class="text-xs text-muted-foreground">{$t("connections.refusal.nothing_installed")}</p>
  {#if refusal.by === "organisation"}
    <p class="text-xs text-muted-foreground">{$t("connections.refusal.ask_administrator")}</p>
  {/if}
  <div class="pt-1">
    <Button size="sm" variant="outline" onclick={onopen} data-testid="connection-refusal-open">
      {refusal.by === "organisation"
        ? $t("connections.refusal.open_policy")
        : $t("connections.refusal.open_permissions")}
    </Button>
  </div>
</div>
