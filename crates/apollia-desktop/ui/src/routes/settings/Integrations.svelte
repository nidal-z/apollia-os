<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { Input } from "$lib/components/ui/input";
  import GoogleDrivePicker from "../../components/integrations/GoogleDrivePicker.svelte";

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
    has_client_secret: boolean;
    client_secret_source: "env" | "file" | "builtin" | "none";
    requires_client_secret: boolean;
    has_api_key: boolean;
    api_key_source: "env" | "file" | "builtin" | "none";
    requires_api_key: boolean;
  }

  interface PickedFolderView {
    id: string;
    name: string;
    mime_type: string;
  }

  const PROVIDER_LABELS: Record<string, string> = {
    google: "Google Workspace",
    microsoft: "Microsoft 365",
  };

  let statuses = $state<OauthClientIdStatus[]>([]);
  let drafts = $state<Record<string, string>>({});
  let secretDrafts = $state<Record<string, string>>({});
  let loading = $state(false);
  let savingProvider = $state<string | null>(null);
  let savingSecretProvider = $state<string | null>(null);
  let error = $state<string | null>(null);
  let info = $state<string | null>(null);

  /** Drive folder configuration loaded from
   *  `~/.apollia/drive-prefs.toml`. Only Google has it today; Microsoft
   *  ignores the entry. Keyed by account_id. */
  interface DriveFolderStatus {
    account_id: string;
    folder_path: string | null;
    effective_folder_path: string;
  }
  let driveFolders = $state<DriveFolderStatus[]>([]);
  let driveFolderDrafts = $state<Record<string, string>>({});
  let savingDriveFolderAccount = $state<string | null>(null);

  /** Per-account list of folders the user picked via Google Picker. Loaded
   *  alongside `driveFolders` so the UI can render add/remove controls. */
  let pickedFoldersByAccount = $state<Record<string, PickedFolderView[]>>({});
  let pickerOpenForAccount = $state<string | null>(null);
  let apiKeyDrafts = $state<Record<string, string>>({});
  let savingApiKeyProvider = $state<string | null>(null);

  async function refresh(): Promise<void> {
    loading = true;
    error = null;
    try {
      const fresh = await invoke<OauthClientIdStatus[]>("oauth_list_client_ids");
      statuses = fresh;
      // Seed drafts from the override file, not from the effective value -
      // we want users to see "empty input means: no override, fall back to
      // the next step of the resolution chain" rather than pre-filling
      // their input with a value they did not choose.
      drafts = Object.fromEntries(
        fresh.map((s) => [s.provider, s.override_client_id ?? ""]),
      );
      // Secret drafts always start empty - we never echo the stored secret
      // back to the UI (write-only). The "Configured ✓" badge tells the user
      // whether one is set; pasting again replaces it.
      secretDrafts = Object.fromEntries(fresh.map((s) => [s.provider, ""]));

      // Drive folder list - only meaningful when at least one Google account
      // is connected. Empty when none, in which case the UI hides the block.
      try {
        const folders = await invoke<DriveFolderStatus[]>("oauth_list_drive_folders");
        driveFolders = folders;
        driveFolderDrafts = Object.fromEntries(
          folders.map((f) => [
            f.account_id,
            f.folder_path ?? f.effective_folder_path,
          ]),
        );
        // Picker-granted folder list per account.
        const all: Record<string, PickedFolderView[]> = {};
        for (const f of folders) {
          try {
            const picked = await invoke<PickedFolderView[]>(
              "oauth_list_picked_drive_folders",
              { accountId: f.account_id },
            );
            all[f.account_id] = picked;
          } catch {
            all[f.account_id] = [];
          }
        }
        pickedFoldersByAccount = all;
      } catch {
        driveFolders = [];
        driveFolderDrafts = {};
        pickedFoldersByAccount = {};
      }
      // API key drafts always start empty (write-only - never echoed back).
      apiKeyDrafts = Object.fromEntries(fresh.map((s) => [s.provider, ""]));
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

  async function saveSecret(provider: string): Promise<void> {
    savingSecretProvider = provider;
    error = null;
    info = null;
    try {
      const value = (secretDrafts[provider] ?? "").trim();
      await invoke("oauth_set_client_secret", {
        provider,
        clientSecret: value,
      });
      info =
        value.length === 0
          ? `Secret retiré pour ${PROVIDER_LABELS[provider] ?? provider}.`
          : `Secret enregistré pour ${PROVIDER_LABELS[provider] ?? provider}.`;
      secretDrafts = { ...secretDrafts, [provider]: "" };
      await refresh();
    } catch (e) {
      error = formatError(e);
    } finally {
      savingSecretProvider = null;
    }
  }

  async function clearSecret(provider: string): Promise<void> {
    secretDrafts = { ...secretDrafts, [provider]: "" };
    await invoke("oauth_set_client_secret", { provider, clientSecret: "" });
    info = `Secret retiré pour ${PROVIDER_LABELS[provider] ?? provider}.`;
    await refresh();
  }

  async function saveApiKey(provider: string): Promise<void> {
    savingApiKeyProvider = provider;
    error = null;
    info = null;
    try {
      const value = (apiKeyDrafts[provider] ?? "").trim();
      await invoke("oauth_set_api_key", { provider, apiKey: value });
      info =
        value.length === 0
          ? `Clé API retirée pour ${PROVIDER_LABELS[provider] ?? provider}.`
          : `Clé API enregistrée pour ${PROVIDER_LABELS[provider] ?? provider}.`;
      apiKeyDrafts = { ...apiKeyDrafts, [provider]: "" };
      await refresh();
    } catch (e) {
      error = formatError(e);
    } finally {
      savingApiKeyProvider = null;
    }
  }

  async function clearApiKey(provider: string): Promise<void> {
    apiKeyDrafts = { ...apiKeyDrafts, [provider]: "" };
    await invoke("oauth_set_api_key", { provider, apiKey: "" });
    info = `Clé API retirée pour ${PROVIDER_LABELS[provider] ?? provider}.`;
    await refresh();
  }

  async function removePickedFolder(accountId: string, folderId: string): Promise<void> {
    try {
      await invoke("oauth_remove_picked_drive_folder", {
        accountId,
        folderId,
      });
      info = "Dossier retiré.";
      await refresh();
    } catch (e) {
      error = formatError(e);
    }
  }

  function openPicker(accountId: string): void {
    pickerOpenForAccount = accountId;
  }

  function closePicker(): void {
    pickerOpenForAccount = null;
  }

  async function handlePickerPicked(count: number): Promise<void> {
    pickerOpenForAccount = null;
    info = `${count} dossier${count > 1 ? "s" : ""} ajouté${count > 1 ? "s" : ""}.`;
    await refresh();
  }

  async function saveDriveFolder(accountId: string): Promise<void> {
    savingDriveFolderAccount = accountId;
    error = null;
    info = null;
    try {
      const value = (driveFolderDrafts[accountId] ?? "").trim();
      await invoke("oauth_set_drive_folder", { accountId, folderPath: value });
      info =
        value.length === 0
          ? `Compte ${accountId} : agents écriront directement à la racine de Drive (pas de sous-dossier Apollia).`
          : `Dossier enregistré pour ${accountId} : ${value}.`;
      await refresh();
    } catch (e) {
      error = formatError(e);
    } finally {
      savingDriveFolderAccount = null;
    }
  }

  async function resetDriveFolder(accountId: string): Promise<void> {
    savingDriveFolderAccount = accountId;
    error = null;
    info = null;
    try {
      await invoke("oauth_reset_drive_folder", { accountId });
      info = `Compte ${accountId} : dossier remis au défaut (Apollia).`;
      await refresh();
    } catch (e) {
      error = formatError(e);
    } finally {
      savingDriveFolderAccount = null;
    }
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
      avec PKCE. Côté Microsoft (« public client » conforme à la spec) un
      simple <code>client_id</code> suffit. Côté Google, leur type « Installed
      app / Desktop app » exige aussi un <code>client_secret</code> au token
      endpoint - c'est non-standard, Google documente lui-même que ce secret
      « n'est pas traité comme un secret » pour les apps natives. Vous le
      retrouvez à côté du Client ID dans la console Google Cloud.
    </p>
    <p class="text-xs text-muted-foreground max-w-prose">
      Ordre de résolution :
      <code>APOLLIA_*_CLIENT_ID</code> / <code>_SECRET</code>
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
        <Skeleton class="h-28 rounded-xl bg-surface-1 border border-border" />
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
            <Button variant="outline" size="sm"
              onclick={() => clearOverride(status.provider)}
              disabled={savingProvider !== null ||
                (status.override_client_id ?? "").length === 0}
            >
              Effacer l'override
            </Button>
          </div>

          {#if status.effective_client_id}
            <div class="text-[11px] text-muted-foreground">
              <span class="font-mono">{status.effective_client_id}</span>
            </div>
          {:else}
            <div class="text-[11px] text-destructive">
              Aucun client_id résolu - le bouton « Connecter » échouera avec
              <code>oauth_client_not_configured</code>.
            </div>
          {/if}

          <label class="block text-xs font-medium">
            Override OAuth client_id
            <Input
              type="text"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
              placeholder={status.provider === "google"
                ? "1234567890-abcdefgh.apps.googleusercontent.com"
                : "00000000-0000-0000-0000-000000000000"}
              value={drafts[status.provider] ?? ""}
              oninput={(e) => {
                const v = (e.currentTarget as HTMLInputElement).value;
                drafts = { ...drafts, [status.provider]: v };
              }}
              disabled={savingProvider === status.provider}
              autocomplete="off"
              spellcheck={false}
              data-testid={`oauth-input-${status.provider}`}
             />
          </label>

          <div class="flex justify-end">
            <Button variant="primary-solid" size="sm"
              onclick={() => save(status.provider)}
              disabled={savingProvider === status.provider}
            >
              {savingProvider === status.provider ? "Enregistrement…" : "Enregistrer"}
            </Button>
          </div>

          {#if status.requires_client_secret}
            <!-- Google-only: the Installed App type also requires a
                 client_secret at the token endpoint. Microsoft public
                 clients skip this block entirely. -->
            <div class="mt-3 border-t border-border pt-3 space-y-2">
              <div class="flex items-center justify-between">
                <div>
                  <div class="text-xs font-medium">OAuth client_secret</div>
                  <div class="text-[11px] {status.has_client_secret ? 'text-emerald-700 dark:text-emerald-400' : 'text-destructive'}">
                    {status.has_client_secret
                      ? `Configuré (${sourceLabel(status.client_secret_source)})`
                      : "Non configuré - l'échange de token échouera avec « client_secret is missing »"}
                  </div>
                </div>
                {#if status.has_client_secret}
                  <Button variant="outline" size="sm"
                    onclick={() => clearSecret(status.provider)}
                    disabled={savingSecretProvider !== null}
                  >
                    Effacer le secret
                  </Button>
                {/if}
              </div>

              <label class="block text-xs font-medium">
                Coller un nouveau client_secret
                <Input
                  type="password"
                  class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
                  placeholder="GOCSPX-…"
                  value={secretDrafts[status.provider] ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).value;
                    secretDrafts = { ...secretDrafts, [status.provider]: v };
                  }}
                  disabled={savingSecretProvider === status.provider}
                  autocomplete="off"
                  spellcheck={false}
                  data-testid={`oauth-secret-input-${status.provider}`}
                />
              </label>
              <p class="text-[11px] text-muted-foreground leading-[1.4]">
                Disponible à côté du Client ID dans la console Google Cloud →
                Credentials → votre OAuth 2.0 Client ID. Stocké dans
                <code>~/.apollia/oauth-clients.toml</code> avec des permissions
                <code>0600</code> ; jamais transmis en clair, jamais affiché ici.
              </p>

              <div class="flex justify-end">
                <Button variant="primary-solid" size="sm"
                  onclick={() => saveSecret(status.provider)}
                  disabled={savingSecretProvider === status.provider
                    || (secretDrafts[status.provider] ?? "").trim().length === 0}
                >
                  {savingSecretProvider === status.provider
                    ? "Enregistrement…"
                    : "Enregistrer le secret"}
                </Button>
              </div>
            </div>
          {/if}

          {#if status.requires_api_key}
            <!-- Google-only: Picker requires a public API key in addition
                 to the OAuth client_id. Stored alongside the secret in
                 oauth-clients.toml. -->
            <div class="mt-3 border-t border-border pt-3 space-y-2">
              <div class="flex items-center justify-between">
                <div>
                  <div class="text-xs font-medium">Google API key (Picker)</div>
                  <div class="text-[11px] {status.has_api_key ? 'text-emerald-700 dark:text-emerald-400' : 'text-muted-foreground'}">
                    {status.has_api_key
                      ? `Configurée (${sourceLabel(status.api_key_source)})`
                      : "Non configurée - le bouton « Ajouter via Picker » échouera tant qu'elle est vide"}
                  </div>
                </div>
                {#if status.has_api_key}
                  <Button variant="outline" size="sm"
                    onclick={() => clearApiKey(status.provider)}
                    disabled={savingApiKeyProvider !== null}
                  >
                    Effacer la clé
                  </Button>
                {/if}
              </div>
              <label class="block text-xs font-medium">
                Coller une nouvelle clé API Google
                <Input
                  type="password"
                  class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
                  placeholder="AIzaSy…"
                  value={apiKeyDrafts[status.provider] ?? ""}
                  oninput={(e) => {
                    const v = (e.currentTarget as HTMLInputElement).value;
                    apiKeyDrafts = { ...apiKeyDrafts, [status.provider]: v };
                  }}
                  disabled={savingApiKeyProvider === status.provider}
                  autocomplete="off"
                  spellcheck={false}
                  data-testid={`oauth-api-key-input-${status.provider}`}
                />
              </label>
              <p class="text-[11px] text-muted-foreground leading-[1.4]">
                Console Google Cloud → APIs & Services → Credentials → Create
                credentials → API key. Restreignez la clé aux APIs <strong>Google
                Picker</strong> + <strong>Google Drive</strong>. Stockée dans
                <code>~/.apollia/oauth-clients.toml</code>, perms 0o600.
              </p>
              <div class="flex justify-end">
                <Button variant="primary-solid" size="sm"
                  onclick={() => saveApiKey(status.provider)}
                  disabled={savingApiKeyProvider === status.provider
                    || (apiKeyDrafts[status.provider] ?? "").trim().length === 0}
                >
                  {savingApiKeyProvider === status.provider
                    ? "Enregistrement…"
                    : "Enregistrer la clé"}
                </Button>
              </div>
            </div>
          {/if}

          {#if status.provider === "google" && driveFolders.length > 0}
            <!-- Per-account Drive folder + Picker-granted folders.
                 Only renders when at least one Google account is connected;
                 each account has its own row so multi-account setups stay clean. -->
            <div class="mt-3 border-t border-border pt-3 space-y-3" data-testid="oauth-drive-folders">
              <div class="text-xs font-medium text-foreground">
                Dossiers Google Drive par compte
              </div>
              <p class="text-[11.5px] text-muted-foreground leading-[1.4]">
                Apollia opère dans : (1) un <em>dossier racine</em> qu'elle crée
                automatiquement (défaut <code>Apollia</code>) ; (2) tout
                dossier que vous lui désignez via le <em>sélecteur Google
                Drive</em> ci-dessous. Pas de scope restreint, pas de CASA -
                Google étend l'accès <code>drive.file</code> au dossier que
                vous picker.
              </p>
              {#each driveFolders as folder (folder.account_id)}
                <div class="rounded-md border border-border bg-background/40 px-3 py-2 space-y-3">
                  <div class="text-[11.5px] text-foreground/85 font-mono">
                    {folder.account_id}
                  </div>

                  <!-- Root folder override (tri-state: default / explicit-root / custom-path) -->
                  <div class="space-y-2">
                    <div class="text-[11px] font-medium text-foreground/80">
                      Dossier racine
                    </div>
                    <div class="text-[10.5px] text-muted-foreground">
                      Effectif :
                      {#if folder.folder_path === ""}
                        <code>(racine de votre Drive)</code>
                        <span class="text-foreground/70">- choix explicite</span>
                      {:else}
                        <code>{folder.effective_folder_path}</code>
                        {#if folder.folder_path === null}
                          <span class="text-foreground/70">(défaut)</span>
                        {:else}
                          <span class="text-foreground/70">(personnalisé)</span>
                        {/if}
                      {/if}
                    </div>
                    <div class="flex gap-2">
                      <Input
                        type="text"
                        class="flex-1 rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
                        placeholder="Apollia"
                        value={driveFolderDrafts[folder.account_id] ?? ""}
                        oninput={(e) => {
                          // Svelte 5 doesn't reliably propagate bind:value
                          // writes through a `$state<Record>` indexed by a
                          // dynamic key inside an {#each}. Re-assign the
                          // whole map so the proxy intercepts the change.
                          const v = (e.currentTarget as HTMLInputElement)
                            .value;
                          driveFolderDrafts = {
                            ...driveFolderDrafts,
                            [folder.account_id]: v,
                          };
                        }}
                        disabled={savingDriveFolderAccount === folder.account_id}
                        autocomplete="off"
                        spellcheck={false}
                        data-testid={`drive-folder-input-${folder.account_id}`}
                      />
                      <Button variant="primary-solid" size="sm"
                        onclick={() => saveDriveFolder(folder.account_id)}
                        disabled={savingDriveFolderAccount === folder.account_id}
                      >
                        {savingDriveFolderAccount === folder.account_id
                          ? "…"
                          : "Enregistrer"}
                      </Button>
                      <Button variant="outline" size="sm"
                        onclick={() => resetDriveFolder(folder.account_id)}
                        disabled={savingDriveFolderAccount === folder.account_id || folder.folder_path === null}
                        data-testid={`drive-folder-reset-${folder.account_id}`}
                      >
                        Défaut
                      </Button>
                    </div>
                    <p class="text-[10.5px] text-muted-foreground leading-relaxed">
                      C'est <strong>l'espace de travail Apollia</strong> dans votre Drive.
                      Tous les agents y créent et y listent leurs fichiers par défaut.
                      Saisissez un chemin (ex. <code>Documents/Apollia</code>) ou
                      <strong>laissez vide et enregistrez</strong> pour utiliser la racine
                      directement. <strong>Défaut</strong> restaure le sous-dossier
                      <code>Apollia</code>. Note Google : le scope <code>drive.file</code>
                      n'expose à Apollia que les fichiers qu'elle a créés (ou que vous lui
                      avez ouverts via le sélecteur) - pas l'ensemble de votre Drive.
                    </p>
                  </div>

                  <!-- Picked folders (Google Picker) - DISABLED v0.1.0 -->
                  <!--
                    Le sélecteur Google charge sa propre iframe qui 401 silencieusement
                    quand l'api_key ne passe pas les restrictions Google Cloud
                    (référent HTTP, Picker API non activée, etc.). On désactive
                    l'entrée utilisateur tant qu'on n'a pas reproduit/débogué
                    proprement. Le code backend et les composants Svelte sont
                    conservés (`oauth_google_picker_session`, `GoogleDrivePicker.svelte`,
                    `oauth_add_picked_drive_folder`, etc.) pour un re-enable rapide.
                  -->
                  {#if false}
                    <div class="space-y-2 pt-2 border-t border-border/40">
                      <div class="flex items-center justify-between">
                        <div class="text-[11px] font-medium text-foreground/80">
                          Dossiers ajoutés via le sélecteur Google
                        </div>
                        <Button variant="outline" size="sm"
                          onclick={() => openPicker(folder.account_id)}
                          disabled={!status.has_api_key}
                          data-testid={`drive-picker-open-${folder.account_id}`}
                        >
                          + Ajouter via Google
                        </Button>
                      </div>
                      {#if (pickedFoldersByAccount[folder.account_id] ?? []).length > 0}
                        <ul class="space-y-1.5">
                          {#each pickedFoldersByAccount[folder.account_id] ?? [] as pf (pf.id)}
                            <li class="flex items-center justify-between gap-2 rounded-md bg-background/60 px-2 py-1.5">
                              <div class="min-w-0">
                                <div class="text-[12px] text-foreground truncate">{pf.name}</div>
                                <div class="text-[10px] text-muted-foreground font-mono truncate">{pf.id}</div>
                              </div>
                              <Button variant="ghost" size="sm"
                                onclick={() => removePickedFolder(folder.account_id, pf.id)}
                                class="text-destructive hover:bg-destructive/10"
                              >
                                Retirer
                              </Button>
                            </li>
                          {/each}
                        </ul>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<GoogleDrivePicker
  open={pickerOpenForAccount !== null}
  accountId={pickerOpenForAccount}
  onclose={closePicker}
  onpicked={handlePickerPicked}
/>
