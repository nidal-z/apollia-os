<script lang="ts" context="module">
  export const meta = {
    title: "settings.nav.shortcuts",
    icon: "keyboard",
    group: "settings.nav.cluster_system",
    cluster: "system",
  } as const;
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { Keyboard } from "lucide-svelte";

  const SHORTCUTS: { combo: string; labelKey: string }[] = [
    { combo: "⌘/Ctrl + K", labelKey: "settings.shortcuts.command_palette" },
    { combo: "⌘/Ctrl + T", labelKey: "settings.shortcuts.new_task" },
    { combo: "⌘/Ctrl + Shift + A", labelKey: "settings.shortcuts.toggle_artifacts" },
    { combo: "⌘/Ctrl + [", labelKey: "settings.shortcuts.nav_back" },
    { combo: "⌘/Ctrl + ]", labelKey: "settings.shortcuts.nav_forward" },
    { combo: "?", labelKey: "settings.shortcuts.keyboard_overlay" },
  ];
</script>

<section class="space-y-4" data-testid="shortcuts-section">
  <div class="flex items-start gap-3">
    <Keyboard size={20} strokeWidth={1.75} class="mt-0.5 text-muted-foreground" aria-hidden="true" />
    <p class="text-sm text-muted-foreground">{$t('settings.shortcuts.subtitle')}</p>
  </div>
  <div class="glass-card glass-border rounded-lg divide-y divide-border/50">
    {#each SHORTCUTS as shortcut (shortcut.combo)}
      <div class="flex items-center justify-between px-4 py-2.5">
        <span class="text-sm text-foreground">{$t(shortcut.labelKey)}</span>
        <kbd class="rounded-md border border-border bg-muted/40 px-2 py-0.5 text-xs font-mono text-muted-foreground">{shortcut.combo}</kbd>
      </div>
    {/each}
  </div>
</section>
