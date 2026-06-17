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
  import { Button } from "$lib/components/ui/button";
  import { EntityCard } from "$lib/components/operator";

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

<EntityCard
  title={provider.name}
  data-testid="provider-card-{provider.id}"
  data-provider-type={provider.provider_type}
>
  {#snippet icon()}<IconCmp size={16} />{/snippet}

  {#snippet badges()}
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
  {/snippet}

  {#snippet trailing()}
    <Toggle
      checked={togglingEnabled}
      loading={toggleBusy}
      onchange={handleToggle}
      aria-label={$t("projects.context_enabled_label")}
      data-testid="provider-toggle-{provider.id}"
    />
  {/snippet}

  {#snippet body()}
    <p class="m-0 mt-1 text-[11.5px] text-muted-foreground leading-[1.5]">
      {description}
    </p>
    {#if provider.path}
      <p class="m-0 mt-1 text-[10.5px] text-muted-foreground font-mono truncate">
        {provider.path}
      </p>
    {/if}
  {/snippet}

  {#snippet actions()}
    <Button variant="ghost" size="sm"
      type="button"
      onclick={() => onedit(provider)}
      class="h-7 w-7 inline-flex items-center justify-center rounded-md bg-transparent border border-border text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
      title={$t("common.edit")}
      data-testid="provider-edit-{provider.id}"
    >
      <Pencil size={12} />
    </Button>
    <Button variant="ghost" size="sm"
      type="button"
      onclick={() => ondelete(provider)}
      class="h-7 w-7 inline-flex items-center justify-center rounded-md bg-transparent border border-border text-muted-foreground hover:bg-destructive/10 hover:text-danger-a11y transition-colors"
      title={$t("common.delete")}
      data-testid="provider-delete-{provider.id}"
    >
      <Trash2 size={12} />
    </Button>
  {/snippet}
</EntityCard>
