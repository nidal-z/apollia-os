<script lang="ts">
  /**
   * Chat shortcuts help dialog.
   *
   * Lists every binding currently registered in `$shortcuts`, grouped
   * by `group`. Opened by `Cmd+/` from the central chat handler - the
   * caller binds `open` and the dialog closes on Esc or backdrop click.
   *
   * Distinct from the global `?` overlay (KeyboardHintOverlay) which is
   * always-mounted at app root: this dialog is route-scoped so its
   * content matches the chat experience precisely.
   */
  import { t } from "svelte-i18n";
  import { fade, scale } from "svelte/transition";
  import { backOut } from "svelte/easing";
  import { X } from "lucide-svelte";
  import { shortcuts } from "$lib/stores/shortcuts";
  import { KeyboardHint } from "$lib/components/feedback";
  import { focusTrap } from "$lib/shortcuts/focusTrap";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    open?: boolean;
    onclose?: () => void;
  }

  let { open = $bindable(false), onclose }: Props = $props();

  const grouped = $derived.by(() => {
    const buckets = new Map<string, typeof $shortcuts>();
    for (const entry of $shortcuts) {
      const list = buckets.get(entry.group) ?? [];
      list.push(entry);
      buckets.set(entry.group, list);
    }
    return Array.from(buckets.entries()).sort(([a], [b]) => a.localeCompare(b));
  });

  function close(): void {
    open = false;
    onclose?.();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  }
</script>

{#if open}
  <div
    class="fixed inset-0 backdrop-warm"
    style="z-index: var(--z-backdrop);"
    role="presentation"
    transition:fade={{ duration: 150 }}
    onclick={close}
    onkeydown={handleKeydown}
    data-testid="shortcuts-help-backdrop"
  ></div>
  <div class="glass-card glass-border fixed left-1/2 top-1/2 w-[min(32rem,90vw)] -translate-x-1/2 -translate-y-1/2 p-5" style="z-index: var(--z-overlay);" role="dialog" aria-modal="true" aria-label={$t("a11y.keyboard_shortcuts")} transition:scale={{ start: 0.97, duration: 200, easing: backOut }} onkeydown={handleKeydown} use:focusTrap tabindex="-1" data-testid="shortcuts-help-dialog">
    <div class="mb-3 flex items-center justify-between">
      <h2 class="text-sm font-semibold text-foreground">
        {$t("a11y.keyboard_shortcuts")}
      </h2>
      <Button variant="ghost" size="sm"
        type="button"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
        onclick={close}
        aria-label={$t("common.close")}
        data-testid="shortcuts-help-close"
      >
        <X size={16} strokeWidth={1.75} />
      </Button>
    </div>

    {#if grouped.length === 0}
      <p class="text-sm text-muted-foreground">
        {$t("a11y.keyboard_shortcuts_hint")}
      </p>
    {:else}
      <div class="max-h-[60vh] overflow-y-auto space-y-3 pr-1">
        {#each grouped as [group, entries] (group)}
          <section>
            <h3 class="mb-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {group}
            </h3>
            <ul class="space-y-1">
              {#each entries as entry (entry.id)}
                <li class="flex items-center justify-between gap-4 text-sm">
                  <span class="text-foreground">{$t(entry.descriptionKey)}</span>
                  <KeyboardHint keys={entry.keys} />
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  </div>
{/if}
