<!--
  Slash-command autocomplete dropdown.

  Rendered absolutely above the ChatInput textarea when the user types
  a leading `/`. Keyboard-navigable: ↑ / ↓ moves the selection, Enter /
  Tab confirms, Esc closes.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import type { SlashCommand } from "$lib/chat/slashCommands";

  interface Props {
    commands: SlashCommand[];
    selectedIndex: number;
    onselect: (cmd: SlashCommand) => void;
    onhover: (index: number) => void;
  }

  let { commands, selectedIndex, onselect, onhover }: Props = $props();
</script>

{#if commands.length > 0}
  <div
    class="absolute bottom-full left-0 z-20 mb-1 w-[min(22rem,calc(100%-1rem))] overflow-hidden rounded-lg border border-border/50 bg-card shadow-elev-2"
    role="listbox"
    aria-label={$t("chat.commands.menu_label")}
    data-testid="chat-slash-menu"
  >
    {#each commands as cmd, i (cmd.id)}
      <button
        type="button"
        role="option"
        aria-selected={i === selectedIndex}
        class="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-body-xs hover:bg-muted/60"
        class:bg-muted={i === selectedIndex}
        class:text-foreground={i === selectedIndex}
        onmousedown={(e) => { e.preventDefault(); onselect(cmd); }}
        onmouseenter={() => onhover(i)}
        data-testid="chat-slash-menu-item"
        data-cmd={cmd.id}
      >
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5 font-medium">
            <span class="text-muted-foreground/60">/</span>
            <span>{cmd.name}</span>
            <span class="text-muted-foreground/60">- {$t(cmd.labelKey)}</span>
          </div>
          <div class="truncate text-micro text-muted-foreground/70">
            {$t(cmd.descriptionKey)}
          </div>
        </div>
      </button>
    {/each}
  </div>
{/if}
