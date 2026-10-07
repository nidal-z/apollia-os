<script lang="ts">
  /**
   * The developer screen, reached from the developer entry of the sidebar with
   * the developer mode on: the local server and its logs, and the
   * documentation. Each pane is its own component.
   */
  import { developerMode } from "$lib/stores/developerMode";
  import DocumentationTree from "../components/developer/DocumentationTree.svelte";
  import DeveloperColumn from "../components/developer/DeveloperColumn.svelte";
  import DocumentationPane from "../components/developer/DocumentationPane.svelte";
  import LocalServerPane from "../components/developer/LocalServerPane.svelte";
  import LogsPane from "../components/developer/LogsPane.svelte";
  import {
    DEFAULT_DEVELOPER_VIEW,
    panesOf,
    screenVisible,
    type DeveloperView,
  } from "../components/developer/developerScreen";

  let view = $state<DeveloperView>(DEFAULT_DEVELOPER_VIEW);
  const panes = $derived(panesOf(view));
</script>

{#if screenVisible($developerMode)}
  <div class="flex h-full min-h-0 flex-col" data-testid="developer-page">
    <DeveloperColumn {view} onview={(next) => (view = next)} />
    <div class="flex min-h-0 flex-1">
    <!-- The tree of the documentation stands beside its pages only. -->
    <aside class={view === "documentation" ? "flex w-60 shrink-0 flex-col border-r border-border" : "hidden"}>
      <DocumentationTree active={view === "documentation"} onopen={() => (view = "documentation")} />
    </aside>
    <div class="flex min-w-0 flex-1 flex-col">
      {#if panes.includes("server")}
        <LocalServerPane />
      {/if}
      {#if panes.includes("documentation")}
        <DocumentationPane />
      {/if}
      {#if panes.includes("logs")}
        <LogsPane />
      {/if}
    </div>
    </div>
  </div>
{/if}
