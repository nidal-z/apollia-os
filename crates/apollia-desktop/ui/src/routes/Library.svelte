<script lang="ts">
  /**
   * Library - the installed packages, what each holds, who published it and
   * whether it is signed, with the two gestures on a package: update it from a
   * more recent file, and uninstall it.
   *
   * Thin orchestrator: it owns the list, the open package and the uninstall
   * dialog, reads through `$lib/ipc/library`, and leaves every decision to
   * `$lib/library/libraryState`. A package is installed or updated only from
   * the preview dialog mounted above every screen, which announces it with
   * `PACKAGE_INSTALLED_EVENT`; the list and the open package follow.
   */
  import { t } from "svelte-i18n";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { Package, Search } from "lucide-svelte";
  import { PageLayout, EmptyState, ErrorBanner } from "$lib/components/operator";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { addToast } from "$lib/components/ui/toast/store";
  import { reportError } from "$lib/errors/reportError";
  import { navigateTo } from "$lib/stores/navigation";
  import { requestSheet } from "$lib/stores/agentOpen";
  import { refreshPackages } from "$lib/stores/agentPackages";
  import { PACKAGE_INSTALLED_EVENT, openPackagePreview } from "$lib/stores/packagePreview";
  import {
    getAgentPackageDetail,
    listAgentPackages,
    uninstallAgentPackage,
  } from "$lib/ipc/library";
  import {
    PROVIDED_ENTRY,
    filterEntries,
    libraryEntries,
    packageDetail,
    providedDetail,
    signatureTally,
    workflowTarget,
    type DataChoice,
    type LibraryDetail,
  } from "$lib/library/libraryState";
  import type { AgentPackageListItem } from "$lib/types";
  import LibraryPackageRow from "../components/library/LibraryPackageRow.svelte";
  import LibraryPackageDetail from "../components/library/LibraryPackageDetail.svelte";
  import LibraryUninstallDialog from "../components/library/LibraryUninstallDialog.svelte";

  let packages = $state<AgentPackageListItem[]>([]);
  let loaded = $state(false);
  let loadError = $state<string | null>(null);
  let query = $state("");

  /** Key of the open entry, `null` on the list. */
  let openKey = $state<string | null>(null);
  let detail = $state<LibraryDetail | null>(null);
  let detailError = $state<string | null>(null);

  let uninstalling = $state<LibraryDetail | null>(null);
  let uninstallBusy = $state(false);
  let uninstallError = $state<string | null>(null);

  const entries = $derived(libraryEntries(packages));
  const shown = $derived(filterEntries(entries, query, $t("library.provided.name")));
  const tally = $derived(signatureTally(entries));

  async function loadList(): Promise<void> {
    loadError = null;
    try {
      packages = await listAgentPackages();
    } catch (err) {
      loadError = String(err);
    } finally {
      loaded = true;
    }
  }

  async function loadDetail(key: string): Promise<void> {
    detailError = null;
    try {
      if (key === PROVIDED_ENTRY) {
        const names = packages.filter((p) => p.embedded).map((p) => p.name);
        const details = await Promise.all(names.map((name) => getAgentPackageDetail(name)));
        if (openKey === key) detail = providedDetail(details);
      } else {
        const read = await getAgentPackageDetail(key);
        if (openKey === key) detail = packageDetail(read);
      }
    } catch (err) {
      if (openKey === key) detailError = String(err);
    }
  }

  function openEntry(key: string): void {
    openKey = key;
    detail = null;
    void loadDetail(key);
  }

  function backToList(): void {
    openKey = null;
    detail = null;
    detailError = null;
  }

  $effect(() => {
    void loadList();
  });

  // A package installed or updated from the preview dialog: the list follows,
  // and so does the open package, whose version and content may have changed.
  $effect(() => {
    const refresh = () => {
      void loadList().then(() => {
        if (openKey !== null) void loadDetail(openKey);
      });
    };
    window.addEventListener(PACKAGE_INSTALLED_EVENT, refresh);
    return () => window.removeEventListener(PACKAGE_INSTALLED_EVENT, refresh);
  });

  /** Lets the user choose a package file, then opens its preview. */
  async function choosePackageFile(origin: "files" | "update"): Promise<void> {
    try {
      const path = await openDialog({
        filters: [{ name: "Apollia", extensions: ["apo"] }],
        multiple: false,
      });
      if (typeof path === "string") openPackagePreview(path, origin);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }

  function askUninstall(): void {
    if (detail === null || detail.provided) return;
    uninstallError = null;
    uninstalling = detail;
  }

  async function uninstall(choice: DataChoice): Promise<void> {
    const target = uninstalling;
    if (target === null) return;
    uninstallBusy = true;
    uninstallError = null;
    try {
      const report = await uninstallAgentPackage(target.key, choice === "erase");
      uninstalling = null;
      backToList();
      if (report.leftovers.length > 0) {
        addToast(
          $t("library.uninstall.done_leftovers", {
            values: { name: target.key, count: report.leftovers.length },
          }),
          "warning",
        );
      } else {
        addToast($t("library.uninstall.done", { values: { name: target.key } }), "success");
      }
      await loadList();
      // The Agents screen lists the packages too, from its own store.
      void refreshPackages();
    } catch (err) {
      uninstallError = String(err);
    } finally {
      uninstallBusy = false;
    }
  }
</script>

{#if openKey !== null}
  <div class="mx-auto w-full max-w-6xl" data-testid="library-page">
    {#if detailError}
      <div class="px-8 pt-8">
        <ErrorBanner tone="danger" data-testid="library-detail-error">
          <p class="font-medium">{$t("library.detail.load_error")}</p>
          <p class="break-all opacity-80">{detailError}</p>
        </ErrorBanner>
        <Button variant="outline" size="sm" class="mt-3" onclick={backToList} data-testid="library-detail-error-back">
          {$t("library.detail.back")}
        </Button>
      </div>
    {:else if detail}
      {@const open = detail}
      <LibraryPackageDetail
        detail={open}
        onback={backToList}
        onupdate={() => choosePackageFile("update")}
        onuninstall={askUninstall}
        onopenagent={(id) => requestSheet({ kind: "agent", id })}
        onopenworkflow={(id) => requestSheet({ kind: "workflow", id: workflowTarget(open, id) })}
      />
    {:else}
      <p class="px-8 pt-8 text-caption text-muted-foreground" data-testid="library-detail-loading">
        {$t("common.loading")}
      </p>
    {/if}
  </div>
{:else}
  <PageLayout
    kicker={$t("library.kicker")}
    title={$t("library.title")}
    subtitle={$t("library.subtitle")}
    data-testid="library-page"
  >
    {#snippet actions()}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={() => choosePackageFile("files")}
        data-testid="library-install-button"
      >
        {$t("library.install")}
      </Button>
    {/snippet}

    <div class="flex flex-col gap-4 px-8 py-6">
      {#if loadError}
        <ErrorBanner tone="danger" onretry={loadList} data-testid="library-load-error">
          <p class="font-medium">{$t("library.load_error")}</p>
          <p class="break-all opacity-80">{loadError}</p>
        </ErrorBanner>
      {:else if !loaded}
        <p class="text-caption text-muted-foreground" data-testid="library-loading">{$t("common.loading")}</p>
      {:else if entries.length === 0}
        <div data-testid="library-empty">
          <EmptyState title={$t("library.empty.title")} desc={$t("library.empty.description")} tone="neutral">
            {#snippet icon()}<Package size={24} />{/snippet}
            {#snippet action()}
              <div class="flex flex-wrap items-center justify-center gap-2">
                <Button
                  variant="primary-solid"
                  size="sm"
                  onclick={() => choosePackageFile("files")}
                  data-testid="library-empty-install"
                >
                  {$t("library.install")}
                </Button>
                <Button variant="ghost" size="sm" onclick={() => navigateTo("agents")} data-testid="library-empty-examples">
                  {$t("library.empty.examples")}
                </Button>
              </div>
            {/snippet}
          </EmptyState>
        </div>
      {:else}
        <div class="flex flex-wrap items-center gap-3">
          <div class="min-w-0 flex-1">
            <Input
              bind:value={query}
              icon={Search}
              size="sm"
              placeholder={$t("library.filter_placeholder")}
              aria-label={$t("library.filter_placeholder")}
              data-testid="library-filter"
            />
          </div>
          <p class="text-caption text-muted-foreground" data-testid="library-tally">
            {$t("library.tally", { values: tally })}
          </p>
        </div>

        {#if shown.length === 0}
          <p class="text-caption text-muted-foreground" data-testid="library-no-match">
            {$t("library.no_match", { values: { query: query.trim() } })}
          </p>
        {:else}
          <ul class="flex flex-col gap-2" data-testid="library-list">
            {#each shown as entry (entry.key)}
              <LibraryPackageRow {entry} onopen={openEntry} />
            {/each}
          </ul>
        {/if}

        <p class="text-caption text-muted-foreground" data-testid="library-footnote">
          {$t("library.footnote")}
        </p>
      {/if}
    </div>
  </PageLayout>
{/if}

<LibraryUninstallDialog
  detail={uninstalling}
  busy={uninstallBusy}
  error={uninstallError}
  oncancel={() => {
    if (!uninstallBusy) uninstalling = null;
  }}
  onconfirm={uninstall}
/>
