<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "svelte-i18n";
  import {
    RotateCw,
    Search,
    Globe,
    Terminal,
    FolderOpen,
    Brain,
    Network,
    FileCode,
    UserSquare,
  } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import SettingSectionSkeleton from "../../components/settings/SettingSectionSkeleton.svelte";
  import ToolCard from "../../components/settings/ToolCard.svelte";
  import ToolConfigDrawer from "../../components/settings/ToolConfigDrawer.svelte";
  import {
    tools,
    loadingTools,
    toolsError,
    loadTools,
    toggleTool,
    type ToolStatusDto,
  } from "$lib/stores/toolGovernance";
  import { addToast } from "$lib/components/ui/toast";

  interface ToolPresentation {
    titleKey: string;
    descKey: string;
    icon: typeof Search;
    canConfigure: boolean;
  }

  const PRESENTATIONS: Record<string, ToolPresentation> = {
    web_search: {
      titleKey: "settings.tools_page.web_search_title",
      descKey: "settings.tools_page.web_search_desc",
      icon: Search,
      canConfigure: true,
    },
    web_read: {
      titleKey: "settings.tools_page.web_read_title",
      descKey: "settings.tools_page.web_read_desc",
      icon: Globe,
      canConfigure: true,
    },
    bash_executor: {
      titleKey: "settings.tools_page.bash_executor_title",
      descKey: "settings.tools_page.bash_executor_desc",
      icon: Terminal,
      canConfigure: false,
    },
    python_executor: {
      titleKey: "settings.tools_page.python_executor_title",
      descKey: "settings.tools_page.python_executor_desc",
      icon: FileCode,
      canConfigure: false,
    },
    file_read: {
      titleKey: "settings.tools_page.file_read_title",
      descKey: "settings.tools_page.file_read_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_write: {
      titleKey: "settings.tools_page.file_write_title",
      descKey: "settings.tools_page.file_write_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_list: {
      titleKey: "settings.tools_page.file_list_title",
      descKey: "settings.tools_page.file_list_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_edit: {
      titleKey: "settings.tools_page.file_edit_title",
      descKey: "settings.tools_page.file_edit_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_glob: {
      titleKey: "settings.tools_page.file_glob_title",
      descKey: "settings.tools_page.file_glob_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_grep: {
      titleKey: "settings.tools_page.file_grep_title",
      descKey: "settings.tools_page.file_grep_desc",
      icon: FolderOpen,
      canConfigure: false,
    },
    notebook_read: {
      titleKey: "settings.tools_page.notebook_read_title",
      descKey: "settings.tools_page.notebook_read_desc",
      icon: FileCode,
      canConfigure: false,
    },
    notebook_edit: {
      titleKey: "settings.tools_page.notebook_edit_title",
      descKey: "settings.tools_page.notebook_edit_desc",
      icon: FileCode,
      canConfigure: false,
    },
    http_fetch: {
      titleKey: "settings.tools_page.http_fetch_title",
      descKey: "settings.tools_page.http_fetch_desc",
      icon: Network,
      canConfigure: false,
    },
    memory_search: {
      titleKey: "settings.tools_page.memory_search_title",
      descKey: "settings.tools_page.memory_search_desc",
      icon: Brain,
      canConfigure: false,
    },
    ask_user: {
      titleKey: "settings.tools_page.ask_user_title",
      descKey: "settings.tools_page.ask_user_desc",
      icon: UserSquare,
      canConfigure: false,
    },
  };

  const FALLBACK: ToolPresentation = {
    titleKey: "",
    descKey: "",
    icon: Terminal,
    canConfigure: false,
  };

  let drawerOpen = $state(false);
  let drawerTool = $state<ToolStatusDto | null>(null);
  let initialized = $state(false);

  onMount(() => {
    void loadTools().finally(() => {
      initialized = true;
    });
  });

  function presentationFor(name: string): ToolPresentation {
    return PRESENTATIONS[name] ?? FALLBACK;
  }

  function titleFor(name: string, presentation: ToolPresentation): string {
    return presentation.titleKey ? $t(presentation.titleKey) : name;
  }

  function warningFor(tool: ToolStatusDto): string | null {
    if (tool.name !== "web_search") return null;
    const cfg = tool.config as { backend?: unknown } | null;
    const backend = typeof cfg?.backend === "string" ? cfg.backend : "auto";
    const hasBraveKey = tool.credential_keys.includes("brave.api_key");
    if (backend === "brave" && !hasBraveKey) {
      return $t("settings.tools_page.warning_brave_forced_no_key");
    }
    if (backend === "auto" && !hasBraveKey) {
      return $t("settings.tools_page.warning_brave_auto_fallback");
    }
    return null;
  }

  async function handleToggle(tool: ToolStatusDto, enabled: boolean): Promise<void> {
    try {
      await toggleTool(tool.name, enabled);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      addToast($t("settings.tools_page.toggle_error", { values: { message } }), "error", {
        "data-testid": "tool-toggle-error-toast",
      });
    }
  }

  function openDrawer(tool: ToolStatusDto): void {
    drawerTool = tool;
    drawerOpen = true;
  }

  function closeDrawer(): void {
    drawerOpen = false;
    void loadTools();
  }

  async function reload(): Promise<void> {
    await loadTools();
    addToast($t("settings.tools_page.reloaded_toast"), "info", {
      "data-testid": "tools-reloaded-toast",
    });
  }
</script>

<section class="space-y-4" data-testid="settings-tools">
  <header class="flex items-center justify-between gap-3">
    <div>
      <h2 class="text-lg font-semibold">{$t("settings.tools_page.heading")}</h2>
      <p class="text-xs text-muted-foreground">
        {$t("settings.tools_page.subtitle")}
      </p>
    </div>
    <Button
      variant="outline"
      size="sm"
      onclick={reload}
      loading={$loadingTools && initialized}
      data-testid="tools-reload"
    >
      <RotateCw size={14} class="mr-1" aria-hidden="true" />
      {$t("settings.tools_page.reload")}
    </Button>
  </header>

  {#if !initialized && $loadingTools}
    <SettingSectionSkeleton />
  {:else if $toolsError}
    <div
      class="rounded-md border border-destructive/30 bg-destructive/5 px-4 py-2 text-sm text-destructive"
      data-testid="tools-error"
    >
      {$toolsError}
    </div>
  {:else if $tools.length === 0}
    <p class="text-sm text-muted-foreground">{$t("settings.tools_page.empty")}</p>
  {:else}
    <ul class="space-y-2" data-testid="tools-list">
      {#each $tools as tool (tool.name)}
        {@const presentation = presentationFor(tool.name)}
        <li>
          <ToolCard
            {tool}
            title={titleFor(tool.name, presentation)}
            description={presentation.descKey ? $t(presentation.descKey) : ""}
            icon={presentation.icon}
            warning={warningFor(tool)}
            canConfigure={presentation.canConfigure}
            busy={$loadingTools}
            onToggle={(enabled) => handleToggle(tool, enabled)}
            onConfigure={() => openDrawer(tool)}
          />
        </li>
      {/each}
    </ul>
  {/if}
</section>

<ToolConfigDrawer open={drawerOpen} tool={drawerTool} onclose={closeDrawer} />
