<script lang="ts">
  /**
   * ModelResultCard - one HuggingFace repo in the results list.
   *
   * A collapsible card: the header (badges + download / GGUF-count meta) is
   * always visible; expanding lazily loads the repo detail and reveals the
   * recommended-parameters strip, the grouped quantization variants, and any
   * vision projector files. Right-click opens a context menu of real actions.
   *
   * The expand mechanic mirrors the shared `Disclosure` pattern (slide + `rm`,
   * rotating chevron, `aria-expanded`) inline, because this card also needs a
   * toggle callback (single-open coordination + lazy detail load), a
   * context-menu trigger, and a `data-testid` on the trigger itself.
   */
  import { t } from "svelte-i18n";
  import { slide } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import {
    ChevronDown,
    Key,
    Package,
    Info,
    ShieldAlert,
    Download,
    Copy,
    ExternalLink,
    Heart,
  } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import ContextMenu from "$lib/components/ui/context-menu/ContextMenu.svelte";
  import type { ActionMenuItem } from "$lib/components/ui/action-menu";
  import { duration, rm } from "$lib/design/motion";
  import { openExternalUrl } from "$lib/utils/externalLink";
  import { addToast } from "$lib/components/ui/toast";
  import type { DownloadProgress, HfFile, HfModelCard } from "$lib/ipc/modelHub";
  import { groupGgufFiles, groupIsDownloading, isBlockingIssue } from "./helpers";
  import ModelVariantRow from "./ModelVariantRow.svelte";

  interface Props {
    model: HfModelCard;
    expanded: boolean;
    detail: HfModelCard | null;
    detailLoading: boolean;
    downloads: DownloadProgress[];
    onToggle: (repoId: string) => void;
    onDownloadGroup: (group: import("./helpers").GgufGroup) => void;
    onDownloadFile: (file: HfFile) => void;
  }

  let {
    model,
    expanded,
    detail,
    detailLoading,
    downloads,
    onToggle,
    onDownloadGroup,
    onDownloadFile,
  }: Props = $props();

  const dimmed = $derived(isBlockingIssue(model.compatibility_issue));
  const grouped = $derived(detail ? groupGgufFiles(detail.gguf_files) : null);

  const menuItems: ActionMenuItem[] = $derived([
    {
      id: "open-hf",
      label: $t("settings.model_hub.ctx.open_hf"),
      icon: ExternalLink,
      onclick: () => openExternalUrl(`https://huggingface.co/${model.repo_id}`),
    },
    {
      id: "copy-repo",
      label: $t("settings.model_hub.ctx.copy_repo"),
      icon: Copy,
      onclick: async () => {
        await navigator.clipboard.writeText(model.repo_id);
        addToast($t("settings.model_hub.ctx.copied_toast"), "success");
      },
    },
  ]);
</script>

