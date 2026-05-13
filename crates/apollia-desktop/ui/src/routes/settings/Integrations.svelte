<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { BtnPrimary, BtnSecondary } from "$lib/components/operator";

  /**
   * Snapshot of the active OAuth client_id per connector provider plus the
   * source of that value: `env` (env var), `file` (user-edited override file),
   * `builtin` (compiled into the release), or `none` (nothing configured).
   */
  interface OauthClientIdStatus {
    provider: string;
    effective_client_id: string;
    source: "env" | "file" | "builtin" | "none";
    override_client_id: string | null;
  }

  const PROVIDER_LABELS: Record<string, string> = {
    google: "Google Workspace",
    microsoft: "Microsoft 365",
  };

  let statuses = $state<OauthClientIdStatus[]>([]);
  let drafts = $state<Record<string, string>>({});
  let loading = $state(false);
  let savingProvider = $state<string | null>(null);
  let error = $state<string | null>(null);
  let info = $state<string | null>(null);

  async function refresh(): Promise<void> {
    loading = true;
    error = null;
    try {
      const fresh = await invoke<OauthClientIdStatus[]>("oauth_list_client_ids");
      statuses = fresh;
      // Seed drafts from the override file, not from the effective value —
      // we want users to see "empty input means: no override, fall back to
      // the next step of the resolution chain" rather than pre-filling
      // their input with a value they did not choose.
      drafts = Object.fromEntries(
        fresh.map((s) => [s.provider, s.override_client_id ?? ""]),
      );
    } catch (e) {
      error = formatError(e);
    } finally {
      loading = false;
    }
  }

  function formatError(e: unknown): string {
    if (typeof e === "string") return e;
    const anyE = e as { kind?: string; detail?: string; message?: string };
    if (anyE.kind && anyE.detail) return `${anyE.kind}: ${anyE.detail}`;
    if (anyE.message) return anyE.message;
    if (anyE.kind) return anyE.kind;
    return String(e);
  }

  async function save(provider: string): Promise<void> {
    savingProvider = provider;
    error = null;
    info = null;
    try {
      const value = (drafts[provider] ?? "").trim();
      await invoke("oauth_set_client_id", { provider, clientId: value });
      info =
        value.length === 0
          ? `Override removed for ${PROVIDER_LABELS[provider] ?? provider}.`
          : `Client ID saved for ${PROVIDER_LABELS[provider] ?? provider}.`;
      await refresh();
    } catch (e) {
      error = formatError(e);
    } finally {
      savingProvider = null;
    }
  }

  async function clearOverride(provider: string): Promise<void> {
    drafts = { ...drafts, [provider]: "" };
    await save(provider);
  }

  function sourceLabel(source: string): string {
    switch (source) {
      case "env":
        return "Variable d'environnement";
      case "file":
        return "Fichier ~/.apollia/oauth-clients.toml";
      case "builtin":
        return "Build officiel (compilé)";
      default:
        return "Non configuré";
    }
  }

  function sourceTone(source: string): string {
    switch (source) {
      case "env":
        return "text-amber-700 dark:text-amber-400";
      case "file":
        return "text-primary";
      case "builtin":
        return "text-emerald-700 dark:text-emerald-400";
      default:
        return "text-destructive";
    }
  }

  onMount(() => {
    void refresh();
  });
</script>

<div class="space-y-5" data-testid="settings-integrations-oauth">
  <section class="space-y-2">
    <h2 class="text-base font-semibold text-foreground">
      Identifiants OAuth des connecteurs
    </h2>
    <p class="text-xs text-muted-foreground max-w-prose">
      Apollia parle aux services Google et Microsoft via leur flow OAuth 2.0
      avec PKCE — donc sans secret client, mais avec un identifiant public
      (<code>client_id</code>) qui identifie l'application Apollia auprès du
      fournisseur. Le build officiel embarque cet identifiant ; sur une compilation
      maison (fork, dev local), branchez ici le vôtre, créé dans la console
      Google Cloud ou Azure AD.
    </p>
    <p class="text-xs text-muted-foreground max-w-prose">
      Ordre de résolution :
      <code>APOLLIA_GOOGLE_CLIENT_ID</code> / <code>APOLLIA_MICROSOFT_CLIENT_ID</code>
      &rarr; ce fichier &rarr; build officiel. Laissez vide pour retomber sur
      l'étape suivante.
    </p>
  </section>

  {#if error}
    <div
      class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive"
      data-testid="oauth-settings-error"
    >
      {error}
    </div>
  {/if}
  {#if info}
    <div
      class="rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300"
      data-testid="oauth-settings-info"
    >
      {info}
    </div>
  {/if}

  {#if loading && statuses.length === 0}
    <div class="space-y-2">
      {#each [0, 1] as i (i)}
        <div class="h-28 rounded-xl bg-surface-1 border border-border animate-pulse"></div>
      {/each}
    </div>
  {:else}
    <div class="grid grid-cols-1 gap-3">
      {#each statuses as status (status.provider)}
        <div
          class="rounded-xl border border-border bg-surface-1 p-4 space-y-3"
          data-testid={`oauth-card-${status.provider}`}
        >
          <div class="flex items-start justify-between gap-3">
            <div>
              <div class="font-medium text-sm">
                {PROVIDER_LABELS[status.provider] ?? status.provider}
              </div>
              <div class="mt-0.5 text-[11px] font-mono {sourceTone(status.source)}">
                Source actuelle : {sourceLabel(status.source)}
              </div>
            </div>
            <BtnSecondary
              onclick={() => clearOverride(status.provider)}
              disabled={savingProvider !== null ||
                (status.override_client_id ?? "").length === 0}
            >
              Effacer l'override
            </BtnSecondary>
          </div>

          {#if status.effective_client_id}
            <div class="text-[11px] text-muted-foreground">
              <span class="font-mono">{status.effective_client_id}</span>
            </div>
          {:else}
            <div class="text-[11px] text-destructive">
              Aucun client_id résolu — le bouton « Connecter » échouera avec
              <code>oauth_client_not_configured</code>.
            </div>
          {/if}

          <label class="block text-xs font-medium">
            Override OAuth client_id
            <input
              type="text"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
              placeholder={status.provider === "google"
                ? "1234567890-abcdefgh.apps.googleusercontent.com"
                : "00000000-0000-0000-0000-000000000000"}
              bind:value={drafts[status.provider]}
              disabled={savingProvider === status.provider}
              autocomplete="off"
              spellcheck={false}
              data-testid={`oauth-input-${status.provider}`}
            />
          </label>

          <div class="flex justify-end">
            <BtnPrimary
              onclick={() => save(status.provider)}
              disabled={savingProvider === status.provider}
            >
              {savingProvider === status.provider ? "Enregistrement…" : "Enregistrer"}
            </BtnPrimary>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
