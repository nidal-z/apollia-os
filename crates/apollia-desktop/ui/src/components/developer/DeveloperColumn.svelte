<script lang="ts">
  /**
   * The column of the developer screen: its two views, the tree of the
   * documentation, and the way to the settings that keep the developer mode
   * switch and the MCP bridge.
   */
  import { t } from "svelte-i18n";
  import { BookOpen, Server } from "lucide-svelte";
  import { cn } from "$lib/utils";
  import { developerMode } from "$lib/stores/developerMode";
  import { openSettingsWindow } from "$lib/stores/settingsWindow";
  import { DEVELOPER_VIEWS, openBridgeSettings, type DeveloperView } from "./developerScreen";

  interface Props {
    /** The view shown. */
    view: DeveloperView;
    /** Called with the view the user chose. */
    onview: (view: DeveloperView) => void;
  }

  let { view, onview }: Props = $props();

  const ICONS = { server: Server, documentation: BookOpen } as const;
</script>

{#if $developerMode}
  <header
    class="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-2 border-b border-border px-6 py-3"
    data-testid="developer-column"
  >
    <h1 class="text-sm font-semibold text-foreground">{$t("nav.developer")}</h1>
    <nav class="flex items-center gap-1" aria-label={$t("nav.developer")}>
      {#each DEVELOPER_VIEWS as entry (entry.view)}
        {@const EntryIcon = ICONS[entry.view]}
        <button
          type="button"
          class={cn(
            "flex items-center gap-2 rounded-full border px-3 py-1 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            view === entry.view
              ? "border-primary/40 bg-primary/5 font-medium text-foreground"
              : "border-transparent text-muted-foreground hover:bg-muted hover:text-foreground",
          )}
          aria-current={view === entry.view ? "page" : undefined}
          onclick={() => onview(entry.view)}
          data-testid={entry.testid}
        >
          <EntryIcon size={15} strokeWidth={1.75} aria-hidden="true" />
          {$t(entry.labelKey)}
        </button>
      {/each}
    </nav>
    <div class="ml-auto flex items-center gap-2">
      <p class="hidden text-xs text-muted-foreground lg:block">{$t("dev.developer.settings_hint")}</p>
      <button
        type="button"
        class="rounded-sm text-xs text-primary underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={() => openBridgeSettings(openSettingsWindow)}
        data-testid="developer-settings-link"
      >
        {$t("dev.developer.settings_link")}
      </button>
    </div>
  </header>
{/if}
