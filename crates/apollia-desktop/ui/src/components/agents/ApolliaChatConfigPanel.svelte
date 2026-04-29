<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast/store";

  /**
   * DTO miroir de `ChatLibreConfigDto` côté Rust.
   * Champs facultatifs cohérents avec les fallbacks silencieux du runtime.
   */
  interface ChatLibreConfigDto {
    system_prompt: string;
    allowed_tools: string[];
    llm_backend: string | null;
  }

  let systemPrompt = $state("");
  let allowedToolsRaw = $state("");
  let llmBackend = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);

  onMount(load);

  async function load(): Promise<void> {
    loading = true;
    error = null;
    try {
      const cfg = await invoke<ChatLibreConfigDto>("get_chat_libre_config");
      systemPrompt = cfg.system_prompt;
      allowedToolsRaw = cfg.allowed_tools.join(", ");
      llmBackend = cfg.llm_backend ?? "";
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function parseTools(raw: string): string[] {
    return raw
      .split(/[,\n]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  }

  async function save(): Promise<void> {
    saving = true;
    try {
      const config: ChatLibreConfigDto = {
        system_prompt: systemPrompt,
        allowed_tools: parseTools(allowedToolsRaw),
        llm_backend: llmBackend.trim().length === 0 ? null : llmBackend.trim(),
      };
      await invoke("update_chat_libre_config", { config });
      addToast("Configuration Apollia Chat enregistrée", "success");
    } catch (err) {
      addToast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      saving = false;
    }
  }
</script>

<section class="space-y-4 rounded-lg border border-border/60 bg-surface-1 p-5" data-testid="apollia-chat-config-panel">
  <header class="space-y-1">
    <h3 class="text-[15px] font-semibold">Apollia Chat — Configuration</h3>
    <p class="text-[11.5px] text-muted-foreground">
      Valeurs persistées dans <code>governance.db</code> · appliquées à toute
      nouvelle session du chat libre.
    </p>
  </header>

  {#if error}
    <div class="rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
      {error}
    </div>
  {/if}

  <div class="space-y-1.5">
    <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="chat-libre-system-prompt">
      System prompt par défaut
    </label>
    <textarea
      id="chat-libre-system-prompt"
      bind:value={systemPrompt}
      disabled={loading}
      rows="6"
      placeholder="(vide ⇒ comportement runtime par défaut)"
      class="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] text-foreground focus:border-primary focus:outline-none"
      data-testid="apollia-chat-system-prompt"
    ></textarea>
  </div>

  <div class="space-y-1.5">
    <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="chat-libre-allowed-tools">
      Outils autorisés sans HITL (séparés par virgule)
    </label>
    <input
      id="chat-libre-allowed-tools"
      type="text"
      bind:value={allowedToolsRaw}
      disabled={loading}
      placeholder="bash_executor, file_read, file_write"
      class="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] text-foreground focus:border-primary focus:outline-none"
      data-testid="apollia-chat-allowed-tools"
    />
    <p class="text-[10.5px] text-muted-foreground">
      Vide ⇒ exposition de tous les outils du registre.
      Les outils auto-approuvés via le bouton « Toujours autoriser » du chat
      apparaissent dans <strong>Réglages › Permissions › Chat</strong>.
    </p>
  </div>

  <div class="space-y-1.5">
    <label class="text-[11px] uppercase tracking-wide text-muted-foreground" for="chat-libre-backend">
      Backend LLM préféré
    </label>
    <input
      id="chat-libre-backend"
      type="text"
      bind:value={llmBackend}
      disabled={loading}
      placeholder="(vide ⇒ défaut runtime)"
      class="w-full rounded-md border border-border bg-background px-3 py-2 font-mono text-[12px] text-foreground focus:border-primary focus:outline-none"
      data-testid="apollia-chat-llm-backend"
    />
  </div>

  <div class="flex justify-end gap-2 pt-2">
    <Button variant="outline" size="sm" onclick={load} disabled={loading || saving}>
      Recharger
    </Button>
    <Button variant="default" size="sm" onclick={save} loading={saving} disabled={loading}>
      Enregistrer
    </Button>
  </div>
</section>
