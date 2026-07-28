<script lang="ts">
  /**
   * ConfigFileCard - one read-only [section] of apollia.toml, rendered as an
   * expandable EntityCard.
   *
   * Collapsed it shows the section name, description, and an OK / Missing badge.
   * Expanded it reveals the shared config-file path (Copy / Reveal in Finder),
   * then one of four content branches:
   *   - file missing        -> a calm "defaults apply" hint (no error);
   *   - a `redirect_route`   -> a "See details" jump to the dedicated view;
   *   - key/value `entries`  -> a two-column grid + a raw TOML disclosure
   *                             (the chat `.tb-code` idiom, code-sm scale);
   *   - otherwise            -> an "empty section" line.
   *
   * Read-only: no inline editing here. Editing apollia.toml happens in an
   * external editor, then a runtime restart.
   */
  import { t } from "svelte-i18n";
  import {
    ChevronRight,
    Copy,
    ExternalLink,
    CheckCircle2,
    FileX,
    Sliders,
    FolderOpen,
    AlertCircle,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import { addToast } from "$lib/components/ui/toast";
  import { Disclosure } from "$lib/components/ui/disclosure";
  import { EntityCard } from "$lib/components/operator";
  import { navigateTo } from "$lib/stores/navigation";
  import { revealPath, isRevealablePath } from "$lib/utils/fileRef";
  import type { ConfigSection } from "$lib/types";

  interface Props {
    section: ConfigSection;
    filePath?: string;
    fileExists: boolean;
    defaultExpanded?: boolean;
  }

  let { section, filePath, fileExists, defaultExpanded = false }: Props = $props();

  // Seed only: `defaultExpanded` sets the initial state; user clicks own it after.
  // svelte-ignore state_referenced_locally
  let expanded = $state(defaultExpanded);

  const revealable = $derived(!!filePath && isRevealablePath(filePath));

  const rawValue = $derived(
    section.entries.map((entry) => `${entry.key} = ${entry.value}`).join("\n"),
  );

  function toggle(): void {
    expanded = !expanded;
  }

  // Inner controls sit inside the card's own onclick region, so each one stops
  // the click from bubbling up and toggling the card.
  async function copyPath(event: MouseEvent): Promise<void> {
    event.stopPropagation();
    if (!filePath) return;
    try {
      await navigator.clipboard.writeText(filePath);
      addToast($t("settings.config.path_copied"), "success");
    } catch {
      addToast($t("settings.config.copy_failed"), "error");
    }
  }

  async function reveal(event: MouseEvent): Promise<void> {
    event.stopPropagation();
    if (filePath) await revealPath(filePath);
  }

  function gotoRedirect(event: MouseEvent): void {
    event.stopPropagation();
    if (section.redirect_route) {
      navigateTo(section.redirect_route as Parameters<typeof navigateTo>[0]);
    }
  }

  function toggleFromChevron(event: MouseEvent): void {
    event.stopPropagation();
    toggle();
  }
</script>

<EntityCard
  accent={fileExists ? "success" : "warning"}
  iconTone={fileExists ? "success" : "warning"}
  title={section.name}
  subtitle={section.description}
  onclick={toggle}
  data-testid="config-file-card-{section.name}"
>
  {#snippet icon()}
    {#if fileExists}
      <Sliders size={16} />
    {:else}
      <FileX size={16} />
    {/if}
  {/snippet}

  {#snippet badges()}
    {#if fileExists}
      <Badge variant="success" size="sm">
        <CheckCircle2 size={12} class="mr-1" />
        {$t("settings.config.status_exists")}
      </Badge>
    {:else}
      <Badge variant="warning" size="sm">
        <FileX size={12} class="mr-1" />
        {$t("settings.config.status_missing")}
      </Badge>
    {/if}
  {/snippet}

  {#snippet trailing()}
    <button
      type="button"
      onclick={toggleFromChevron}
      aria-expanded={expanded}
      aria-label={$t("settings.config.toggle_section")}
      class="inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground/60 transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
    >
      <ChevronRight
        size={16}
        class="transition-transform duration-base"
        style={expanded ? "transform: rotate(90deg);" : ""}
        aria-hidden="true"
      />
    </button>
  {/snippet}

  {#snippet body()}
    {#if expanded}
      <div class="space-y-3 pt-1">
        {#if filePath}
          <div class="flex flex-wrap items-center justify-between gap-2">
            <code
              class="min-w-0 flex-1 truncate font-mono text-code-sm text-muted-foreground"
              title={filePath}>{filePath}</code
            >
            <div class="flex flex-none items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onclick={copyPath}
                data-testid="config-file-card-copy-{section.name}"
              >
                {#snippet icon()}<Copy size={13} />{/snippet}
                {$t("settings.config.copy_path")}
              </Button>
              {#if revealable}
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={reveal}
                  data-testid="config-file-card-reveal-{section.name}"
                >
                  {#snippet icon()}<FolderOpen size={13} />{/snippet}
                  {$t("settings.config.reveal_in_finder")}
                </Button>
              {/if}
            </div>
          </div>
        {/if}

        {#if !fileExists}
          <div
            class="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning/5 px-3 py-2 text-body-sm text-warning-a11y"
          >
            <AlertCircle size={15} class="mt-px shrink-0" />
            <span>{$t("settings.config.missing_hint")}</span>
          </div>
        {:else if section.redirect_route}
          <Button
            variant="outline"
            size="sm"
            onclick={gotoRedirect}
            data-testid="config-file-card-redirect-{section.name}"
          >
            {#snippet icon()}<ExternalLink size={13} />{/snippet}
            {$t("settings.see_details")}
          </Button>
        {:else if section.entries.length > 0}
          <div
            class="grid grid-cols-3 gap-x-4 gap-y-1"
            data-testid="config-file-card-entries-{section.name}"
          >
            {#each section.entries as entry (entry.key)}
              <span class="text-body-sm text-muted-foreground">{entry.key}</span>
              <span class="col-span-2 break-all font-mono text-code-sm text-foreground"
                >{entry.value}</span
              >
            {/each}
          </div>

          <Disclosure testid="config-file-card-raw-{section.name}">
            {#snippet summary()}
              <span class="text-body-sm">{$t("settings.config.show_raw")}</span>
            {/snippet}
            {#snippet children()}
              <pre class="tb-code font-mono text-code-sm"><code>{rawValue}</code></pre>
            {/snippet}
          </Disclosure>
        {:else}
          <p class="text-body-sm italic text-muted-foreground">
            {$t("settings.config.empty")}
          </p>
        {/if}
      </div>
    {/if}
  {/snippet}
</EntityCard>
