<!--
  /integrations — Native OAuth connectors (Google Workspace, Microsoft 365).

  Distinct from /connections which handles MCP servers. This route surfaces
  the Tauri commands defined in commands/integrations.rs:
  - oauth_start_flow / oauth_complete_flow drive the OAuth code flow.
  - oauth_list_accounts / oauth_get_status feed the connected-accounts UI.
  - oauth_disconnect revokes a stored token.

  The OAuth callback is consumed by the runtime's existing callback HTTP
  listener (apollia-auth::callback). The renderer opens the auth URL in the
  system browser via Tauri's shell plugin, then polls oauth_get_status to
  detect completion.

  v0.1.0 power-user audience (cf. ADR positioning): the wording is direct,
  not over-pedagogical, and surfaces the actual scope groups granted.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  type ProviderId = "google" | "microsoft";

  type OauthAccountInfo = {
    provider: ProviderId;
    account_id: string;
  };

  type OauthStartFlow = {
    auth_url: string;
    state: string;
    callback_port: number;
  };

  type SovereigntyProfile = "cloud_allowed" | "local_only";

  // Provider catalog rendered as cards.
  const PROVIDERS: {
    id: ProviderId;
    name: string;
    description: string;
    scopes: { id: string; label: string }[];
  }[] = [
    {
      id: "google",
      name: "Google Workspace",
      description:
        "Gmail (envoi + brouillons), Google Calendar, Google Drive Workspace.",
      scopes: [
        { id: "mail.send", label: "Envoyer des emails (Gmail)" },
        { id: "mail.compose", label: "Brouillons (Gmail)" },
        { id: "calendar.read", label: "Lire l'agenda" },
        { id: "calendar.write", label: "Créer/modifier des événements" },
        { id: "drive.workspace", label: "Espace Drive de l'agent" },
      ],
    },
    {
      id: "microsoft",
      name: "Microsoft 365",
      description:
        "Outlook Mail (lecture + envoi), Outlook Calendar, OneDrive.",
      scopes: [
        { id: "mail.read", label: "Lire les emails Outlook" },
        { id: "mail.send", label: "Envoyer des emails Outlook" },
        { id: "calendar.read", label: "Lire l'agenda Outlook" },
        { id: "calendar.write", label: "Créer/modifier des événements" },
        { id: "files.read", label: "Lire OneDrive" },
      ],
    },
  ];

  let accounts = $state<OauthAccountInfo[]>([]);
  let loading = $state(false);
  let pendingState = $state<string | null>(null);
  let pendingProvider = $state<ProviderId | null>(null);
  let selectedScopes = $state<Map<ProviderId, Set<string>>>(
    new Map(PROVIDERS.map((p) => [p.id, new Set(p.scopes.map((s) => s.id))])),
  );
  let errorMessage = $state<string | null>(null);
  let sovereignty = $state<SovereigntyProfile>("cloud_allowed");

  async function refreshStatus() {
    try {
      accounts = await invoke<OauthAccountInfo[]>("oauth_get_status");
    } catch (e) {
      // Backend errors carry a "kind" tag — surface a friendly summary.
      errorMessage = formatError(e);
    }
  }

  function accountsFor(provider: ProviderId): OauthAccountInfo[] {
    return accounts.filter((a) => a.provider === provider);
  }

  function toggleScope(provider: ProviderId, scopeId: string) {
    const current = selectedScopes.get(provider) ?? new Set<string>();
    if (current.has(scopeId)) {
      current.delete(scopeId);
    } else {
      current.add(scopeId);
    }
    selectedScopes = new Map(selectedScopes.set(provider, current));
  }

  async function startConnect(provider: ProviderId) {
    errorMessage = null;
    if (sovereignty === "local_only") {
      errorMessage =
        "Profil souveraineté « local-only » actif : les connecteurs cloud sont désactivés.";
      return;
    }
    loading = true;
    try {
      const scopes = Array.from(selectedScopes.get(provider) ?? new Set());
      const start = await invoke<OauthStartFlow>("oauth_start_flow", {
        provider,
        scopes,
        sovereignty,
      });
      pendingState = start.state;
      pendingProvider = provider;
      // Open the authorization URL in the system browser. apollia-auth's
      // callback listener is already bound and will receive the code; the
      // backend reconciles the state -> flow mapping in oauth_complete_flow
      // once the renderer or the callback HTTP server calls it.
      window.open(start.auth_url, "_blank");
    } catch (e) {
      errorMessage = formatError(e);
    } finally {
      loading = false;
    }
  }

  /**
   * Manually advance the flow with the authorization code.
   *
   * v0.1.0 ships this as a small text field so the operator can paste the
   * code emitted by the browser callback page. A later iteration wires the
   * callback HTTP listener directly to the renderer via a Tauri event.
   */
  let pastedCode = $state("");
  async function completeWithCode() {
    if (!pendingState || !pastedCode) return;
    loading = true;
    try {
      const result = await invoke("oauth_complete_flow", {
        state: pendingState,
        code: pastedCode,
      });
      pastedCode = "";
      pendingState = null;
      pendingProvider = null;
      await refreshStatus();
      errorMessage = null;
      console.log("OAuth flow completed:", result);
    } catch (e) {
      errorMessage = formatError(e);
    } finally {
      loading = false;
    }
  }

  async function disconnect(account: OauthAccountInfo) {
    if (
      !confirm(
        `Déconnecter ${account.account_id} (${account.provider}) ? Le token sera révoqué localement.`,
      )
    ) {
      return;
    }
    loading = true;
    try {
      await invoke("oauth_disconnect", {
        provider: account.provider,
        accountId: account.account_id,
      });
      await refreshStatus();
    } catch (e) {
      errorMessage = formatError(e);
    } finally {
      loading = false;
    }
  }

  function formatError(e: unknown): string {
    if (typeof e === "string") return e;
    const anyE = e as { kind?: string; detail?: string };
    if (anyE.kind === "sovereignty_blocked") {
      return "Profil souveraineté « local-only » : connecteurs cloud désactivés.";
    }
    if (anyE.kind && anyE.detail) {
      return `${anyE.kind}: ${anyE.detail}`;
    }
    if (anyE.kind) return anyE.kind;
    return String(e);
  }

  onMount(() => {
    void refreshStatus();
  });
