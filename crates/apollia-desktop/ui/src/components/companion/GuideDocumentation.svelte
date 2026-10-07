<script lang="ts">
  /**
   * What the Guide tab shows when no model can answer: a calm notice, then
   * the page of the embedded documentation for the current screen, read with
   * no network. "Open in the browser" leaves for the same page of the public
   * site; the input stays in place, disabled, and says why.
   */
  import { t, locale } from "svelte-i18n";
  import { AlertTriangle, BookOpen, ExternalLink } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { MarkdownContent } from "$lib/components/ui/markdown";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { getDocumentationPage, type DocumentationPage } from "$lib/ipc/documentation";
  import { documentationView } from "$lib/apolliaGuide/guideDocumentation";
  import {
    guideDocumentationRoute,
    type GuideContextId,
  } from "$lib/apolliaGuide/screenContexts";
  import { ONBOARDING_GUIDE_ROUTE } from "$lib/stores/guideContext";
  import { navigateTo } from "$lib/stores/navigation";
  import { openExternalUrl } from "$lib/utils/externalLink";

  interface Props {
    /** The context whose page is shown. */
    id: GuideContextId;
  }

  let { id }: Props = $props();

  const firstLaunch = $derived(id === ONBOARDING_GUIDE_ROUTE);

  let page = $state<DocumentationPage | null>(null);
  let failed = $state(false);

  // The page follows the context and the language of the interface. A read
  // that ends after a newer one started is dropped.
  $effect(() => {
    const route = guideDocumentationRoute(id);
    const language = $locale ?? "";
    let current = true;
    page = null;
    failed = false;
    getDocumentationPage(route, language).then(
      (read) => {
        if (current) page = read;
      },
      () => {
        if (current) failed = true;
      },
    );
    return () => {
      current = false;
    };
  });

  const view = $derived(page ? documentationView(page) : null);
</script>

<div class="flex min-h-0 flex-1 flex-col" data-testid="guide-documentation" data-context={id}>
  <div class="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 pb-4">
    <Banner
      variant="neutral"
      surface="card"
      icon={AlertTriangle}
      title={$t(firstLaunch ? "guide.no_model.onboarding_title" : "guide.no_model.title")}
      data-testid="guide-no-model"
    >
      <p>{$t(firstLaunch ? "guide.no_model.onboarding_body" : "guide.no_model.body")}</p>
      {#if !firstLaunch}
        <Button
          variant="outline"
          size="sm"
          class="mt-2"
          onclick={() => navigateTo("llm")}
          data-testid="guide-no-model-action"
        >
          {$t("guide.no_model.action")}
        </Button>
      {/if}
    </Banner>

    <article class="flex flex-col gap-3" data-testid="guide-documentation-page">
      <div class="flex items-center justify-between gap-2">
        <p class="flex min-w-0 items-center gap-1.5 text-caption text-muted-foreground">
          <BookOpen size={14} class="shrink-0" aria-hidden="true" />
          <span class="truncate">
            {$t("guide.doc.breadcrumb")}{view ? ` › ${view.title}` : ""}
          </span>
        </p>
        {#if page}
          {@const address = page.public_url}
          <Button
            variant="outline"
            size="sm"
            class="shrink-0"
            onclick={() => void openExternalUrl(address)}
            data-testid="guide-documentation-open"
          >
            {$t("guide.doc.open_in_browser")}
            <ExternalLink size={14} aria-hidden="true" />
          </Button>
        {/if}
      </div>

      {#if failed}
        <p class="text-body-sm text-muted-foreground" data-testid="guide-documentation-error">
          {$t("guide.doc.error")}
        </p>
      {:else if page && view}
        {#if page.fallback}
          <p class="text-caption text-muted-foreground">{$t("guide.doc.fallback")}</p>
        {/if}
        <h2 class="text-heading-sm text-foreground">{view.title}</h2>
        <MarkdownContent content={view.body} class="text-body-sm" />
      {:else}
        <div class="flex flex-col gap-2" aria-busy="true" aria-label={$t("guide.doc.loading")}>
          <Skeleton class="h-5 w-2/3" />
          <Skeleton class="h-4 w-full" />
          <Skeleton class="h-4 w-5/6" />
        </div>
      {/if}
    </article>
  </div>

  <div class="shrink-0 border-t border-border px-4 py-3">
    <Input
      disabled
      value=""
      placeholder={$t("guide.no_model.field")}
      aria-label={$t("guide.no_model.field")}
      data-testid="guide-input-disabled"
    />
  </div>
</div>
