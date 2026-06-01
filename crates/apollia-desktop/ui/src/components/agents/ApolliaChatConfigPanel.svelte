<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Search, Sparkles, Wrench, Cpu } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Badge } from "$lib/components/ui/badge";
  import { addToast } from "$lib/components/ui/toast/store";

  interface ChatLibreConfigDto {
    system_prompt: string;
    allowed_tools: string[];
    llm_backend: string | null;
  }

  interface ToolSummary {
    name: string;
    version: string;
    description: string;
    kind: string;
  }

  interface LlmBackendView {
    name: string;
    provider: string;
    model: string;
    enabled: boolean;
    is_default: boolean;
  }

  // ── State ────────────────────────────────────────────────────────────
  let systemPrompt = $state("");
  let allowedTools = $state<Set<string>>(new Set());
  /** "" ⇒ utiliser le modèle par défaut. */
  let llmBackend = $state("");

  let availableTools = $state<ToolSummary[]>([]);
  let availableBackends = $state<LlmBackendView[]>([]);

  let toolFilter = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  onMount(load);

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      const [cfg, tools, backends] = await Promise.all([
        invoke<ChatLibreConfigDto>("get_chat_libre_config"),
        invoke<ToolSummary[]>("list_tools").catch(() => [] as ToolSummary[]),
        invoke<LlmBackendView[]>("list_llm_backends").catch(
          () => [] as LlmBackendView[],
        ),
      ]);
      systemPrompt = cfg.system_prompt;
      allowedTools = new Set(cfg.allowed_tools);
      llmBackend = cfg.llm_backend ?? "";
      availableTools = [...tools].sort((a, b) =>
        a.name.localeCompare(b.name),
      );
      availableBackends = [...backends]
        .filter((b) => b.enabled)
        .sort((a, b) => a.name.localeCompare(b.name));
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  async function save(): Promise<void> {
    saving = true;
    try {
      const config: ChatLibreConfigDto = {
        system_prompt: systemPrompt,
        allowed_tools: Array.from(allowedTools).sort(),
        llm_backend: llmBackend.length === 0 ? null : llmBackend,
      };
      await invoke("update_chat_libre_config", { config });
      addToast("Configuration enregistrée", "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }

  // ── Derived ──────────────────────────────────────────────────────────
  const filteredTools = $derived.by(() => {
    const q = toolFilter.trim().toLowerCase();
    if (q.length === 0) return availableTools;
    return availableTools.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q),
    );
  });

  const allowedCount = $derived(allowedTools.size);
  const defaultBackend = $derived(
    availableBackends.find((b) => b.is_default) ?? null,
  );

  // ── Helpers ──────────────────────────────────────────────────────────
  function toggleTool(name: string): void {
    const next = new Set(allowedTools);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    allowedTools = next;
  }

  function selectAll(): void {
    allowedTools = new Set(filteredTools.map((t) => t.name));
  }

  function selectNone(): void {
    allowedTools = new Set();
  }

  function kindLabel(kind: string): string {
    switch (kind) {
      case "native":
        return "Natif";
      case "mcp":
        return "MCP";
      case "python":
        return "Python";
      default:
        return kind;
    }
  }
</script>

<div class="space-y-6" data-testid="apollia-chat-config-panel">
  {#if error}
    <div
      class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive"
      data-testid="apollia-chat-config-error"
    >
      {error}
    </div>
  {/if}

  <!-- ── Section : Personnalité ───────────────────────────────────── -->
  <section
    class="space-y-3 rounded-lg border border-border/60 bg-surface-1 p-5"
  >
    <header class="flex items-start gap-3">
      <div
        class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
      >
        <Sparkles size={14} />
      </div>
      <div class="min-w-0">
        <h3 class="text-[14px] font-semibold">Personnalité</h3>
        <p class="text-[11.5px] text-muted-foreground">
          Le ton, le style et les consignes que l'assistant suit dans chaque
          conversation.
        </p>
      </div>
    </header>
    <Textarea
      id="apollia-chat-system-prompt"
      bind:value={systemPrompt}
      disabled={loading}
      rows={6}
      placeholder="Ex. : Tu es Apollia, un assistant local concis et pragmatique. Tu réponds en français, tu poses une question si quelque chose est ambigu, et tu privilégies les réponses courtes."
      data-testid="apollia-chat-system-prompt"
    />
    <p class="text-[10.5px] text-muted-foreground">
      Laissez vide pour conserver le comportement standard.
    </p>
  </section>

  <!-- ── Section : Outils ─────────────────────────────────────────── -->
  <section
    class="space-y-3 rounded-lg border border-border/60 bg-surface-1 p-5"
  >
    <header class="flex items-start gap-3">
      <div
        class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
      >
        <Wrench size={14} />
      </div>
      <div class="min-w-0 flex-1">
        <h3 class="text-[14px] font-semibold">
          Outils autorisés
          <span class="ml-1 text-muted-foreground font-normal">
            · {allowedCount} sélectionné{allowedCount === 1 ? "" : "s"}
          </span>
        </h3>
        <p class="text-[11.5px] text-muted-foreground">
          Cochez les outils qu'Apollia Chat peut utiliser sans demander
          confirmation à chaque appel.
        </p>
      </div>
      <div class="flex shrink-0 gap-1.5">
        <Button
          variant="ghost"
          size="sm"
          onclick={selectAll}
          disabled={loading || filteredTools.length === 0}
          data-testid="apollia-chat-tools-all"
        >
          Tout cocher
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onclick={selectNone}
          disabled={loading || allowedCount === 0}
          data-testid="apollia-chat-tools-none"
        >
          Tout décocher
        </Button>
      </div>
    </header>

    <div
      class="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-[7px]"
    >
      <Search size={11} class="text-muted-foreground" />
      <input
        type="text"
        bind:value={toolFilter}
        placeholder="Filtrer par nom ou description"
        disabled={loading}
        class="flex-1 border-none bg-transparent text-[12px] text-foreground placeholder:text-muted-foreground focus:outline-none"
        data-testid="apollia-chat-tools-filter"
      />
    </div>

    {#if loading}
      <p class="px-2 py-6 text-center text-xs text-muted-foreground">
        Chargement des outils…
      </p>
    {:else if availableTools.length === 0}
      <p
        class="rounded-md border border-dashed border-border px-4 py-4 text-center text-xs text-muted-foreground"
      >
        Aucun outil disponible dans ce runtime.
      </p>
    {:else if filteredTools.length === 0}
      <p
        class="rounded-md border border-dashed border-border px-4 py-4 text-center text-xs text-muted-foreground"
      >
        Aucun outil ne correspond à « {toolFilter} ».
      </p>
    {:else}
      <ul
        class="max-h-72 divide-y divide-border overflow-y-auto rounded-md border border-border"
        data-testid="apollia-chat-tools-list"
      >
        {#each filteredTools as tool (tool.name)}
          {@const checked = allowedTools.has(tool.name)}
          <li>
            <Button variant="ghost" size="sm"
              type="button"
              onclick={() => toggleTool(tool.name)}
              class="flex w-full items-start gap-3 px-3 py-2 text-left transition-colors hover:bg-surface-2"
              data-testid="apollia-chat-tool-row"
              data-tool-name={tool.name}
              data-checked={checked}
            >
              <Checkbox {checked} class="mt-0.5" />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2 text-[12.5px]">
                  <code class="font-mono text-foreground">{tool.name}</code>
                  <Badge variant="secondary" class="text-[10px]">
                    {kindLabel(tool.kind)}
                  </Badge>
                </div>
                {#if tool.description}
                  <p class="mt-0.5 text-[11px] text-muted-foreground">
                    {tool.description}
                  </p>
                {/if}
              </div>
            </Button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <!-- ── Section : Modèle ─────────────────────────────────────────── -->
  <section
    class="space-y-3 rounded-lg border border-border/60 bg-surface-1 p-5"
  >
    <header class="flex items-start gap-3">
      <div
        class="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary"
      >
        <Cpu size={14} />
      </div>
      <div class="min-w-0">
        <h3 class="text-[14px] font-semibold">Modèle</h3>
        <p class="text-[11.5px] text-muted-foreground">
          Choisissez quel modèle Apollia Chat doit utiliser.
        </p>
      </div>
    </header>

    <Select
      id="apollia-chat-llm-backend"
      bind:value={llmBackend}
      disabled={loading || availableBackends.length === 0}
      data-testid="apollia-chat-llm-backend"
    >
      <option value="">
        Modèle par défaut{defaultBackend
          ? ` (${defaultBackend.name} - ${defaultBackend.model})`
          : ""}
      </option>
      {#each availableBackends as b (b.name)}
        <option value={b.name}>
          {b.name} · {b.provider} · {b.model}{b.is_default ? " (défaut)" : ""}
        </option>
      {/each}
    </Select>
    {#if availableBackends.length === 0 && !loading}
      <p class="text-[10.5px] text-muted-foreground">
        Aucun modèle configuré. Ajoutez-en un dans Réglages › Modèles.
      </p>
    {/if}
  </section>

  <!-- ── Actions ──────────────────────────────────────────────────── -->
  <div class="flex justify-end gap-2 pt-1">
    <Button
      variant="outline"
      size="sm"
      onclick={load}
      disabled={loading || saving}
      data-testid="apollia-chat-config-reload"
    >
      Annuler les modifications
    </Button>
    <Button
      variant="default"
      size="sm"
      onclick={save}
      loading={saving}
      disabled={loading}
      data-testid="apollia-chat-config-save"
    >
      Enregistrer
    </Button>
  </div>
</div>
