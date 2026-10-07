<script lang="ts">
  /**
   * The documentation pane of the developer screen: a page of the archive
   * compiled into the application, with its breadcrumb, the page before and
   * the page after, "Copy as Markdown" and "View on the web". A page of the
   * HTTP reference is generated from the OpenAPI document: it has no source
   * to copy, and says so in place of the copy button.
   */
  import { t } from "svelte-i18n";
  import { ArrowLeft, ArrowRight, ChevronRight, CircleHelp, Copy, ExternalLink } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Tooltip } from "$lib/components/ui/tooltip";
  import { developerMode } from "$lib/stores/developerMode";
  import {
    ADMONITION_LABELS,
    breadcrumbOf,
    firstRoute,
    neighbours,
    type AdmonitionKind,
  } from "./documentation";
  import {
    WEB_BUTTON_ATTR,
    decorateAdmonitions,
    decorateDiagrams,
    decorateHeadings,
    decorateImages,
    revealAnchor,
  } from "./documentationDecor";
  import { documentationReader } from "./documentationReader";
  import HttpReferencePage from "./HttpReferencePage.svelte";
  import "./documentation-prose.css";

  const manifest = $derived($documentationReader.manifest);
  const route = $derived($documentationReader.route);
  const page = $derived($documentationReader.page);
  const view = $derived($documentationReader.view);
  const reference = $derived($documentationReader.reference);
  const anchor = $derived($documentationReader.anchor);
  const failed = $derived($documentationReader.failed);
  const fallback = $derived($documentationReader.fallback);
  const copy = $derived($documentationReader.copy);

  const tree = $derived(manifest?.tree ?? []);
  const crumbs = $derived(
    route === null || route === firstRoute(tree) ? [] : breadcrumbOf(tree, route),
  );
  const around = $derived(
    manifest && route !== null ? neighbours(manifest, route) : { previous: null, next: null },
  );
  const admonitionLabel = $derived((kind: AdmonitionKind) => $t(ADMONITION_LABELS[kind]));
  const diagramLabel = $derived($t("dev.developer.docs.diagram_on_web"));

  function open(target: string): void {
    void documentationReader.open(target);
  }

  // What the pane grafts onto the sanitized page, after every render and
  // whenever the heading to show changes.
  function decorate(root: HTMLElement): void {
    decorateHeadings(root);
    decorateAdmonitions(root, admonitionLabel);
    decorateDiagrams(root, diagramLabel);
    decorateImages(root, documentationReader.imageUrl);
    revealAnchor(root, anchor);
  }

  function onContentClick(event: MouseEvent): void {
    const origin = event.target as Element | null;
    if (origin?.closest?.(`[${WEB_BUTTON_ATTR}]`)) void documentationReader.viewOnWeb();
  }
</script>

