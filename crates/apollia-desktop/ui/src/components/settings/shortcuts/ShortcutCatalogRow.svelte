<script lang="ts">
  /**
   * One catalog row: description + optional scope hint on the left, computed
   * coverage flags and the keycap combo on the right. Read-only - this row
   * offers no rebind affordance because the catalog has no persistence backing
   * yet (see the page header for the report note).
   */
  import { t } from "svelte-i18n";
  import { Slash, TriangleAlert } from "lucide-svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { comboFor, type ShortcutEntry } from "$lib/navigation/shortcuts";
  import ShortcutCombo from "./ShortcutCombo.svelte";

  interface Props {
    entry: ShortcutEntry;
    mac: boolean;
    reserved?: boolean;
    overlaps?: boolean;
    /** Resolved scope label shown when the group mixes scopes (no group badge). */
    scopeInline?: string | null;
  }

  let {
    entry,
    mac,
    reserved = false,
    overlaps = false,
    scopeInline = null,
  }: Props = $props();

  let description = $derived($t(entry.descriptionKey));
  let scopeHint = $derived(
    entry.scopeHintKey ? $t(entry.scopeHintKey) : scopeInline,
  );
  let ariaLabel = $derived(
    `${description}: ${comboFor(entry, mac).replace(/\+/g, " ")}`,
  );
</script>

<div
  class="flex items-center justify-between gap-4 border-b border-border/60 py-2.5 last:border-b-0"
  data-testid="shortcut-row-{entry.id}"
>
  <div class="min-w-0 flex-1">
    <p class="text-body-sm text-foreground">{description}</p>
    {#if scopeHint}
      <p class="mt-0.5 text-caption text-muted-foreground/70">{scopeHint}</p>
    {/if}
  </div>
  <div class="flex flex-shrink-0 items-center gap-2">
    {#if reserved}
      <Badge variant="danger" size="sm" outline title={$t("settings.shortcuts.reserved_hint")}>
        {#snippet icon()}<Slash size={11} strokeWidth={2} aria-hidden="true" />{/snippet}
        {$t("settings.shortcuts.reserved")}
      </Badge>
    {:else if overlaps}
      <Badge variant="warning" size="sm" outline title={$t("settings.shortcuts.overlaps_hint")}>
        {#snippet icon()}<TriangleAlert size={11} strokeWidth={2} aria-hidden="true" />{/snippet}
        {$t("settings.shortcuts.overlaps_global")}
      </Badge>
    {/if}
    <ShortcutCombo {entry} {mac} {ariaLabel} />
  </div>
</div>
