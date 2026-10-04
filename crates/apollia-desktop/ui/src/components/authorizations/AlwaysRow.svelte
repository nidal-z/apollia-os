<script lang="ts">
  /**
   * AlwaysRow - one standing consent: the family it covers, since when, and
   * its withdrawal in one click. Once withdrawn the line stays in place, says
   * so, and offers the undo.
   */
  import { locale, t } from "svelte-i18n";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import type { FamilyConsent } from "$lib/ipc/permissions";
  import { FAMILY_LABEL, formatDay, isFamilyKey } from "./authorizations";

  interface Props {
    consent: FamilyConsent;
    withdrawn: boolean;
    busy?: boolean;
    onWithdraw: (consent: FamilyConsent) => void;
    onUndo: (consent: FamilyConsent) => void;
  }

  let { consent, withdrawn, busy = false, onWithdraw, onUndo }: Props = $props();

  const family = $derived(
    isFamilyKey(consent.family) ? $t(FAMILY_LABEL[consent.family]) : consent.family,
  );
  const since = $derived(formatDay(consent.granted_at, $locale ?? "en"));
</script>

<li
  class="flex items-center gap-3 border-b border-border/40 py-2.5 last:border-b-0"
  data-testid="authorizations-always"
  data-family={consent.family}
  data-state={withdrawn ? "withdrawn" : "given"}
>
  <div class="min-w-0 flex-1">
    <div class="text-body-sm {withdrawn ? 'text-muted-foreground' : 'text-foreground'}">{family}</div>
    <div class="text-caption text-muted-foreground">
      {#if withdrawn}
        {$t("authorizations.always_withdrawn")}
      {:else if consent.granted_by === "upgrade"}
        {$t("authorizations.always_since_upgrade", { values: { date: since } })}
      {:else}
        {$t("authorizations.always_since", { values: { date: since } })}
      {/if}
    </div>
  </div>
  {#if withdrawn}
    <Badge size="sm" variant="neutral" outline>{$t("authorizations.withdrawn_badge")}</Badge>
    <Button
      variant="ghost"
      size="sm"
      disabled={busy}
      loading={busy}
      onclick={() => onUndo(consent)}
      data-testid="authorizations-always-undo"
    >
      {$t("common.cancel")}
    </Button>
  {:else}
    <Button
      variant="outline"
      size="sm"
      disabled={busy}
      loading={busy}
      onclick={() => onWithdraw(consent)}
      data-testid="authorizations-always-withdraw"
    >
      {$t("authorizations.withdraw")}
    </Button>
  {/if}
</li>
