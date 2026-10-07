<script lang="ts">
  /**
   * NativeOauthDialog - the native connector OAuth flow.
   *
   * Self-contained: on open it starts the loopback flow, listens for the
   * `oauth://code-ready` / `oauth://error` events, and on success (Google)
   * advances to the Drive-folder step; then, when the parent can try it, to
   * the account added and its "Try". Uses the canonical `Dialog` (bits-ui
   * focus trap + portal), not a hand-rolled overlay.
   */
  import { t } from "svelte-i18n";
  import { CircleCheck, Play } from "lucide-svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import OauthAuthStep from "./OauthAuthStep.svelte";
  import OauthDriveFolderStep from "./OauthDriveFolderStep.svelte";
  import OauthPolicyRefusal from "./OauthPolicyRefusal.svelte";
  import {
    formatTauriError,
    isMissingClientError,
    isMissingSecretError,
    isRefusedByPolicy,
  } from "$lib/connections/errors";
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
    /** Bring the connection's OAuth client into view, on its page. */
    onopenclient: (provider: ProviderId) => void;
    /** "Try" the account just connected. */
    ontry?: (provider: ProviderId, accountId: string) => void;
  }

  let { open, provider, onclose, oncompleted, onopenclient, ontry }: Props = $props();

  let authUrl = $state<string | null>(null);
  let flowState = $state<string | null>(null);
  let pastedCode = $state("");
  let error = $state<string | null>(null);
  let errorIsMissingClient = $state(false);
  // The organisation policy refused the connection. Said in its own box, not
  // as an error: retrying cannot change it, only the policy can.
  let errorIsPolicyRefusal = $state(false);
  let errorIsMissingSecret = $state(false);
  let busy = $state(false);
  let awaitingCallback = $state(false);
  let step = $state<"auth" | "drive_folder" | "done">("auth");
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
      : step === "done"
        ? $t("connections.try.native_added_title", { values: { provider: providerShort } })
        : $t("connections.oauth_connect_title", { values: { provider: providerName } }),
  );

  /** The flow is over: the account added and its "Try" when offered, else closed. */
  function finish(): void {
    if (ontry && connectedAccountId) step = "done";
    else close();
  }

  function tryAccount(): void {
    const target = provider;
    const account = connectedAccountId;
    close();
    if (target && account) ontry?.(target, account);
  }

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
    errorIsPolicyRefusal = false;
    errorIsMissingSecret = false;
    busy = true;
    awaitingCallback = false;
    step = "auth";
    connectedAccountId = null;
    driveFolderDraft = "";
    driveFolderError = null;
    try {
      const scopes =
        provider === "google"
          ? // "mail.drafts" and not "mail.compose": the only draft operation wired in
            // the bridge is gmail.compose_draft, a POST on {BASE}/drafts, which
            // gmail.drafts.create covers. Asking for the wider compose scope
            // contradicted the published policy without buying an operation.
            // The six aliases below were checked green in the capability list
            // while no scope carried their right; they exist on both sides of
            // the bridge, so asking for them makes the list true.
            [
              "mail.send",
              "mail.drafts",
              "calendar.read",
              "calendar.write",
              "drive.workspace",
              "sheets",
              "docs",
              "slides",
              "tasks",
              "forms",
            ]
          : // "mail.readwrite" carries outlook.move: Graph refuses the move
            // action on Mail.Read alone, so a consent without it left that
            // operation advertised and unusable.
            ["mail.read", "mail.readwrite", "mail.send", "calendar.read", "calendar.write", "files.read"];
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
      errorIsMissingSecret = isMissingSecretError(e);
      errorIsPolicyRefusal = isRefusedByPolicy(e);
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
      connectedAccountId = result.account_id;
      if (result.provider === "google") {
        driveFolderDraft = "Apollia";
        step = "drive_folder";
      } else {
        finish();
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
      finish();
    } catch (e) {
      driveFolderError = formatTauriError(e, $t);
    } finally {
      driveFolderSaving = false;
    }
  }

  function openClientSection(): void {
    const target = provider;
    close();
    if (target) onopenclient(target);
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
  {#if step === "done"}
    <p class="flex items-center gap-2 text-body-sm text-foreground" data-testid="oauth-done">
      <CircleCheck size={14} class="shrink-0 text-success" aria-hidden="true" />
      {$t("connections.try.native_added_body", { values: { account: connectedAccountId ?? "" } })}
    </p>
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={tryAccount} data-testid="oauth-done-try">
        {#snippet icon()}<Play size={12} />{/snippet}
        {$t("connections.try.action")}
      </Button>
      <Button variant="primary-solid" size="sm" onclick={close} data-testid="oauth-done-close">
        {$t("connections.try.done")}
      </Button>
    </div>
  {:else if step === "drive_folder"}
    <OauthDriveFolderStep bind:draft={driveFolderDraft} saving={driveFolderSaving} error={driveFolderError} />
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="ghost" size="sm" onclick={finish} disabled={driveFolderSaving}>
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
  {:else if errorIsMissingSecret}
    <!-- The operator already pasted a client id, so the copy names the half
         that is missing instead of sending them back to square one. -->
    <Banner variant="warning" title={$t("connections.oauth_missing_secret_title")}>
      {@html $t("connections.oauth_missing_secret_body_html", {
        values: { provider: providerShort },
      })}
    </Banner>
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="outline" size="sm" onclick={close} data-testid="oauth-cancel-btn">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        onclick={openClientSection}
        data-testid="oauth-open-settings-btn"
      >
        {$t("connections.open_connection_client")}
      </Button>
    </div>
  {:else if errorIsPolicyRefusal}
    <OauthPolicyRefusal {providerShort} />
    <div class="mt-4 flex justify-end gap-2">
      <Button variant="primary-solid" size="sm" onclick={close} data-testid="oauth-policy-refused-close-btn">
        {$t("common.close")}
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
      <Button variant="primary-solid" size="sm" onclick={openClientSection} data-testid="oauth-open-settings-btn">
        {$t("connections.open_connection_client")}
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
