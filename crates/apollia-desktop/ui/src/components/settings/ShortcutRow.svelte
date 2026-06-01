<script lang="ts">
  /**
   * A single shortcut row : description + scope hint on the
   * left, keycap(s) on the right. Rendered inside `/settings/shortcuts`.
   */
  import { t } from "svelte-i18n";
  import type { ShortcutEntry } from "$lib/navigation/shortcuts";
  import { comboFor, splitCombo } from "$lib/navigation/shortcuts";

  interface Props {
    entry: ShortcutEntry;
    mac: boolean;
  }

  let { entry, mac }: Props = $props();

  let parts = $derived(splitCombo(comboFor(entry, mac)));
  let ariaLabel = $derived(
    `${$t(entry.descriptionKey)} - ${comboFor(entry, mac).replace(/\+/g, " ")}`,
  );
</script>

<div
  class="flex items-center justify-between gap-4 px-4 py-2.5"
  data-testid="shortcut-row-{entry.id}"
>
  <div class="min-w-0 flex-1">
    <p class="text-sm text-foreground">{$t(entry.descriptionKey)}</p>
    {#if entry.scopeHintKey}
      <p class="text-[11px] text-muted-foreground/70">{$t(entry.scopeHintKey)}</p>
    {:else if entry.scope === "global"}
      <p class="text-[11px] text-muted-foreground/70">{$t("settings.shortcuts.scope.global")}</p>
    {/if}
  </div>
  <div class="flex flex-shrink-0 items-center gap-1" aria-label={ariaLabel}>
    {#each parts as part, i (i)}
      {#if i > 0}
        <span class="text-xs text-muted-foreground/50" aria-hidden="true">+</span>
      {/if}
      <kbd
        class="inline-flex min-w-[1.75rem] items-center justify-center rounded-md border border-border bg-muted/40 px-2 py-0.5 text-xs font-mono text-muted-foreground"
      >
        {part}
      </kbd>
    {/each}
  </div>
</div>