{#if $developerMode}
  <section
    class="flex min-h-0 flex-1 flex-col overflow-y-auto"
    aria-labelledby="developer-documentation-title"
    data-testid="developer-pane-documentation"
    data-doc-scroll
  >
    <div class="mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4 p-6">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <nav aria-label={$t("dev.developer.docs.breadcrumb_label")} data-testid="developer-docs-breadcrumb">
          <ol class="flex flex-wrap items-center gap-1 text-caption text-muted-foreground">
            <li>{$t("dev.developer.views.documentation")}</li>
            {#each crumbs as step, index (index)}
              <li class="flex items-center gap-1">
                <ChevronRight size={12} aria-hidden="true" />
                {#if step.route !== null && index < crumbs.length - 1}
                  {@const target = step.route}
                  <button
                    type="button"
                    class="rounded-sm hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                    onclick={() => open(target)}
                    data-testid="developer-docs-breadcrumb-step"
                  >
                    {step.label}
                  </button>
                {:else}
                  <span aria-current={index === crumbs.length - 1 ? "page" : undefined}>
                    {step.label}
                  </span>
                {/if}
              </li>
            {/each}
          </ol>
        </nav>
        <div class="flex shrink-0 items-center gap-2">
          {#if reference}
            <Tooltip
              content={$t("dev.developer.docs.reference.copy_absent")}
              side="bottom"
              class="max-w-xs"
              triggerLabel={$t("dev.developer.docs.reference.copy_absent")}
              triggerTestid="developer-docs-copy-absent"
              triggerClass="shrink-0 cursor-help rounded-full text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
            >
              <CircleHelp class="h-4 w-4" aria-hidden="true" />
            </Tooltip>
          {:else}
            <Button
              variant="outline"
              size="sm"
              disabled={page === null}
              onclick={() => void documentationReader.copyMarkdown()}
              data-testid="developer-docs-copy"
            >
              <Copy size={14} aria-hidden="true" />
              {$t(copy === "copied" ? "dev.developer.docs.copied" : "dev.developer.docs.copy_markdown")}
            </Button>
          {/if}
          <Button
            variant="outline"
            size="sm"
            disabled={route === null}
            onclick={() => void documentationReader.viewOnWeb()}
            data-testid="developer-docs-web"
          >
            <ExternalLink size={14} aria-hidden="true" />
            {$t("dev.developer.docs.view_on_web")}
          </Button>
        </div>
      </div>
      {#if copy === "failed"}
        <p class="text-caption text-danger-a11y" role="status">
          {$t("dev.developer.docs.copy_failed")}
        </p>
      {/if}

      <div class="flex items-center gap-2">
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={around.previous === null}
          aria-label={$t("dev.developer.docs.previous_page")}
          onclick={() => around.previous && open(around.previous.route)}
          data-testid="developer-docs-back"
        >
          <ArrowLeft size={16} aria-hidden="true" />
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={around.next === null}
          aria-label={$t("dev.developer.docs.next_page")}
          onclick={() => around.next && open(around.next.route)}
          data-testid="developer-docs-forward"
        >
          <ArrowRight size={16} aria-hidden="true" />
        </Button>
        <h2
          id="developer-documentation-title"
          class="min-w-0 text-heading-md font-semibold text-foreground"
          data-testid="developer-docs-title"
        >
          {view?.title ?? $t("dev.developer.views.documentation")}
        </h2>
      </div>

      {#if failed}
        <p class="text-body-sm text-muted-foreground" data-testid="developer-docs-error">
          {$t("dev.developer.docs.error")}
        </p>
      {:else if reference}
        <HttpReferencePage page={reference} onopen={open} />
      {:else if view}
        {#if fallback}
          <p class="text-caption text-muted-foreground" role="note" data-testid="developer-docs-fallback">
            {$t("dev.developer.docs.fallback")}
          </p>
        {/if}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div onclick={onContentClick} data-testid="developer-docs-page" data-route={route}>
          <MarkdownContent
            content={view.body}
            profile="documentation"
            class="apollia-doc text-body-md"
            onlink={(href) => void documentationReader.follow(href)}
            ondecorate={decorate}
          />
        </div>
      {:else}
        <div class="flex flex-col gap-2" aria-busy="true" aria-label={$t("dev.developer.docs.loading")}>
          <Skeleton class="h-5 w-2/3" />
          <Skeleton class="h-4 w-full" />
          <Skeleton class="h-4 w-5/6" />
        </div>
      {/if}

      <div class="mt-auto flex flex-wrap items-center justify-between gap-2 pt-4">
        {#if around.previous}
          {@const previous = around.previous}
          <Button
            variant="outline"
            size="sm"
            onclick={() => open(previous.route)}
            data-testid="developer-docs-previous"
          >
            {$t("dev.developer.docs.previous", { values: { title: previous.title } })}
          </Button>
        {:else}
          <span></span>
        {/if}
        {#if around.next}
          {@const next = around.next}
          <Button
            variant="outline"
            size="sm"
            onclick={() => open(next.route)}
            data-testid="developer-docs-next"
          >
            {$t("dev.developer.docs.next", { values: { title: next.title } })}
          </Button>
        {/if}
      </div>
    </div>
  </section>
{/if}
