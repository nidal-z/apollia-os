<script lang="ts">
  import { Eye, EyeOff, Check, X } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";
  import { Input } from "$lib/components/ui/input";
  import {
    setCredential,
    deleteCredential,
    testCredential,
    type CredentialTestResultDto,
  } from "$lib/stores/toolGovernance";

  interface Props {
    toolName: string;
    keyName: string;
    label: string;
    configured: boolean;
    canTest?: boolean;
    "data-testid"?: string;
  }

  let {
    toolName,
    keyName,
    label,
    configured,
    canTest = false,
    "data-testid": dataTestId,
  }: Props = $props();

  let editing = $state(false);
  let draft = $state("");
  let visible = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let saveError = $state<string | null>(null);
  let testResult = $state<CredentialTestResultDto | null>(null);

  function startEdit(): void {
    editing = true;
    draft = "";
    visible = false;
    saveError = null;
    testResult = null;
  }

  function cancelEdit(): void {
    editing = false;
    draft = "";
    visible = false;
    saveError = null;
  }

  async function commit(): Promise<void> {
    if (!draft) {
      saveError = "value_required";
      return;
    }
    saving = true;
    saveError = null;
    try {
      await setCredential(toolName, keyName, draft);
      editing = false;
      draft = "";
    } catch (err) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function remove(): Promise<void> {
    saving = true;
    saveError = null;
    try {
      await deleteCredential(toolName, keyName);
      testResult = null;
    } catch (err) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  async function runTest(): Promise<void> {
    testing = true;
    testResult = null;
    try {
      testResult = await testCredential(toolName);
    } catch (err) {
      testResult = {
        ok: false,
        latency_ms: null,
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      testing = false;
    }
  }
</script>

<div class="space-y-2" data-testid={dataTestId}>
  <div class="flex items-center justify-between gap-2">
    <label class="text-sm font-medium text-foreground" for="{dataTestId}-input">{label}</label>
    {#if configured && !editing}
      <span class="inline-flex items-center gap-1 rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs text-emerald-700 dark:text-emerald-400">
        <Check size={12} strokeWidth={2.5} aria-hidden="true" />
        Configurée
      </span>
    {:else if !configured && !editing}
      <span class="inline-flex items-center gap-1 rounded-md bg-amber-500/10 px-2 py-0.5 text-xs text-amber-700 dark:text-amber-400">
        Absente
      </span>
    {/if}
  </div>

  {#if !editing}
    <div class="flex items-center gap-2">
      <code class="flex-1 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm text-muted-foreground">
        {configured ? "••••••••••••" : "Non configurée"}
      </code>
      <Button variant="outline" size="sm" onclick={startEdit} data-testid="{dataTestId}-edit">
        {configured ? "Modifier" : "Ajouter"}
      </Button>
      {#if configured}
        <Button
          variant="ghost"
          size="sm"
          onclick={remove}
          disabled={saving}
          data-testid="{dataTestId}-delete"
        >
          Supprimer
        </Button>
        {#if canTest}
          <Button
            variant="outline"
            size="sm"
            onclick={runTest}
            disabled={testing}
            data-testid="{dataTestId}-test"
          >
            {#if testing}
              <Spinner class="mr-1 h-3 w-3" aria-hidden="true" />
            {/if}
            Tester
          </Button>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="flex items-center gap-2">
      <div class="relative flex-1">
        <Input
          id="{dataTestId}-input"
          type={visible ? "text" : "password"}
          bind:value={draft}
          autocomplete="off"
          spellcheck="false"
          class="flex h-10 w-full rounded-md border border-border bg-background px-3 py-2 pr-10 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-primary"
          placeholder="Saisir la valeur"
          data-testid="{dataTestId}-input"
         />
        <Button variant="ghost" size="sm"
          type="button"
          class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
          aria-label={visible ? "Masquer" : "Afficher"}
          onclick={() => (visible = !visible)}
        >
          {#if visible}
            <EyeOff size={16} aria-hidden="true" />
          {:else}
            <Eye size={16} aria-hidden="true" />
          {/if}
        </Button>
      </div>
      <Button
        variant="default"
        size="sm"
        onclick={commit}
        loading={saving}
        disabled={!draft || saving}
        data-testid="{dataTestId}-save"
      >
        Enregistrer
      </Button>
      <Button variant="ghost" size="sm" onclick={cancelEdit} disabled={saving}>
        Annuler
      </Button>
    </div>
  {/if}

  {#if saveError}
    <p class="text-xs text-destructive" data-testid="{dataTestId}-error">{saveError}</p>
  {/if}

  {#if testResult}
    <p
      class="flex items-center gap-1 text-xs"
      class:text-emerald-700={testResult.ok}
      class:text-destructive={!testResult.ok}
      data-testid="{dataTestId}-test-result"
    >
      {#if testResult.ok}
        <Check size={12} strokeWidth={2.5} aria-hidden="true" />
        Valide{testResult.latency_ms != null ? ` (${testResult.latency_ms} ms)` : ""}
      {:else}
        <X size={12} strokeWidth={2.5} aria-hidden="true" />
        Échec : {testResult.error ?? "erreur inconnue"}
      {/if}
    </p>
  {/if}
</div>
