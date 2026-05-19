<script lang="ts">
  /**
   * Google Drive Picker dialog — lets the user designate folders Apollia
   * agents can read/write. Uses the official Google Picker JS widget so
   * we stay within the free-tier `drive.file` scope (no CASA needed).
   *
   * Boot sequence:
   *   1. fetch a fresh OAuth access_token + api_key + app_id from the
   *      Rust backend (`oauth_google_picker_session`).
   *   2. load `https://apis.google.com/js/api.js` + `picker.js`.
   *   3. construct the Picker with folder-mode views.
   *   4. on pick, persist each folder via `oauth_add_picked_drive_folder`
   *      then re-emit `picked` so the parent can refresh its list.
   *
   * Closing the dialog (Escape / outside click) is a clean no-op; the
   * Picker JS owns its own modal and we just toggle visibility.
   */

  import { invoke } from "@tauri-apps/api/core";
  import { Button } from "$lib/components/ui/button";
  import { Spinner } from "$lib/components/ui/progress";

  interface PickerSession {
    access_token: string;
    app_id: string;
    api_key: string;
    account_id: string;
  }

  interface Props {
    open: boolean;
    /** Optional account id; when null, backend uses the first connected one. */
    accountId?: string | null;
    onclose: () => void;
    /** Called after the user picked and the folders were persisted. */
    onpicked: (count: number) => void;
  }

  let { open, accountId = null, onclose, onpicked }: Props = $props();

  let booting = $state(false);
  let bootError = $state<string | null>(null);
  let picking = $state(false);
  /**
   * The Picker's own iframe is opaque — if Google rejects the api_key it
   * silently 401s in the browser console without calling our callback.
   * We surface a manual escape hatch after a delay so the user isn't
   * stuck on a blank page.
   */
  let pickerLikelyStuck = $state(false);
  let stuckTimer: ReturnType<typeof setTimeout> | null = null;

  /** Google JS globals — loaded lazily. Marked any to keep this file
   *  drop-in without typing the whole gapi surface. */
  type WindowWithGapi = Window & {
    gapi?: { load: (name: string, cb: () => void) => void };
    google?: {
      picker?: {
        PickerBuilder: new () => any;
        ViewId: { FOLDERS: string; DOCS: string };
        Action: { PICKED: string; CANCEL: string };
        DocsView: new (viewId: string) => any;
      };
    };
  };

  function loadScript(src: string): Promise<void> {
    return new Promise((resolve, reject) => {
      // Already loaded?
      if (document.querySelector(`script[src="${src}"]`)) {
        resolve();
        return;
      }
      const tag = document.createElement("script");
      tag.src = src;
      tag.async = true;
      tag.onload = () => resolve();
      tag.onerror = () => reject(new Error(`failed to load ${src}`));
      document.head.appendChild(tag);
    });
  }

  async function ensurePickerLib(): Promise<void> {
    await loadScript("https://apis.google.com/js/api.js");
    await new Promise<void>((resolve) => {
      const w = window as WindowWithGapi;
      w.gapi!.load("picker", () => resolve());
    });
  }

  async function startPicker(): Promise<void> {
    if (booting || picking) return;
    booting = true;
    bootError = null;
    try {
      const session: PickerSession = await invoke("oauth_google_picker_session", {
        accountId,
      });
      await ensurePickerLib();
      const w = window as WindowWithGapi;
      const picker = w.google!.picker!;

      const folderView = new picker.DocsView(picker.ViewId.FOLDERS);
      folderView.setSelectFolderEnabled(true);
      folderView.setIncludeFolders(true);
      folderView.setMimeTypes("application/vnd.google-apps.folder");

      // Second view exposes "Shared with me" so the user can pick folders
      // they don't own but have access to.
      const sharedView = new picker.DocsView(picker.ViewId.FOLDERS);
      sharedView.setSelectFolderEnabled(true);
      sharedView.setIncludeFolders(true);
      sharedView.setOwnedByMe(false);

      const builder = new picker.PickerBuilder()
        .setOAuthToken(session.access_token)
        .setDeveloperKey(session.api_key)
        .setAppId(session.app_id)
        .addView(folderView)
        .addView(sharedView)
        .setCallback(async (data: any) => {
          // Any callback means the Picker isn't stuck — clear the watchdog.
          if (stuckTimer) {
            clearTimeout(stuckTimer);
            stuckTimer = null;
          }
          pickerLikelyStuck = false;
          if (data.action === picker.Action.PICKED) {
            picking = true;
            try {
              const docs: Array<{ id: string; name: string; mimeType: string }> =
                data.docs ?? [];
              for (const d of docs) {
                await invoke("oauth_add_picked_drive_folder", {
                  accountId: session.account_id,
                  folderId: d.id,
                  folderName: d.name,
                  mimeType: d.mimeType,
                });
              }
              onpicked(docs.length);
              onclose();
            } catch (e) {
              bootError = e instanceof Error ? e.message : String(e);
            } finally {
              picking = false;
            }
          } else if (data.action === picker.Action.CANCEL) {
            onclose();
          }
        });
      // setVisible(true) renders Google's own modal over our DOM.
      builder.build().setVisible(true);

      // The Picker iframe can 401 silently when the api_key is rejected
      // (HTTP referrer restrictions, Picker API not enabled, etc.). After
      // 6s with no callback we surface a manual escape hatch.
      if (stuckTimer) clearTimeout(stuckTimer);
      stuckTimer = setTimeout(() => {
        pickerLikelyStuck = true;
      }, 6000);
    } catch (e) {
      bootError = e instanceof Error ? e.message : String(e);
    } finally {
      booting = false;
    }
  }

  function dismissStuck(): void {
    if (stuckTimer) {
      clearTimeout(stuckTimer);
      stuckTimer = null;
    }
    pickerLikelyStuck = false;
    onclose();
  }

  $effect(() => {
    if (open) {
      void startPicker();
    } else {
      // Reset watchdog when the parent closes us.
      if (stuckTimer) {
        clearTimeout(stuckTimer);
        stuckTimer = null;
      }
      pickerLikelyStuck = false;
    }
  });
