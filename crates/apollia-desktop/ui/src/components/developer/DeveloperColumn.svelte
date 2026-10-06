<script lang="ts">
  /**
   * The column of the developer screen: its two views, and the way to the
   * settings that keep the developer mode switch and the MCP bridge.
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
  <aside
    class="flex w-60 shrink-0 flex-col border-r border-border"
    data-testid="developer-column"
  >
    <h1 class="px-4 pb-2 pt-4 text-sm font-semibold text-foreground">{$t("nav.developer")}</h1>
    <nav class="flex flex-col gap-0.5 px-2" aria-label={$t("nav.developer")}>
      {#each DEVELOPER_VIEWS as entry (entry.view)}
        {@const EntryIcon = ICONS[entry.view]}
        <button
          type="button"
          class={cn(
            "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60",
            view === entry.view
              ? "bg-muted font-medium text-foreground"
              : "text-muted-foreground hover:bg-muted hover:text-foreground",
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
    <div class="mt-auto flex flex-col items-start gap-1 p-4">
      <p class="text-xs text-muted-foreground">{$t("dev.developer.settings_hint")}</p>
      <button
        type="button"
        class="rounded-sm text-xs text-primary underline underline-offset-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
        onclick={() => openBridgeSettings(openSettingsWindow)}
        data-testid="developer-settings-link"
      >
        {$t("dev.developer.settings_link")}
      </button>
    </div>
  </aside>
{/if}
