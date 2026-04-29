<script lang="ts">
  /**
   * Template gallery route.
   *
   * Three tabs scope the grid to automations / agents / pipelines. A
   * dedicated filters column on the left persists to the URL query so the
   * page survives reloads and can be deep-linked from empty states or the
   * wizard's "browse templates" CTA.
   *
   * Clicking "Use this template" dispatches on `kind` and opens the
   * corresponding creation surface pre-filled. Actual duplication of the
   * TOML body into user space is delegated to the existing wizards (they
   * already own the "create" path) — here we just hand the body over via
   * a window-level event that the wizards listen to on mount.
   */
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import { TabBar } from "$lib/components/ui/tabs";
  import { addToast } from "$lib/components/ui/toast/store";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { currentRoute } from "$lib/stores/navigation";
  import {
    listTemplates,
    instantiateTemplate,
    type TemplateCategory,
    type TemplateDifficulty,
    type TemplateKind,
    type TemplateMeta,
    type TemplateSource,
  } from "$lib/templates/registry";
  import { showCommunityTemplates } from "$lib/templates/preferences";
  import TemplateCard from "../components/templates/TemplateCard.svelte";
  import TemplateFilters from "../components/templates/TemplateFilters.svelte";
  import TemplateDetailSheet from "../components/templates/TemplateDetailSheet.svelte";

  let templates = $state<TemplateMeta[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let detailTarget = $state<TemplateMeta | null>(null);

  // URL-persisted filters — the first `readUrl()` primes them, then every
  // mutation pushes back via `writeUrl()`.
  let activeTab = $state<TemplateKind>("automation");
  let search = $state("");
  let categories = $state<Set<TemplateCategory>>(new Set());
  let difficulties = $state<Set<TemplateDifficulty>>(new Set());
  let sources = $state<Set<TemplateSource>>(new Set());

  function readUrl() {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    const tab = params.get("kind") as TemplateKind | null;
    if (tab === "automation" || tab === "agent") {
      activeTab = tab;
    }
    search = params.get("q") ?? "";
    const cats = params.get("cat");
    if (cats) categories = new Set(cats.split(",") as TemplateCategory[]);
    const diffs = params.get("diff");
    if (diffs) difficulties = new Set(diffs.split(",") as TemplateDifficulty[]);
    const srcs = params.get("src");
    if (srcs) sources = new Set(srcs.split(",") as TemplateSource[]);
  }

  function writeUrl() {
    if (typeof window === "undefined") return;
    const params = new URLSearchParams(window.location.search);
    params.set("kind", activeTab);
    if (search) params.set("q", search);
    else params.delete("q");
    if (categories.size) params.set("cat", [...categories].join(","));
    else params.delete("cat");
    if (difficulties.size) params.set("diff", [...difficulties].join(","));
    else params.delete("diff");
    if (sources.size) params.set("src", [...sources].join(","));
    else params.delete("src");
    const url = new URL(window.location.href);
    url.search = params.toString();
    window.history.replaceState({}, "", url.toString());
  }

  function handleFiltersChange() {
    writeUrl();
  }
  function resetFilters() {
    search = "";
    categories = new Set();
    difficulties = new Set();
    sources = new Set();
    writeUrl();
  }

  function setTab(kind: string) {
    activeTab = kind as TemplateKind;
    writeUrl();
  }

  onMount(() => {
    readUrl();
    listTemplates()
      .then((list) => {
        templates = list;
      })
      .catch((e) => {
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        loading = false;
      });
  });

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    return templates.filter((tmpl) => {
      if (tmpl.kind !== activeTab) return false;
      // Community templates hidden unless the user opted in — even if a
      // stale URL param sets the source filter.
      if (tmpl.source === "community" && !$showCommunityTemplates) return false;
      if (categories.size > 0 && !categories.has(tmpl.category)) return false;
      if (difficulties.size > 0 && !difficulties.has(tmpl.difficulty)) return false;
      if (sources.size > 0 && !sources.has(tmpl.source)) return false;
      if (q) {
        const haystack = `${tmpl.title} ${tmpl.description}`.toLowerCase();
        if (!haystack.includes(q)) return false;
      }
      return true;
    });
  });

  const counts = $derived.by(() => {
    const result: Record<TemplateKind, number> = { automation: 0, agent: 0 };
    for (const tmpl of templates) {
      if (tmpl.source === "community" && !$showCommunityTemplates) continue;
      result[tmpl.kind]++;
    }
    return result;
  });

  const tabItems = $derived([
    {
      key: "automation",
      label: $t("templates.tabs.automations"),
      count: counts.automation,
    },
    {
      key: "agent",
      label: $t("templates.tabs.agents"),
      count: counts.agent,
    },
  ]);

  async function useTemplate(template: TemplateMeta) {
    try {
      const payload = await instantiateTemplate(template.id);
      // Dispatch a window event — the existing creation surfaces (automation
      // wizard, agent creation dialog, CreatePipelineDialog) listen for it
      // on mount and pre-fill their forms. This avoids a tight coupling
      // between this route and each surface's internal API.
      window.dispatchEvent(
        new CustomEvent("apollia:template_instantiate", { detail: payload }),
      );
      detailTarget = null;
      addToast(
        $t("templates.use_toast", { values: { title: template.title } }),
        "success",
      );
      if (template.kind === "automation") currentRoute.set("automations");
      else currentRoute.set("agents");
    } catch (e) {
      addToast(
        $t("templates.use_error", {
          values: { message: e instanceof Error ? e.message : String(e) },
        }),
        "error",
      );
    }
  }
</script>

<div class="mx-auto w-full max-w-6xl space-y-5" data-testid="templates-page">
  <header>
    <h1 class="text-2xl font-semibold" data-testid="templates-title">
      {$t("templates.title")}
    </h1>
    <p class="mt-0.5 text-sm text-muted-foreground">{$t("templates.subtitle")}</p>
  </header>

  <TabBar
    items={tabItems}
    activeTab={activeTab}
    ontabchange={setTab}
    testidPrefix="templates"
  />

  <div class="grid gap-6 md:grid-cols-[220px_1fr]">
    <aside>
      <TemplateFilters
        bind:search
        bind:categories
        bind:difficulties
        bind:sources
        showCommunity={$showCommunityTemplates}
        onchange={handleFiltersChange}
        onreset={resetFilters}
      />
    </aside>

    <section>
      {#if loading}
        <div class="grid gap-3 sm:grid-cols-2" data-testid="templates-loading">
          {#each Array.from({ length: 4 }) as _}
            <Skeleton class="h-32" />
          {/each}
        </div>
      {:else if error}
        <p class="text-sm text-destructive" data-testid="templates-error">{error}</p>
      {:else if filtered.length === 0}
        <div
          class="rounded-lg border border-dashed border-border/70 px-6 py-10 text-center"
          data-testid="templates-empty"
        >
          <p class="text-sm font-medium">{$t("templates.empty.title")}</p>
          <p class="mt-1 text-xs text-muted-foreground">{$t("templates.empty.description")}</p>
        </div>
      {:else}
        <div class="grid gap-3 sm:grid-cols-2" data-testid="templates-grid">
          {#each filtered as template (template.id)}
            <TemplateCard
              {template}
              onselect={(t) => (detailTarget = t)}
              onuse={useTemplate}
            />
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

<TemplateDetailSheet
  template={detailTarget}
  onclose={() => (detailTarget = null)}
  onuse={useTemplate}
/>