<ContextMenu items={menuItems} data-testid="model-context-menu-{model.repo_id}">
  <div
    class="overflow-hidden rounded-xl border border-border bg-card shadow-sm transition-all duration-base hover:border-primary/40 hover:shadow-elev-2"
    class:opacity-70={dimmed}
  >
    <button
      type="button"
      class="flex w-full items-center gap-3 p-4 text-left outline-none transition-colors hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60"
      aria-expanded={expanded}
      onclick={() => onToggle(model.repo_id)}
      data-testid="model-card-{model.repo_id}"
    >
      <div class="min-w-0 flex-1">
        <div class="flex flex-wrap items-center gap-2">
          <span class="truncate text-body-sm font-semibold text-foreground">{model.repo_id}</span>
          {#if model.license}
            <Badge variant="neutral">{model.license}</Badge>
          {/if}
          {#if model.gated}
            <Badge variant="warning">
              {#snippet icon()}<Key size={11} aria-hidden="true" />{/snippet}
              {$t("settings.model_hub.card.gated")}
            </Badge>
          {/if}
          {#if model.compatibility_issue === "embedding_model"}
            <Badge variant="danger" title={$t("settings.model_hub.card.embedding_only_title")}>
              {#snippet icon()}<ShieldAlert size={11} aria-hidden="true" />{/snippet}
              {$t("settings.model_hub.card.embedding_only")}
            </Badge>
          {:else if model.compatibility_issue === "no_gguf_files"}
            <Badge variant="danger" title={$t("settings.model_hub.card.no_gguf_title")}>
              {#snippet icon()}<ShieldAlert size={11} aria-hidden="true" />{/snippet}
              {$t("settings.model_hub.card.no_gguf")}
            </Badge>
          {:else if model.compatibility_issue === "unknown_architecture"}
            <Badge variant="warning" title={$t("settings.model_hub.card.arch_unknown_title")}>
              {#snippet icon()}<ShieldAlert size={11} aria-hidden="true" />{/snippet}
              {$t("settings.model_hub.card.arch_unknown")}
            </Badge>
          {/if}
        </div>
        <div class="mt-1.5 flex flex-wrap items-center gap-4 text-caption text-muted-foreground">
          <span class="inline-flex items-center gap-1 tabular-nums">
            <Download size={12} aria-hidden="true" />
            {$t("settings.model_hub.card.downloads_count", { values: { count: (model.downloads / 1000).toFixed(0) } })}
          </span>
          {#if model.likes > 0}
            <span class="inline-flex items-center gap-1 tabular-nums">
              <Heart size={12} aria-hidden="true" />{model.likes}
            </span>
          {/if}
          {#if model.gguf_files.length > 0}
            <span class="inline-flex items-center gap-1">
              <Package size={12} aria-hidden="true" />
              {$t("settings.model_hub.card.gguf_files_count", { values: { count: model.gguf_files.length } })}
            </span>
          {/if}
        </div>
      </div>
      <ChevronDown
        size={16}
        class="shrink-0 text-muted-foreground transition-transform duration-base"
        style={expanded ? "transform: rotate(180deg);" : ""}
        aria-hidden="true"
      />
    </button>

    {#if expanded}
      <div class="border-t border-border/60 bg-surface-1/50 p-4" transition:slide={rm({ duration: duration.slow, easing: quintOut })}>
        {#if detailLoading}
          <div class="flex items-center gap-2 text-body-sm text-muted-foreground">
            <Spinner class="h-4 w-4" />
            {$t("settings.model_hub.detail.loading_files")}
          </div>
        {:else if detail && detail.repo_id === model.repo_id}
          {#if detail.generation_config}
            {@const cfg = detail.generation_config}
            <div class="mb-4 flex flex-wrap gap-x-6 gap-y-1 rounded-md bg-muted/40 px-3 py-2.5">
              <span class="w-full text-overline uppercase text-muted-foreground">{$t("settings.model_hub.detail.recommended_params")}</span>
              {#if cfg.temperature !== undefined}<span class="text-caption text-muted-foreground">{$t("settings.model_hub.detail.param_temperature")}<span class="ml-1.5 font-mono text-foreground">{cfg.temperature}</span></span>{/if}
              {#if cfg.top_p !== undefined}<span class="text-caption text-muted-foreground">{$t("settings.model_hub.detail.param_top_p")}<span class="ml-1.5 font-mono text-foreground">{cfg.top_p}</span></span>{/if}
              {#if cfg.top_k !== undefined}<span class="text-caption text-muted-foreground">{$t("settings.model_hub.detail.param_top_k")}<span class="ml-1.5 font-mono text-foreground">{cfg.top_k}</span></span>{/if}
              {#if cfg.repetition_penalty !== undefined}<span class="text-caption text-muted-foreground">{$t("settings.model_hub.detail.param_rep_penalty")}<span class="ml-1.5 font-mono text-foreground">{cfg.repetition_penalty}</span></span>{/if}
              {#if cfg.max_new_tokens !== undefined}<span class="text-caption text-muted-foreground">{$t("settings.model_hub.detail.param_max_tokens")}<span class="ml-1.5 font-mono text-foreground">{cfg.max_new_tokens}</span></span>{/if}
            </div>
          {/if}

          {#if grouped && grouped.models.length > 0}
            <div class="mb-2 text-caption font-medium text-muted-foreground">
              {grouped.models.length === 1
                ? $t("settings.model_hub.detail.variants_one", { values: { count: grouped.models.length } })
                : $t("settings.model_hub.detail.variants_other", { values: { count: grouped.models.length } })}
            </div>
            <div class="flex flex-col gap-1.5">
              {#each grouped.models as group (group.quantName + group.fullName)}
                <ModelVariantRow
                  {group}
                  downloading={groupIsDownloading(group, downloads)}
                  onDownload={onDownloadGroup}
                />
              {/each}
            </div>

            {#if grouped.projectors.length > 0}
              <div class="mt-3 rounded-md border border-dashed border-border/60 p-2.5">
                <div class="mb-1.5 flex items-center gap-1.5 text-caption text-muted-foreground">
                  <Info size={12} class="shrink-0" aria-hidden="true" />
                  {$t("settings.model_hub.detail.vision_projector")}
                </div>
                {#each grouped.projectors as f (f.filename)}
                  <div class="flex items-center gap-2 py-1 text-caption">
                    <span class="flex-1 truncate font-mono text-foreground">{f.filename.split("/").pop()}</span>
                    <span class="tabular-nums text-muted-foreground">{f.size_human || "-"}</span>
                    <Button
                      variant="ghost"
                      size="sm"
                      class="h-7 px-2 text-caption"
                      onclick={() => onDownloadFile(f)}
                      data-testid="model-projector-download-{f.filename}"
                    >
                      {#snippet icon()}<Download size={13} />{/snippet}
                      {$t("settings.model_hub.detail.download")}
                    </Button>
                  </div>
                {/each}
              </div>
            {/if}
          {:else}
            <div class="text-body-sm text-muted-foreground">{$t("settings.model_hub.detail.no_gguf_in_repo")}</div>
          {/if}
        {/if}
      </div>
    {/if}
  </div>
</ContextMenu>