</script>

{#if open && (booting || bootError || pickerLikelyStuck)}
  <div
    class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 p-4"
    role="dialog"
    aria-modal="true"
    data-testid="google-drive-picker-status"
  >
    <div class="w-full max-w-md rounded-xl border border-border bg-surface-1 p-5 space-y-3">
      <h2 class="text-base font-semibold">Sélecteur Google Drive</h2>
      {#if booting}
        <div class="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner size={14} />
          Chargement du sélecteur Google…
        </div>
      {/if}
      {#if bootError}
        <div class="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-xs text-destructive whitespace-pre-line" data-testid="picker-error">
          {bootError}
        </div>
      {/if}
      {#if pickerLikelyStuck && !bootError}
        <div class="rounded-md border border-amber-500/40 bg-amber-500/10 p-3 text-xs text-amber-900 dark:text-amber-200 space-y-2" data-testid="picker-stuck-hint">
          <p class="font-medium">Le sélecteur ne répond pas.</p>
          <p>
            Google a probablement refusé la clé API (souvent un 401 visible dans
            la console DevTools). Vérifiez dans Google Cloud Console :
          </p>
          <ul class="list-disc pl-4 space-y-0.5">
            <li>
              <strong>Picker API</strong> doit être activée
              (<code>APIs &amp; Services → Library → Picker API</code>).
            </li>
            <li>
              La clé doit être sans <strong>restriction de référent HTTP</strong>
              (le webview Tauri n'envoie pas une origine HTTPS reconnue), ou
              autoriser explicitement <code>tauri://localhost/*</code> ET
              <code>http://tauri.localhost/*</code>.
            </li>
            <li>
              <strong>Restrictions d'API</strong> : la clé doit inclure
              <code>Google Picker API</code> et <code>Google Drive API</code>.
            </li>
            <li>
              Si vous venez d'activer la Picker API, attendez 1–2 minutes que
              Google propage la config.
            </li>
          </ul>
        </div>
      {/if}
      <div class="flex justify-end">
        <Button variant="outline" size="sm" onclick={dismissStuck}>Fermer</Button>
      </div>
    </div>
  </div>
{/if}
