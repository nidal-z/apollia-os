<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { ShieldCheck } from "lucide-svelte";
  import { Spinner } from "$lib/components/ui/progress";
  import { Card } from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import { Textarea } from "$lib/components/ui/textarea";

  // ── Props ────────────────────────────────────────────────────────────────────
  // Generic editor for the launch-time configuration of an installed MCP server.
  // Renders different field groups depending on `transport`:
  //   - `stdio`        → command (read-only), launcher prefix + package id
  //                      (read-only, auto-detected), user args (editable),
  //                      env values / secrets, timeouts.
  //   - `streamable-http` / `sse` → URL (read-only), auth env keys with
  //                                 secret-rotation affordance, timeouts.
  //
  // The component performs the full round-trip via the existing IPC commands
  // (`get_mcp_server_raw_config` → `update_mcp_server_config` /
  // `store_mcp_secret`) and emits `onSaved` so the parent can refetch detail.

  interface Props {
    serverName: string;
    onSaved?: () => void;
  }

  let { serverName, onSaved }: Props = $props();

  // ── Persisted config shape (matches backend `McpServerConfig`) ───────────────
  type RawConfig = {
    name: string;
    command: string;
    args: string[];
    env: Record<string, string>;
    transport: string;
    url?: string | null;
    requires_approval: boolean;
    init_timeout_secs: number;
    call_timeout_secs: number;
    tags: string[];
  };

  let raw = $state<RawConfig | null>(null);
  let loading = $state(true);
  let loadError = $state<string | null>(null);

  // Form state ── separated from `raw` so cancel reverts cleanly.
  let argsTailDraft = $state(""); // textarea contents for user-supplied tail
  let urlDraft = $state(""); // remote URL when transport != stdio
  let initTimeoutDraft = $state(30);
  let callTimeoutDraft = $state(60);
  let envValuesDraft = $state<Record<string, string>>({}); // plain (non-secret) env values
  let secretRotateValues = $state<Record<string, string>>({}); // new secret material per key
  /** Env keys whose value is `${APOLLIA_OAUTH}` - managed via the OAuth
   *  orchestrator, not through static rotation. */
  let oauthEnvHeaders = $state<string[]>([]);
  let oauthReconnecting = $state(false);
  let oauthReconnectError = $state<string | null>(null);
  let oauthReconnectSuccess = $state(false);

  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let saveOk = $state(false);

  // ── Helpers ──────────────────────────────────────────────────────────────────

  /**
   * Detect whether `arg` looks like an npm/pypi package identifier (scoped or
   * reverse-DNS), so we can render it as read-only and edit only the tail.
   * Mirrors the backend heuristic in `apollia-mcp::config::looks_like_package_identifier`.
   */
  function looksLikePackageIdentifier(arg: string): boolean {
    const a = arg.trim();
    if (a.startsWith("@")) return true;
    if (a.startsWith("/") || a.startsWith("./") || a.startsWith("../")) return false;
    if (a.length >= 3 && a[1] === ":" && (a[2] === "/" || a[2] === "\\")) return false;
    return a.includes("/");
  }

  /**
   * Split a stdio argv into a non-editable launcher prefix (flags + package id,
   * if any) and the editable user tail. Examples:
   *   ["-y", "@pkg/foo", "/Users/me/Docs"]  → head ["-y", "@pkg/foo"],   tail ["/Users/me/Docs"]
   *   ["@pkg/foo", "/Users/me/Docs"]        → head ["@pkg/foo"],         tail ["/Users/me/Docs"]
   *   ["mcp-server-time", "--local"]        → head ["mcp-server-time"],  tail ["--local"]
   *   ["--port", "9000"]                    → head [],                   tail ["--port", "9000"]  (no package id detected)
   *   ["/path/to/server.js"]                → head [],                   tail ["/path/to/server.js"]
   */
  function splitLauncherHead(args: string[]): { head: string[]; tail: string[] } {
    const idx = args.findIndex(looksLikePackageIdentifier);
    if (idx >= 0) {
      return { head: args.slice(0, idx + 1), tail: args.slice(idx + 1) };
    }
    return { head: [], tail: args };
  }

  /** True when an env-map value is a redacted secret placeholder. */
  function isSecretPlaceholder(value: string): boolean {
    return /^\$\{APOLLIA_SECRET:.+\}$/.test(value);
  }

  /** True when an env-map value is the dynamic OAuth placeholder. The token
   *  itself lives in the keychain under `mcp_oauth:{server_name}`; the
   *  transport resolves it through `apollia-auth::ensure_fresh_token` at each
   *  request. */
  function isOAuthPlaceholder(value: string): boolean {
    return value === "${APOLLIA_OAUTH}";
  }

  // ── Load raw config ──────────────────────────────────────────────────────────

  async function load(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      raw = await invoke<RawConfig>("get_mcp_server_raw_config", {
        name: serverName,
      });
      const split = splitLauncherHead(raw.args ?? []);
      argsTailDraft = split.tail.join("\n");
      urlDraft = raw.url ?? "";
      initTimeoutDraft = raw.init_timeout_secs;
      callTimeoutDraft = raw.call_timeout_secs;

      // Hydrate env drafts - plain values become editable, secrets stay as
      // placeholders with a separate rotation field initialised empty, OAuth
      // placeholders are surfaced separately so the operator can re-trigger
      // the sign-in flow without seeing them as "static secrets".
      const next: Record<string, string> = {};
      const rot: Record<string, string> = {};
      const oauthHeaders: string[] = [];
      for (const [k, v] of Object.entries(raw.env ?? {})) {
        if (isOAuthPlaceholder(v)) {
          oauthHeaders.push(k);
        } else if (isSecretPlaceholder(v)) {
          rot[k] = "";
        } else {
          next[k] = v;
        }
      }
      envValuesDraft = next;
      secretRotateValues = rot;
      oauthEnvHeaders = oauthHeaders;
      saveError = null;
      saveOk = false;
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (serverName) {
      void load();
    }
  });

  // ── Derived UI state ─────────────────────────────────────────────────────────

  const launcherHead = $derived(
    raw ? splitLauncherHead(raw.args ?? []).head : [],
  );
  const stdio = $derived(raw?.transport === "stdio");
  const remote = $derived(
    raw?.transport === "streamable-http" || raw?.transport === "sse",
  );
  const plainEnvKeys = $derived(Object.keys(envValuesDraft).sort());
  const secretEnvKeys = $derived(Object.keys(secretRotateValues).sort());

  // ── OAuth reconnect ────────────────────────────────────────

  /** Re-run the MCP OAuth flow for this server. Re-uses `mcp_oauth_login` -
   *  idempotent at the orchestrator level: a fresh token simply overwrites the
   *  one persisted under `mcp_oauth:{server_name}`. */
  async function reconnectOAuth(): Promise<void> {
    if (!raw || !remote) return;
    oauthReconnecting = true;
    oauthReconnectError = null;
    oauthReconnectSuccess = false;
    try {
      await invoke("mcp_oauth_login", {
        serverName: raw.name,
        serverUrl: raw.url ?? "",
        wwwAuthenticate: null,
        scopes: [],
      });
      oauthReconnectSuccess = true;
      onSaved?.();
    } catch (err: unknown) {
      oauthReconnectError = err instanceof Error ? err.message : String(err);
    } finally {
      oauthReconnecting = false;
    }
  }

  // ── Save ─────────────────────────────────────────────────────────────────────

  function buildNextConfig(): RawConfig | null {
    if (!raw) return null;
    const tail = argsTailDraft
      .split(/\r?\n|\s+/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    const nextArgs = stdio ? [...launcherHead, ...tail] : raw.args;

    // Rebuild env: keep secret placeholders verbatim (the underlying secret is
    // updated via `store_mcp_secret` separately) + apply edited plain values.
    const nextEnv: Record<string, string> = {};
    for (const [k, v] of Object.entries(raw.env ?? {})) {
      if (isSecretPlaceholder(v)) {
        nextEnv[k] = v;
      } else if (k in envValuesDraft) {
        nextEnv[k] = envValuesDraft[k];
      } else {
        nextEnv[k] = v;
      }
    }

    return {
      ...raw,
      args: nextArgs,
      env: nextEnv,
      url: remote ? (urlDraft.trim() || null) : raw.url,
      init_timeout_secs: initTimeoutDraft,
      call_timeout_secs: callTimeoutDraft,
    };
  }

  async function save(): Promise<void> {
    if (!raw) return;
    saving = true;
    saveError = null;
    saveOk = false;
    try {
      // 1. Rotate any provided secret values before the config update so a
      //    successful save observes the new value on first connect.
      for (const [key, value] of Object.entries(secretRotateValues)) {
        if (value.trim().length === 0) continue;
        await invoke("store_mcp_secret", {
          serverName,
          envVar: key,
          value,
        });
      }
      // 2. PUT the updated config - the runtime does remove → add → persist
      //    and restarts the server with the new parameters.
      const next = buildNextConfig();
      if (!next) return;
      await invoke("update_mcp_server_config", {
        name: serverName,
        config: next,
      });
      saveOk = true;
      onSaved?.();
      // Re-fetch so any backend-side normalisation is reflected in the form.
      await load();
    } catch (err: unknown) {
      saveError = err instanceof Error ? err.message : String(err);
    } finally {
      saving = false;
    }
  }

  function reset(): void {
    if (saving) return;
    void load();
  }

  // ── Validation ───────────────────────────────────────────────────────────────

  const initTimeoutInvalid = $derived(
    !Number.isFinite(initTimeoutDraft) ||
      initTimeoutDraft < 1 ||
      initTimeoutDraft > 300,
  );
  const callTimeoutInvalid = $derived(
    !Number.isFinite(callTimeoutDraft) ||
      callTimeoutDraft < 1 ||
      callTimeoutDraft > 600,
  );
  const urlInvalid = $derived(
    remote &&
      urlDraft.trim().length > 0 &&
      !urlDraft.trim().startsWith("http://") &&
      !urlDraft.trim().startsWith("https://"),
  );
  const canSave = $derived(
    !saving &&
      !loading &&
      !initTimeoutInvalid &&
      !callTimeoutInvalid &&
      !urlInvalid,
  );

  // ── Hint detection (best-effort UX for known catalog packages) ───────────────
  // Surfaces a short caption above the args textarea so a fresh operator
  // doesn't need to read the registry README to know what to type.
  const argsHint = $derived.by(() => {
    if (!stdio || !raw) return null;
    const id = launcherHead.findLast?.(looksLikePackageIdentifier);
    if (!id) return null;
    if (id.includes("server-filesystem")) {
      return "Un chemin absolu par ligne - chaque dossier listé devient accessible en lecture/écriture au serveur (ex. `/Users/moi/Documents`).";
    }
    if (id.includes("server-git")) {
      return "Un dépôt par ligne - chemin absolu vers la racine d'un dépôt Git auquel exposer l'historique.";
    }
    if (id.includes("server-postgres")) {
      return "URI de connexion PostgreSQL (`postgres://user:pass@host:5432/db`) en premier argument.";
    }
    if (id.includes("server-sqlite")) {
      return "Chemin absolu vers le fichier `.sqlite` à exposer.";
    }
    return null;
  });
</script>

{#if loading}
  <div class="flex items-center gap-2 text-[12.5px] text-muted-foreground" data-testid="mcp-settings-loading">
    <Spinner size={14} />
    <span>Chargement…</span>
  </div>
{:else if loadError}
  <Card class="border-destructive/30 bg-destructive/5 p-[14px_16px]">
    <p class="text-[12.5px] text-destructive" data-testid="mcp-settings-load-error">{loadError}</p>
  </Card>
{:else if raw}
  <div class="space-y-4">

    {#if stdio}
      <!-- Launcher (read-only) -->
      <Card class="p-[14px_16px] space-y-2">
        <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Lancement
        </div>
        <div class="grid gap-1.5 text-[12px]">
          <div class="flex items-baseline gap-2">
            <span class="w-[88px] shrink-0 text-[10.5px] uppercase tracking-wide text-muted-foreground">Commande</span>
            <span class="font-mono text-foreground truncate" data-testid="mcp-settings-command">{raw.command}</span>
          </div>
          {#if launcherHead.length > 0}
            <div class="flex items-baseline gap-2">
              <span class="w-[88px] shrink-0 text-[10.5px] uppercase tracking-wide text-muted-foreground">Préfixe</span>
              <span class="font-mono text-foreground/80 truncate" title={launcherHead.join(" ")}>
                {launcherHead.join(" ")}
              </span>
            </div>
          {/if}
          <div class="flex items-baseline gap-2">
            <span class="w-[88px] shrink-0 text-[10.5px] uppercase tracking-wide text-muted-foreground">Transport</span>
            <span class="font-mono text-foreground/80">stdio</span>
          </div>
        </div>
        <p class="text-[10.5px] text-muted-foreground leading-[1.5]">
          Ces champs ne sont pas éditables ici - ils proviennent du catalogue. Pour pointer ce serveur vers un autre paquet, désinstallez puis réinstallez via le catalogue.
        </p>
      </Card>

      <!-- User-editable args -->
      <Card class="p-[14px_16px] space-y-2">
        <label for="mcp-settings-args" class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Arguments du serveur
        </label>
        {#if argsHint}
          <p class="text-[11px] text-muted-foreground leading-[1.5]" data-testid="mcp-settings-args-hint">
            {argsHint}
          </p>
        {/if}
        <Textarea
          id="mcp-settings-args"
          rows={Math.max(3, argsTailDraft.split(/\r?\n/).length + 1)}
          bind:value={argsTailDraft}
          placeholder={launcherHead.some((a) => a.includes("server-filesystem"))
            ? "/Users/moi/Documents\n/Users/moi/Desktop"
            : "Un argument par ligne"}
          class="font-mono text-[12px]"
          data-testid="mcp-settings-args-input"
          disabled={saving}
        />
        <p class="text-[10.5px] text-muted-foreground leading-[1.5]">
          Un argument par ligne. Sauvegarder redémarre le serveur immédiatement.
        </p>
      </Card>
    {/if}

    {#if remote}
      <!-- Remote endpoint -->
      <Card class="p-[14px_16px] space-y-2">
        <label for="mcp-settings-url" class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          URL du serveur
        </label>
        <input
          id="mcp-settings-url"
          type="url"
          bind:value={urlDraft}
          class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-[12.5px] font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
          placeholder="https://mcp.example.com/mcp"
          disabled={saving}
          data-testid="mcp-settings-url-input"
        />
        {#if urlInvalid}
          <p class="text-[11px] text-destructive">L'URL doit commencer par <code>http://</code> ou <code>https://</code>.</p>
        {/if}
        <div class="flex items-baseline gap-2 text-[11.5px] text-muted-foreground">
          <span class="text-[10.5px] uppercase tracking-wide">Transport</span>
          <span class="font-mono">{raw.transport}</span>
        </div>
      </Card>
    {/if}

    {#if oauthEnvHeaders.length > 0}
      <!-- OAuth - the token is managed by the orchestrator,
           never exposed to the user. Surface its presence + offer a sign-in
           refresh button so the operator can rotate without uninstalling. -->
      <Card class="p-[14px_16px] space-y-2.5" data-testid="mcp-settings-oauth">
        <div class="flex items-start gap-2">
          <ShieldCheck size={18} class="mt-0.5 shrink-0 text-success" aria-hidden="true" />
          <div class="flex-1 space-y-0.5">
            <p class="text-sm font-medium text-foreground">
              {$t("integrations.wizard.oauth_settings_connected")}
            </p>
            <p class="text-[11.5px] text-muted-foreground leading-[1.5]">
              {#each oauthEnvHeaders as header (header)}
                <span class="font-mono text-[11px]">{header}</span>
                {' '}
              {/each}
              · ${'{APOLLIA_OAUTH}'}
            </p>
          </div>
        </div>
        <div class="flex items-center justify-end gap-2">
          {#if oauthReconnectSuccess}
            <span class="text-[11.5px] text-success" data-testid="mcp-settings-oauth-success">
              ✓
            </span>
          {/if}
          <Button
            variant="outline"
            size="sm"
            onclick={reconnectOAuth}
            disabled={oauthReconnecting || saving}
            data-testid="mcp-settings-oauth-reconnect"
          >
            {#if oauthReconnecting}
              <Spinner size={12} class="mr-1.5" />
              {$t("integrations.wizard.oauth_signin_in_progress")}
            {:else}
              {$t("integrations.wizard.oauth_signin_reconnect")}
            {/if}
          </Button>
        </div>
        {#if oauthReconnectError}
          <p class="text-[11.5px] text-destructive" data-testid="mcp-settings-oauth-error">
            {oauthReconnectError}
          </p>
        {/if}
      </Card>
    {/if}

    {#if plainEnvKeys.length > 0}
      <!-- Plain env vars (non-secret) -->
      <Card class="p-[14px_16px] space-y-2.5">
        <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Variables d'environnement
        </div>
        {#each plainEnvKeys as key (key)}
          <div class="space-y-1">
            <label for={`mcp-settings-env-${key}`} class="text-[11.5px] font-mono text-foreground">
              {key}
            </label>
            <input
              id={`mcp-settings-env-${key}`}
              type="text"
              bind:value={envValuesDraft[key]}
              class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-[12.5px] font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              disabled={saving}
              data-testid={`mcp-settings-env-${key}`}
            />
          </div>
        {/each}
      </Card>
    {/if}

    {#if secretEnvKeys.length > 0}
      <!-- Secret env vars (stored in keychain) -->
      <Card class="p-[14px_16px] space-y-2.5">
        <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Secrets (trousseau)
        </div>
        <p class="text-[11px] text-muted-foreground leading-[1.5]">
          Ces valeurs sont stockées chiffrées dans le trousseau système. Saisir une nouvelle valeur la remplacera ; laisser vide conserve l'actuelle.
        </p>
        {#each secretEnvKeys as key (key)}
          <div class="space-y-1">
            <label for={`mcp-settings-secret-${key}`} class="text-[11.5px] font-mono text-foreground">
              {key}
            </label>
            <input
              id={`mcp-settings-secret-${key}`}
              type="password"
              autocomplete="off"
              bind:value={secretRotateValues[key]}
              class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-[12.5px] font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
              placeholder="Nouvelle valeur (laisser vide pour conserver)"
              disabled={saving}
              data-testid={`mcp-settings-secret-${key}`}
            />
          </div>
        {/each}
      </Card>
    {/if}

    <!-- Timeouts -->
    <Card class="p-[14px_16px] space-y-2.5">
      <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
        Délais d'attente
      </div>
      <div class="grid grid-cols-2 gap-3">
        <div class="space-y-1">
          <label for="mcp-settings-init-timeout" class="text-[11.5px] text-foreground">
            Connexion initiale (s)
          </label>
          <input
            id="mcp-settings-init-timeout"
            type="number"
            min="1"
            max="300"
            bind:value={initTimeoutDraft}
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-[12.5px] font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            disabled={saving}
            data-testid="mcp-settings-init-timeout"
          />
          {#if initTimeoutInvalid}
            <p class="text-[10.5px] text-destructive">Doit être compris entre 1 et 300 secondes.</p>
          {/if}
        </div>
        <div class="space-y-1">
          <label for="mcp-settings-call-timeout" class="text-[11.5px] text-foreground">
            Appel d'outil (s)
          </label>
          <input
            id="mcp-settings-call-timeout"
            type="number"
            min="1"
            max="600"
            bind:value={callTimeoutDraft}
            class="w-full rounded-md border border-border bg-background px-3 py-1.5 text-[12.5px] font-mono text-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            disabled={saving}
            data-testid="mcp-settings-call-timeout"
          />
          {#if callTimeoutInvalid}
            <p class="text-[10.5px] text-destructive">Doit être compris entre 1 et 600 secondes.</p>
          {/if}
        </div>
      </div>
    </Card>

    {#if saveError}
      <p class="text-[12px] text-destructive" data-testid="mcp-settings-save-error">{saveError}</p>
    {/if}
    {#if saveOk}
      <p class="text-[12px] text-success" data-testid="mcp-settings-save-ok">
        Paramètres enregistrés. Le serveur a été redémarré.
      </p>
    {/if}

    <div class="flex justify-end gap-2 pt-1">
      <Button variant="outline" size="sm" onclick={reset} disabled={saving || loading} data-testid="mcp-settings-reset">
        Réinitialiser
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={save}
        disabled={!canSave}
        data-testid="mcp-settings-save"
      >
        {#if saving}
          <Spinner size={12} class="mr-1.5" />
          Enregistrement…
        {:else}
          Enregistrer
        {/if}
      </Button>
    </div>
  </div>
{/if}
