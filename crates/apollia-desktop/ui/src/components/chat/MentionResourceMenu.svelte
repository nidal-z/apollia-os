<!--
  MCP resource @-mention picker (user-initiative path).

  Rendered absolutely above the ChatInput textarea when the user types a
  leading `@`. Mirrors SlashCommandMenu: keyboard-navigable (up / down moves the
  selection, Enter / Tab confirms, Esc closes). Selecting a resource pins it to
  the next turn as an explicit system-prefix; nothing is auto-injected.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { Database } from "lucide-svelte";
  import type { McpResourceView } from "$lib/chat/mcpResources";

  interface Props {
    resources: McpResourceView[];
    selectedIndex: number;
    loading: boolean;
    onselect: (resource: McpResourceView) => void;
    onhover: (index: number) => void;
  }

  let { resources, selectedIndex, loading, onselect, onhover }: Props = $props();
</script>

<div
  class="absolute bottom-full left-0 z-20 mb-1 w-[min(26rem,calc(100%-1rem))] overflow-hidden rounded-lg border border-border/50 bg-card shadow-elev-2"
  role="listbox"
  aria-label={$t("chat.resources.menu_label")}
  data-testid="chat-mention-menu"
>
  {#if loading}
    <div class="px-3 py-2 text-[11px] text-muted-foreground/70">
      {$t("chat.resources.loading")}
    </div>
  {:else if resources.length === 0}
    <div class="px-3 py-2 text-[11px] text-muted-foreground/70">
      {$t("chat.resources.empty")}
    </div>
  {:else}
    {#each resources as resource, i (resource.server + "::" + resource.uri)}
      <button
        type="button"
        role="option"
        aria-selected={i === selectedIndex}
        class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[12px] hover:bg-muted/60"
        class:bg-muted={i === selectedIndex}
        class:text-foreground={i === selectedIndex}
        onmousedown={(e) => { e.preventDefault(); onselect(resource); }}
        onmouseenter={() => onhover(i)}
        data-testid="chat-mention-menu-item"
        data-uri={resource.uri}
      >
        <Database size={13} class="shrink-0 text-muted-foreground/70" />
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5 font-medium">
            <span class="truncate">{resource.name}</span>
            <span class="shrink-0 text-muted-foreground/60">{resource.server}</span>
          </div>
          <div class="truncate text-[10px] text-muted-foreground/70">
            {resource.description ?? resource.uri}
          </div>
        </div>
      </button>
    {/each}
  {/if}
</div>
