<script lang="ts">
  /**
   * InstalledModelsCard - the on-disk models manager section.
   *
   * Lists files found under the models directory (`list_installed_models`) with
   * a guarded delete: an `in_use` model has its delete disabled, and deleting
   * routes through the parent's ConfirmDialog (never an implicit removal).
   */
  import { t } from "svelte-i18n";
  import { HardDrive, Trash2 } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import SettingsSection from "../SettingsSection.svelte";
  import type { InstalledModel } from "$lib/ipc/modelHub";
  import { formatSizeBytes } from "./helpers";

  interface Props {
    models: InstalledModel[];
    loading: boolean;
    onRequestDelete: (model: InstalledModel) => void;
  }

  let { models, loading, onRequestDelete }: Props = $props();

  const totalBytes = $derived(models.reduce((sum, m) => sum + m.size_bytes, 0));
</script>

<SettingsSection
  title={$t("settings.model_hub.installed.label")}
  description={$t("settings.model_hub.installed.help")}
  data-testid="installed-models-section"
>
  {#snippet icon()}<HardDrive size={15} strokeWidth={1.75} />{/snippet}
  {#snippet actions()}
    {#if !loading && models.length > 0}
      <span class="text-caption tabular-nums text-muted-foreground" data-testid="installed-models-total">
        {$t("settings.model_hub.installed.total", { values: { size: formatSizeBytes(totalBytes) } })}
      </span>
    {/if}
  {/snippet}

  {#if loading}
    <div class="flex items-center gap-2 text-body-sm text-muted-foreground">
      <Spinner class="h-4 w-4" />
      {$t("common.loading")}
    </div>
  {:else if models.length === 0}
    <p class="text-body-sm text-muted-foreground" data-testid="installed-models-empty">
      {$t("settings.model_hub.installed.empty")}
    </p>
  {:else}
    <div class="flex flex-col divide-y divide-border/60" data-testid="installed-models-list">
      {#each models as model (model.path)}
        <div
          class="flex items-center gap-3 py-2.5"
          data-testid="installed-model-row-{model.name}"
        >
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate font-mono text-body-xs text-foreground" title={model.path}>{model.name}</span>
              <span class="rounded bg-surface-3 px-1.5 py-0.5 text-overline uppercase text-muted-foreground">{model.kind}</span>
              {#if model.in_use}
                <Badge variant="primary" title={model.used_by ?? ""} data-testid="installed-model-inuse-{model.name}">
                  {$t("settings.model_hub.installed.in_use")}
                </Badge>
              {/if}
            </div>
            <div class="text-caption tabular-nums text-muted-foreground" data-testid="installed-model-size-{model.name}">
              {formatSizeBytes(model.size_bytes)}
            </div>
          </div>
          <Button
            size="icon-sm"
            variant="ghost"
            class="text-destructive hover:bg-destructive/10"
            disabled={model.in_use}
            title={model.in_use ? (model.used_by ?? "") : $t("settings.model_hub.installed.delete")}
            aria-label={$t("settings.model_hub.installed.delete")}
            onclick={() => onRequestDelete(model)}
            data-testid="installed-model-delete-{model.name}"
          >
            <Trash2 size={14} />
          </Button>
        </div>
      {/each}
    </div>
  {/if}
</SettingsSection>