</script>

<div class="integrations-route">
  <header>
    <h1>Intégrations</h1>
    <p>
      Connectez vos comptes Google et Microsoft : vos tokens OAuth restent
      dans le keyring local, aucune donnée ne transite par Apollia.
    </p>
  </header>

  {#if sovereignty === "local_only"}
    <div class="banner banner--info">
      Profil souveraineté <strong>local-only</strong> actif. Les connecteurs
      cloud sont désactivés. Seuls les serveurs MCP stdio purement locaux
      restent disponibles dans l'onglet <em>Connexions</em>.
    </div>
  {/if}

  {#if errorMessage}
    <div class="banner banner--error">{errorMessage}</div>
  {/if}

  <section class="providers">
    {#each PROVIDERS as provider}
      {@const connected = accountsFor(provider.id)}
      <article class="provider-card">
        <h2>{provider.name}</h2>
        <p>{provider.description}</p>

        {#if connected.length > 0}
          <h3>Comptes connectés</h3>
          <ul class="account-list">
            {#each connected as account}
              <li>
                <span>{account.account_id}</span>
                <button
                  type="button"
                  onclick={() => disconnect(account)}
                  disabled={loading}
                >
                  Déconnecter
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        <details>
          <summary>Permissions à demander</summary>
          <ul class="scope-list">
            {#each provider.scopes as scope}
              <li>
                <label>
                  <input
                    type="checkbox"
                    checked={selectedScopes.get(provider.id)?.has(scope.id)}
                    onchange={() => toggleScope(provider.id, scope.id)}
                  />
                  {scope.label}
                </label>
              </li>
            {/each}
          </ul>
        </details>

        <button
          type="button"
          class="btn-primary"
          onclick={() => startConnect(provider.id)}
          disabled={loading || sovereignty === "local_only"}
        >
          Connecter un compte {provider.name}
        </button>

        {#if pendingState && pendingProvider === provider.id}
          <div class="pending">
            <p>
              Une fenêtre navigateur a été ouverte. Une fois l'autorisation
              accordée, collez ici le code retourné :
            </p>
            <input
              type="text"
              bind:value={pastedCode}
              placeholder="code OAuth"
            />
            <button
              type="button"
              onclick={completeWithCode}
              disabled={loading || !pastedCode}
            >
              Finaliser la connexion
            </button>
          </div>
        {/if}
      </article>
    {/each}
  </section>

  <footer class="footnote">
    <p>
      Pour Gmail en mode lecture complète, consultez
      <code>mode-expert-google-restricted-scopes</code> dans la documentation.
      Apollia v0.1.0 n'inclut pas les scopes restricted Google (qui exigent
      un audit CASA Tier 2).
    </p>
  </footer>
</div>

<style>
  .integrations-route {
    max-width: 960px;
    margin: 0 auto;
    padding: 2rem 1.5rem;
    display: grid;
    gap: 1.5rem;
  }

  h1 {
    margin: 0 0 0.25rem 0;
  }

  .banner {
    padding: 0.75rem 1rem;
    border-radius: 0.5rem;
    border: 1px solid transparent;
  }

  .banner--info {
    background: var(--bg-info, hsl(210 80% 95%));
    border-color: var(--border-info, hsl(210 60% 80%));
  }

  .banner--error {
    background: var(--bg-error, hsl(0 80% 96%));
    border-color: var(--border-error, hsl(0 60% 80%));
  }

  .providers {
    display: grid;
    gap: 1.5rem;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
  }

  .provider-card {
    padding: 1.5rem;
    border-radius: 0.75rem;
    border: 1px solid var(--border-subtle, hsl(0 0% 88%));
    background: var(--bg-card, hsl(0 0% 100%));
    display: grid;
    gap: 0.75rem;
  }

  .account-list,
  .scope-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 0.5rem;
  }

  .account-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .scope-list label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .pending {
    display: grid;
    gap: 0.5rem;
  }

  .btn-primary {
    padding: 0.5rem 0.875rem;
    border-radius: 0.5rem;
    border: none;
    background: var(--accent, hsl(218 86% 50%));
    color: white;
    cursor: pointer;
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .footnote {
    font-size: 0.875rem;
    color: var(--text-subtle, hsl(0 0% 40%));
  }
</style>
