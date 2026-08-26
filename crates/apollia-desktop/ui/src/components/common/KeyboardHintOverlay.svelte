<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { fade, fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { shortcuts, registerShortcuts, type Shortcut } from "$lib/stores/shortcuts";
  import { KeyboardHint } from "$lib/components/feedback";
  import { X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  let open = $state(false);

  const modKey = typeof navigator !== "undefined" && navigator.platform.startsWith("Mac")
    ? "Cmd"
    : "Ctrl";

  // Canonical shortcuts owned by App root - other components register
  // additional entries via `registerShortcuts` on mount.
  const builtins: Shortcut[] = [
    { id: "nav.back",    keys: [modKey, "["], descriptionKey: "a11y.shortcut.nav_back",    group: "navigation" },
    { id: "nav.forward", keys: [modKey, "]"], descriptionKey: "a11y.shortcut.nav_forward", group: "navigation" },
    { id: "a11y.open_hints", keys: ["?"],     descriptionKey: "a11y.shortcut.open_hints",  group: "help" },
    { id: "a11y.close",  keys: ["Esc"],       descriptionKey: "a11y.shortcut.close_modal", group: "help" },
  ];

  onMount(() => {
    const unregister = registerShortcuts(builtins);

    function handleKey(event: KeyboardEvent) {
      // Do not intercept when the user is typing in a field.
      const target = event.target as HTMLElement | null;
      const isEditable =
        target instanceof HTMLInputElement ||
        target instanceof HTMLTextAreaElement ||
        target?.getAttribute("contenteditable") === "true";

      if (event.key === "Escape" && open) {
        event.preventDefault();
        open = false;
        return;
      }
      if (isEditable) return;
      if (event.key === "?" || (event.shiftKey && event.key === "/")) {
        event.preventDefault();
        open = !open;
      }
    }

    window.addEventListener("keydown", handleKey);
    return () => {
      window.removeEventListener("keydown", handleKey);
      unregister();
    };
  });

  // Group shortcuts by their `group` field for rendering.
  const grouped = $derived.by(() => {
    const buckets = new Map<string, Shortcut[]>();
    for (const shortcut of $shortcuts) {
      const list = buckets.get(shortcut.group) ?? [];
      list.push(shortcut);
      buckets.set(shortcut.group, list);
    }
    return Array.from(buckets.entries()).sort(([a], [b]) => a.localeCompare(b));
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-[90] app-backdrop"
    onclick={() => (open = false)}
    onkeydown={(e) => {
      if (e.key === "Escape") open = false;
    }}
    role="presentation"
    transition:fade={{ duration: 150 }}
    data-testid="keyboard-hint-backdrop"
  ></div>
  <div class="glass-card glass-border fixed left-1/2 top-1/2 z-[91] w-[min(28rem,90vw)] -translate-x-1/2 -translate-y-1/2 rounded-xl p-5" role="dialog" aria-modal="true" aria-label={$t("a11y.keyboard_shortcuts")} transition:fly={{ y: 8, duration: 200, easing: cubicOut }} data-testid="keyboard-hint-overlay">
    <div class="mb-3 flex items-center justify-between">
      <h2 class="text-sm font-semibold text-foreground">
        {$t("a11y.keyboard_shortcuts")}
      </h2>
      <Button variant="ghost" size="sm"
        class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-a11y hover:bg-muted hover:text-foreground"
        onclick={() => (open = false)}
        aria-label={$t("common.close")}
        data-testid="keyboard-hint-close"
      >
        <X size={16} strokeWidth={1.75} />
      </Button>
    </div>

    {#if grouped.length === 0}
      <p class="text-sm text-muted-a11y">{$t("a11y.keyboard_shortcuts_hint")}</p>
    {:else}
      <div class="space-y-3">
        {#each grouped as [group, entries]}
          <section>
            <h3 class="mb-1 text-caption font-semibold uppercase tracking-wider text-muted-a11y">
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
