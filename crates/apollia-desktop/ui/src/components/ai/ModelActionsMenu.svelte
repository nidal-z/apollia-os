<script lang="ts">
  /**
   * ModelActionsMenu - the "..." menu of a row of the AI screen. An entry that
   * cannot be used stays visible, inactive, with its reason on hover.
   */
  import { t } from "svelte-i18n";
  import {
    Copy,
    ExternalLink,
    FolderOpen,
    Ellipsis,
    Pin,
    PinOff,
    Search,
    Star,
    Trash2,
  } from "lucide-svelte";
  import { ActionMenu } from "$lib/components/ui/action-menu";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import type { InstalledModel } from "$lib/ipc/modelHub";
  import { cn } from "$lib/utils";
  import { menuEntries, type HostPlatform, type MenuEntryId } from "./aiModels";

  interface Props {
    model: InstalledModel;
    platform: HostPlatform;
    onselect: (model: InstalledModel, entry: MenuEntryId) => void;
  }

  let { model, platform, onselect }: Props = $props();

  let open = $state(false);

  const ICONS = {
    reveal: FolderOpen,
    pin: Pin,
    copy_id: Copy,
    copy_path: Copy,
    source: ExternalLink,
    use_default: Star,
    use_search: Search,
    delete: Trash2,
  } as const;

  const entries = $derived(menuEntries(model, platform));
</script>

<ActionMenu bind:open align="end" class="min-w-[15rem]">
  {#snippet triggerSlot(props)}
    <button
      type="button"
      {...props}
      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
      aria-label={$t("ai.menu.more", { values: { name: model.id } })}
      data-testid="ai-model-menu-btn"
    >
      <Ellipsis size={15} strokeWidth={1.75} aria-hidden="true" />
    </button>
  {/snippet}
  {#snippet body({ close })}
    <ul class="flex flex-col gap-0.5" role="menu" data-testid="ai-model-menu">
      {#each entries as entry, i (entry.id)}
        {@const Icon = entry.id === "pin" && model.pinned_at ? PinOff : ICONS[entry.id]}
        {#if i > 0 && entries[i - 1]?.section !== entry.section}
          <li role="separator" class="my-1 h-px bg-border"></li>
        {/if}
        <li role="none">
          {#if entry.reasonKey}
            <Tooltip content={$t(entry.reasonKey)} side="left">
              <span
                role="menuitem"
                aria-disabled="true"
                tabindex="-1"
                class="flex w-full cursor-not-allowed items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left text-body-xs text-foreground opacity-50"
                data-testid="ai-model-menu-{entry.id}"
              >
                <span>{$t(entry.labelKey)}</span>
                <Icon size={14} aria-hidden="true" />
              </span>
            </Tooltip>
          {:else}
            <button
              type="button"
              role="menuitem"
              class={cn(
                "flex w-full items-center justify-between gap-3 rounded-sm px-2 py-1.5 text-left text-body-xs transition-colors",
                entry.destructive
                  ? "text-destructive hover:bg-destructive/10"
                  : "text-foreground hover:bg-muted",
              )}
              onclick={() => {
                close();
                onselect(model, entry.id);
              }}
              data-testid="ai-model-menu-{entry.id}"
            >
              <span>{$t(entry.labelKey)}</span>
              <Icon size={14} aria-hidden="true" />
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  {/snippet}
</ActionMenu>
