<script lang="ts">
  import { onMount } from "svelte";
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
    title: string;
    description: string;
    icon: typeof Search;
    canConfigure: boolean;
  }

  const PRESENTATIONS: Record<string, ToolPresentation> = {
    web_search: {
      title: "Recherche Web",
      description: "Interroge DuckDuckGo (par défaut) ou Brave si une clé API est configurée.",
      icon: Search,
      canConfigure: true,
    },
    web_read: {
      title: "Lecture Web",
      description: "Récupère et extrait le contenu lisible d'une URL avec garde anti-SSRF.",
      icon: Globe,
      canConfigure: true,
    },
    bash_executor: {
      title: "Bash",
      description: "Exécute des commandes shell dans une sandbox supervisée.",
      icon: Terminal,
      canConfigure: false,
    },
    python_executor: {
      title: "Python",
      description: "Exécute du code Python isolé dans le runtime PyO3.",
      icon: FileCode,
      canConfigure: false,
    },
    file_read: {
      title: "Lecture de fichiers",
      description: "Lit un fichier dans le workspace de l'agent.",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_write: {
      title: "Écriture de fichiers",
      description: "Écrit ou crée un fichier dans le workspace.",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_list: {
      title: "Listing de fichiers",
      description: "Liste les fichiers d'un répertoire.",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_edit: {
      title: "Édition de fichiers",
      description: "Applique un patch ou une édition sur un fichier existant.",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_glob: {
      title: "Recherche par glob",
      description: "Trouve les fichiers correspondant à un pattern.",
      icon: FolderOpen,
      canConfigure: false,
    },
    file_grep: {
      title: "Recherche de contenu",
      description: "Recherche un motif dans les fichiers du workspace.",
      icon: FolderOpen,
      canConfigure: false,
    },
    notebook_read: {
      title: "Lecture de notebooks",
      description: "Lit les cellules d'un notebook Jupyter.",
      icon: FileCode,
      canConfigure: false,
    },
    notebook_edit: {
      title: "Édition de notebooks",
      description: "Modifie les cellules d'un notebook Jupyter.",
      icon: FileCode,
      canConfigure: false,
    },
    http_fetch: {
      title: "HTTP Fetch",
      description: "Effectue une requête HTTP arbitraire (avec garde anti-SSRF).",
      icon: Network,
      canConfigure: false,
    },
    memory_search: {
      title: "Mémoire",
      description: "Recherche dans la mémoire épisodique/sémantique de l'agent.",
      icon: Brain,
      canConfigure: false,
    },
    ask_user: {
      title: "Demande utilisateur",
      description: "Sollicite une réponse humaine via un dialogue HITL.",
      icon: UserSquare,
      canConfigure: false,
    },
  };

  const FALLBACK: ToolPresentation = {
    title: "",
    description: "",
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
    return PRESENTATIONS[name] ?? { ...FALLBACK, title: name };
  }

  function warningFor(tool: ToolStatusDto): string | null {
    if (tool.name !== "web_search") return null;
    const cfg = tool.config as { backend?: unknown } | null;
    const backend = typeof cfg?.backend === "string" ? cfg.backend : "auto";
    const hasBraveKey = tool.credential_keys.includes("brave.api_key");
    if (backend === "brave" && !hasBraveKey) {
      return "Brave forcé mais clé absente — l'outil échouera.";
    }
    if (backend === "auto" && !hasBraveKey) {
      return "Brave non configuré — DuckDuckGo utilisé par défaut.";
    }
    return null;
  }

  async function handleToggle(tool: ToolStatusDto, enabled: boolean): Promise<void> {
    try {
      await toggleTool(tool.name, enabled);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      addToast(`Action refusée : ${message}`, "error", {
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
    addToast("Outils rechargés", "info", { "data-testid": "tools-reloaded-toast" });
  }
</script>

<section class="space-y-4" data-testid="settings-tools">
  <header class="flex items-center justify-between gap-3">
    <div>
      <h2 class="text-lg font-semibold">Outils natifs</h2>
      <p class="text-xs text-muted-foreground">
        Activez, désactivez et configurez chaque outil mis à disposition des agents.
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
      Recharger
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
    <p class="text-sm text-muted-foreground">Aucun outil natif détecté.</p>
  {:else}
    <ul class="space-y-2" data-testid="tools-list">
      {#each $tools as tool (tool.name)}
        {@const presentation = presentationFor(tool.name)}
        <li>
          <ToolCard
            {tool}
            title={presentation.title}
            description={presentation.description}
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
