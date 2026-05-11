<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import {
    GitBranch,
    FolderTree,
    ScrollText,
    Terminal,
    Palette,
    Pencil,
    Trash2,
    HelpCircle,
    type Icon,
  } from "lucide-svelte";
  import { Toggle } from "$lib/components/ui/toggle";
  import { addToast } from "$lib/components/ui/toast/store";
  import type { ProjectProviderRow } from "$lib/types";

  interface Props {
    provider: ProjectProviderRow;
    onedit: (row: ProjectProviderRow) => void;
    ondelete: (row: ProjectProviderRow) => void;
    onchanged?: () => void;
  }

  let { provider, onedit, ondelete, onchanged }: Props = $props();

  const ICONS: Record<string, typeof Icon> = {
    git: GitBranch,
    tree: FolderTree,
    rules: ScrollText,
    script: Terminal,
    style: Palette,
  };

  const IconCmp = $derived(ICONS[provider.provider_type] ?? HelpCircle);

  const typeKey = $derived(
    ["git", "tree", "rules", "script", "style"].includes(provider.provider_type)
      ? provider.provider_type
      : null,
  );

  const description = $derived(
    typeKey
      ? $t(`projects.provider_type_${typeKey}_desc`)
      : $t("projects.provider_unknown_desc", {
          values: { type: provider.provider_type },
        }),
  );

  // Mirrors `provider.enabled` for optimistic UI; the `$effect` below keeps
  // it in sync with the prop whenever the parent reloads the row.
  let togglingEnabled = $state(false);
  let toggleBusy = $state(false);

  $effect(() => {
    togglingEnabled = provider.enabled;
  });

  async function handleToggle(next: boolean): Promise<void> {
    toggleBusy = true;
    try {
      await invoke("toggle_project_provider", {
        providerId: provider.id,
        enabled: next,
      });
      onchanged?.();
    } catch (err) {
      // Rollback local state on failure.
      togglingEnabled = !next;
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      toggleBusy = false;
    }
  }
</script>

<div
  class="flex items-start gap-3 px-4 py-3 rounded-xl border border-border bg-card"
  data-testid="provider-card-{provider.id}"
  data-provider-type={provider.provider_type}
>
  <div class="shrink-0 w-9 h-9 rounded-lg inline-flex items-center justify-center bg-primary/10 text-primary">
    <IconCmp size={15} />
  </div>

  <div class="flex-1 min-w-0">
    <div class="flex items-center gap-2">
      <span class="text-[13px] font-semibold text-foreground truncate">{provider.name}</span>
      <span
        class="text-[9.5px] font-mono uppercase tracking-[1px] px-1.5 py-px rounded border border-border text-muted-foreground"
      >
        {provider.provider_type}
      </span>
      <span
        class="text-[9.5px] font-mono px-1.5 py-px rounded bg-surface-1 text-muted-foreground"
        title={$t("projects.context_priority_label")}
      >
        P{provider.priority}
      </span>
      {#if !provider.enabled}
        <span class="text-[9.5px] font-medium text-muted-foreground italic">
          {$t("projects.provider_disabled_label")}
        </span>
      {/if}
    </div>
    <p class="m-0 mt-1 text-[11.5px] text-muted-foreground leading-[1.5]">
      {description}
    </p>
    {#if provider.path}
      <p class="m-0 mt-1 text-[10.5px] text-muted-foreground font-mono truncate">
        {provider.path}
      </p>
    {/if}
  </div>

  <div class="flex items-center gap-2 shrink-0">
    <Toggle
      checked={togglingEnabled}
      loading={toggleBusy}
      onchange={handleToggle}
      aria-label={$t("projects.context_enabled_label")}
      data-testid="provider-toggle-{provider.id}"
    />
    <button
      type="button"
      onclick={() => onedit(provider)}
      class="h-7 w-7 inline-flex items-center justify-center rounded-md bg-transparent border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
      title={$t("common.edit")}
      data-testid="provider-edit-{provider.id}"
    >
      <Pencil size={12} />
    </button>
    <button
      type="button"
      onclick={() => ondelete(provider)}
      class="h-7 w-7 inline-flex items-center justify-center rounded-md bg-transparent border border-border text-muted-foreground hover:bg-destructive/10 hover:text-danger-a11y transition-colors"
      title={$t("common.delete")}
      data-testid="provider-delete-{provider.id}"
    >
      <Trash2 size={12} />
    </button>
  </div>
</div>
