<script lang="ts">
  /**
   * NativeOauthDialog - the native connector OAuth flow.
   *
   * Self-contained: on open it starts the loopback flow, listens for the
   * `oauth://code-ready` / `oauth://error` events, and on success (Google)
   * advances to the Drive-folder step. Uses the canonical `Dialog` (bits-ui
   * focus trap + portal), not a hand-rolled overlay.
   */
  import { t } from "svelte-i18n";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import OauthAuthStep from "./OauthAuthStep.svelte";
  import OauthDriveFolderStep from "./OauthDriveFolderStep.svelte";
  import { navigateToSettings } from "$lib/router";
  import { formatTauriError, isMissingClientError } from "$lib/connections/errors";
  import {
    oauthStartFlow,
    oauthCompleteFlow,
    oauthSetDriveFolder,
    type ProviderId,
  } from "$lib/ipc/connections";

  interface Props {
    open: boolean;
    provider: ProviderId | null;
    onclose: () => void;
    /** Refresh the native account list once a connection completes. */
    oncompleted: () => void;
  }

  let { open, provider, onclose, oncompleted }: Props = $props();

  let authUrl = $state<string | null>(null);
  let flowState = $state<string | null>(null);
  let pastedCode = $state("");
  let error = $state<string | null>(null);
  let errorIsMissingClient = $state(false);
  let busy = $state(false);
  let awaitingCallback = $state(false);
  let step = $state<"auth" | "drive_folder">("auth");
  let connectedAccountId = $state<string | null>(null);
  let driveFolderDraft = $state("");
  let driveFolderSaving = $state(false);
  let driveFolderError = $state<string | null>(null);
  let unlistenFns: UnlistenFn[] = [];
  let wasOpen = false;

  const providerName = $derived(
    provider === "google" ? "Google Workspace" : "Microsoft 365",
  );
  const providerShort = $derived(provider === "google" ? "Google" : "Microsoft");
  const dialogTitle = $derived(
    step === "drive_folder"
      ? $t("connections.oauth_drive_folder_title")
      : $t("connections.oauth_connect_title", { values: { provider: providerName } }),
  );

  async function clearListeners(): Promise<void> {
    const fns = unlistenFns;
    unlistenFns = [];
    for (const fn of fns) {
      try {
        fn();
      } catch {
        /* best-effort cleanup */
      }
    }
  }

  async function startFlow(): Promise<void> {
    if (!provider) return;
    await clearListeners();
    authUrl = null;
    flowState = null;
    pastedCode = "";
    error = null;
    errorIsMissingClient = false;
    busy = true;
    awaitingCallback = false;
    step = "auth";
    connectedAccountId = null;
    driveFolderDraft = "";
    driveFolderError = null;
    try {
      const scopes =
        provider === "google"
          ? ["mail.send", "mail.compose", "calendar.read", "calendar.write", "drive.workspace"]
          : ["mail.read", "mail.send", "calendar.read", "calendar.write", "files.read"];
      const startResult = await oauthStartFlow(provider, scopes);
      authUrl = startResult.auth_url;
      flowState = startResult.state;
      awaitingCallback = true;

      const expectedState = startResult.state;
      const codeUnlisten = await listen<{ state: string; code: string }>(
        "oauth://code-ready",
        (event) => {
          if (event.payload?.state !== expectedState) return;
          pastedCode = event.payload.code;
          awaitingCallback = false;
          void completeFlow();
        },
      );
      const errUnlisten = await listen<{ state: string; error: string }>(
        "oauth://error",
        (event) => {
          if (event.payload?.state !== expectedState) return;
          awaitingCallback = false;
          error = event.payload.error;
        },
      );
      unlistenFns = [codeUnlisten, errUnlisten];
    } catch (e) {
      error = formatTauriError(e, $t);
      errorIsMissingClient = isMissingClientError(e);
    } finally {
      busy = false;
    }
  }

  async function completeFlow(): Promise<void> {
    if (!flowState || !pastedCode) return;
    busy = true;
    error = null;
    try {
      const result = await oauthCompleteFlow(flowState, pastedCode.trim());
      oncompleted();
      if (result.provider === "google") {
        connectedAccountId = result.account_id;
        driveFolderDraft = "Apollia";
        step = "drive_folder";
      } else {
        close();
      }
    } catch (e) {
      error = formatTauriError(e, $t);
    } finally {
      busy = false;
    }
  }

  async function saveDriveFolderAndFinish(): Promise<void> {
    if (!connectedAccountId) {
      close();
      return;
    }
    driveFolderSaving = true;
    driveFolderError = null;
    try {
      await oauthSetDriveFolder(connectedAccountId, driveFolderDraft.trim());
      close();
    } catch (e) {
      driveFolderError = formatTauriError(e, $t);
    } finally {
      driveFolderSaving = false;
    }
  }

  function openSettingsIntegrations(): void {
    close();
    navigateToSettings("integrations");
  }

  function close(): void {
    awaitingCallback = false;
    void clearListeners();
    onclose();
  }

  // Kick off the flow when the dialog opens; tear down when it closes.
  $effect(() => {
    if (open && !wasOpen) {
      void startFlow();
    } else if (!open && wasOpen) {
      void clearListeners();
    }
    wasOpen = open;
  });
</script>

<Dialog {open} onclose={close} size="sm" title={dialogTitle} data-testid="oauth-dialog">
  {#if step === "drive_folder"}
    <OauthDriveFolderStep bind:draft={driveFolderDraft} saving={driveFolderSaving} error={driveFolderError} />
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="ghost" size="sm" onclick={close} disabled={driveFolderSaving}>
        {$t("connections.keep_default")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={saveDriveFolderAndFinish}
        disabled={driveFolderSaving}
        data-testid="oauth-drive-folder-save"
      >
        {driveFolderSaving ? $t("connections.saving") : $t("connections.save")}
      </Button>
    </div>
  {:else if errorIsMissingClient}
    <Banner variant="warning" title={$t("connections.oauth_missing_client_title")}>
      {@html $t("connections.oauth_missing_client_body_html", {
        values: {
          provider: providerShort,
          console: provider === "google" ? "Google Cloud" : "Azure / Entra ID",
        },
      })}
    </Banner>
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={close} data-testid="oauth-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={openSettingsIntegrations} data-testid="oauth-open-settings-btn">
        {$t("connections.open_settings_integrations")}
      </Button>
    </div>
  {:else if busy && !authUrl}
    <p class="text-body-sm text-muted-foreground">{$t("connections.oauth_preparing")}</p>
  {:else if error && !authUrl}
    <p class="text-body-sm text-destructive">{error}</p>
    <div class="mt-4 flex justify-end">
      <Button variant="outline" size="sm" onclick={close} data-testid="oauth-cancel-btn">{$t("common.cancel")}</Button>
    </div>
  {:else if authUrl}
    <OauthAuthStep {providerShort} {authUrl} {awaitingCallback} {busy} bind:pastedCode {error} />
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={close} data-testid="oauth-cancel-btn">{$t("common.cancel")}</Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={completeFlow}
        disabled={busy || pastedCode.trim().length === 0}
        data-testid="oauth-finalize-btn"
      >
        {busy ? $t("common.finalizing") : $t("common.finalize")}
      </Button>
    </div>
  {/if}
</Dialog>
