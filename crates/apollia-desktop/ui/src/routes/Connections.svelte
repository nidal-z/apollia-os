<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { t } from "svelte-i18n";
  import {
    Plus,
    Sparkles,
    Link as LinkIcon,
    Search,
    Wrench,
    RefreshCw,
    Trash2,
  } from "lucide-svelte";
  import {
    StatusDot,
    EmptyState,
    SplitLayout,
    FilterChipBar,
    type ConnectionStatus,
  } from "$lib/components/operator";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Select } from "$lib/components/ui/select";
  import { Textarea } from "$lib/components/ui/textarea";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { Card } from "$lib/components/ui/card";
  import { Spinner } from "$lib/components/ui/progress";
  import { TabBar } from "$lib/components/ui/tabs";
  import { Sheet, SheetHeader, SheetContent } from "$lib/components/ui/sheet";
  import { formatRelativeTime } from "$lib/utils";
  import type {
    AgentListItem,
    ConnectorEnrichmentView,
    McpConnectionTestResponse,
    McpHealth,
    McpServerDetailView,
    McpServerStatusView,
    RegistryServerView,
  } from "$lib/types";
  import McpDisclaimerDialog, {
    isDisclaimerAccepted,
  } from "../components/integrations/McpDisclaimerDialog.svelte";
  import ConnectorWizard from "../components/integrations/ConnectorWizard.svelte";
  import { navigateToSettings } from "$lib/router";
  import { uiMode } from "$lib/stores/mode";
  import ConnectionStatusIndicator from "../components/integrations/ConnectionStatusIndicator.svelte";
  import ConnectionErrorModal from "../components/connections/ConnectionErrorModal.svelte";
  import { rankSuggestions } from "../components/connections/ConnectionSuggestions.svelte";
  import ApprovalLevelSelector from "$lib/components/operator/approval/ApprovalLevelSelector.svelte";
  import McpServerSettingsEditor from "../components/integrations/McpServerSettingsEditor.svelte";
  import { Skeleton } from "$lib/components/ui/skeleton";

  interface Props {
    onNavigateTasks?: () => void;
  }

  let { onNavigateTasks }: Props = $props();

  // ── State (preserved from old Connections.svelte + Integrations.svelte) ────
  let servers = $state<McpServerStatusView[]>([]);
  let enrichmentMap = $state(new Map<string, ConnectorEnrichmentView>());
  let registry = $state<RegistryServerView[]>([]);
  let agents = $state<AgentListItem[]>([]);
  let loading = $state(false);
  let registryLoading = $state(false);
  let loadError = $state<string | null>(null);

  // Catalog pagination - show 48 cards initially, expand 48 at a time.
  const CATALOG_PAGE = 48;
  let catalogLimit = $state(CATALOG_PAGE);

  // Filter state
  type StatusFilter = "all" | "active" | "attention" | "error" | "idle";
  let statusFilter = $state<StatusFilter>("all");
  let mcpFilter = $state<"all" | "installed" | "official" | "community">("all");

  // Modal / wizard state
  let disclaimerOpen = $state(false);
  let selectedRegistryServer = $state<RegistryServerView | null>(null);
  let wizardOpen = $state(false);
  let errorModalOpen = $state(false);
  let errorServer = $state<McpServerStatusView | null>(null);

  // ── Sidebar+detail selection (mirror Projets/Agents) ──────────────────
  // Sidebar entries:
  //   - 2 native connectors (Google, Microsoft) - always shown
  //   - Installed MCP servers - sorted by status
  // Selection is a discriminated union so the right pane knows which tabs
  // to render and which actions to wire.
  type Selection =
    | { kind: "native"; provider: ProviderId }
    | { kind: "mcp"; name: string }
    | null;
  let selection = $state<Selection>(null);
  let sidebarFilter = $state("");

  type NativeTab = "accounts" | "capabilities" | "settings";
  type McpTab = "overview" | "tools" | "settings";
  let nativeTab = $state<NativeTab>("accounts");
  let mcpTab = $state<McpTab>("overview");

  // Catalogue Sheet (replaces the inline catalogue grid + custom MCP dialog)
  type CatalogueTab = "discover" | "custom";
  let catalogueOpen = $state(false);
  let catalogueTab = $state<CatalogueTab>("discover");

  // ── MCP detail state (manage: test / reconnect / disconnect) ──────────
  // When the user selects an MCP server in the sidebar, the detail right
  // pane needs the full McpServerDetailView (tools list, config, env keys,
  // …). We fetch it lazily and re-fetch on selection change.
  type ApprovalLevel = "auto" | "ask" | "readonly";
  let mcpDetail = $state<McpServerDetailView | null>(null);
  let mcpDetailLoading = $state(false);
  let mcpDetailError = $state<string | null>(null);
  let approvalLevel = $state<ApprovalLevel>("ask");
  let approvalPending = $state(false);
  let approvalError = $state<string | null>(null);
  type TestState =
    | "idle"
    | "testing"
    | "ok"
    | "unverified"
    | "degraded"
    | "needs_reauth"
    | "error";
  let testState = $state<TestState>("idle");
  let testToolCount = $state(0);
  /** Builder-mode technical detail captured from the last test. */
  let testDetail = $state<string | null>(null);
  let testError = $state<string | null>(null);
  const isOperator = $derived($uiMode === "operator");
  let reconnecting = $state(false);
  let reconnectError = $state<string | null>(null);
  let confirmDisconnect = $state(false);
  let disconnecting = $state(false);
  let disconnectError = $state<string | null>(null);

  function deriveApprovalLevel(requiresApproval: boolean): ApprovalLevel {
    return requiresApproval ? "ask" : "auto";
  }

  async function fetchMcpDetail(name: string): Promise<void> {
    mcpDetailLoading = true;
    mcpDetailError = null;
    try {
      mcpDetail = await invoke<McpServerDetailView>("get_mcp_server_detail", {
        name,
      });
      approvalLevel = deriveApprovalLevel(mcpDetail.config.requires_approval);
    } catch (err: unknown) {
      mcpDetailError = err instanceof Error ? err.message : String(err);
      mcpDetail = null;
    } finally {
      mcpDetailLoading = false;
    }
  }

  // Re-fetch detail when MCP selection changes; reset volatile state.
  $effect(() => {
    if (selection?.kind !== "mcp") {
      mcpDetail = null;
      return;
    }
    const name = selection.name;
    mcpDetail = null;
    testState = "idle";
    testDetail = null;
    testError = null;
    reconnectError = null;
    confirmDisconnect = false;
    disconnectError = null;
    approvalError = null;
    mcpTab = "overview";
    void fetchMcpDetail(name);
  });

  // Reset native tab when switching to a native connector.
  $effect(() => {
    if (selection?.kind === "native") {
      nativeTab = "accounts";
    }
  });

  /** Capability matrix shown on the "Ce qu'Apollia peut faire" tab. Reads
   *  i18n keys (so FR/EN are surfaced through the user's locale) and groups
   *  operations by service. The booleans below mirror the actual scope set
   *  exposed in `connector_providers.rs` for v0.1.0 - keep both in sync. */
  interface CapabilityEntry {
    labelKey: string;
    detailKey?: string;
    supported: boolean;
  }
  interface CapabilityGroup {
    key: string;
    entries: CapabilityEntry[];
  }
  function capabilityGroupsFor(providerId: ProviderId): CapabilityGroup[] {
    if (providerId === "google") {
      return [
        {
          key: "gmail",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gmail_send",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gmail_compose",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gmail_read_inbox",
              detailKey: "connections.capabilities.entries.gmail_read_inbox_detail",
              supported: false,
            },
            {
              labelKey: "connections.capabilities.entries.gmail_search",
              detailKey: "connections.capabilities.entries.gmail_read_inbox_detail",
              supported: false,
            },
          ],
        },
        {
          key: "gcal",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gcal_list",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gcal_get",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gcal_create",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gcal_update",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gcal_delete",
              supported: true,
            },
          ],
        },
        {
          key: "gdrive",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gdrive_workspace_list",
              detailKey:
                "connections.capabilities.entries.gdrive_workspace_detail",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_workspace_read",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_workspace_write",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_workspace_share",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_picker",
              detailKey:
                "connections.capabilities.entries.gdrive_picker_detail",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_full",
              detailKey: "connections.capabilities.entries.gdrive_full_detail",
              supported: false,
            },
            {
              labelKey: "connections.capabilities.entries.gdrive_write_anywhere",
              detailKey:
                "connections.capabilities.entries.gdrive_write_anywhere_detail",
              supported: false,
            },
          ],
        },
        {
          key: "gsheets",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gsheets_create",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gsheets_read_values",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gsheets_write_values",
              detailKey: "connections.capabilities.entries.gsheets_detail",
              supported: true,
            },
          ],
        },
        {
          key: "gdocs",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gdocs_create",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdocs_read_text",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gdocs_append_text",
              detailKey: "connections.capabilities.entries.gdocs_detail",
              supported: true,
            },
          ],
        },
        {
          key: "gslides",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gslides_create",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gslides_append_slide",
              detailKey: "connections.capabilities.entries.gslides_detail",
              supported: true,
            },
          ],
        },
        {
          key: "gtasks",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gtasks_list",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gtasks_create",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gtasks_complete",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.gtasks_delete",
              detailKey: "connections.capabilities.entries.gtasks_detail",
              supported: true,
            },
          ],
        },
        {
          key: "gforms",
          entries: [
            {
              labelKey: "connections.capabilities.entries.gforms_create",
              detailKey: "connections.capabilities.entries.gforms_detail",
              supported: true,
            },
          ],
        },
        {
          key: "youtube",
          entries: [
            {
              labelKey: "connections.capabilities.entries.youtube_search",
              supported: true,
            },
            {
              labelKey: "connections.capabilities.entries.youtube_video_details",
              detailKey: "connections.capabilities.entries.youtube_detail",
              supported: true,
            },
          ],
        },
      ];
    }
    // Microsoft - capability list mirrors the wired ops in connectors_bridge.rs.
    return [
      {
        key: "outlook_mail",
        entries: [
          {
            labelKey: "connections.capabilities.entries.outlook_search",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_read",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_send",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_reply",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_folders",
            supported: true,
          },
        ],
      },
      {
        key: "outlook_calendar",
        entries: [
          {
            labelKey: "connections.capabilities.entries.outlook_cal_read",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_cal_create",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_cal_update",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.outlook_cal_delete",
            supported: true,
          },
        ],
      },
      {
        key: "onedrive",
        entries: [
          {
            labelKey: "connections.capabilities.entries.onedrive_search",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.onedrive_read",
            detailKey: "connections.capabilities.entries.onedrive_detail",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.onedrive_recent",
            supported: true,
          },
          {
            labelKey: "connections.capabilities.entries.onedrive_write",
            detailKey: "connections.capabilities.entries.onedrive_write_detail",
            supported: false,
          },
        ],
      },
    ];
  }

  async function handleApprovalChange(newLevel: ApprovalLevel): Promise<void> {
    if (!mcpDetail || selection?.kind !== "mcp") return;
    const name = selection.name;
    approvalLevel = newLevel;
    approvalPending = true;
    approvalError = null;
    const requiresApproval = newLevel === "ask";
    try {
      await invoke("set_mcp_server_approval", { name, requiresApproval });
      mcpDetail = {
        ...mcpDetail,
        config: { ...mcpDetail.config, requires_approval: requiresApproval },
        status: { ...mcpDetail.status, requires_approval: requiresApproval },
      };
    } catch (err: unknown) {
      approvalError = err instanceof Error ? err.message : String(err);
      approvalLevel = deriveApprovalLevel(mcpDetail.config.requires_approval);
    } finally {
      approvalPending = false;
    }
  }

  async function handleTest(): Promise<void> {
    if (selection?.kind !== "mcp") return;
    const name = selection.name;
    testState = "testing";
    testError = null;
    testDetail = null;
    try {
      // Re-handshake for reachability plus the connector's read-only probe; the
      // verdict comes from live_health, not a bare "connected" flag.
      const res = await invoke<McpConnectionTestResponse>("test_mcp_live_server", {
        name,
      });
      if (res.kind === "oauth_required") {
        testState = "needs_reauth";
        return;
      }
      testToolCount = res.tools.length;
      const health = res.live_health ?? null;
      // Reflect the verdict on the live badge immediately.
      if (mcpDetail && health) {
        mcpDetail = {
          ...mcpDetail,
          status: { ...mcpDetail.status, health },
        };
      }
      switch (health?.state) {
        case undefined:
        case "healthy":
          testState = health?.state === "healthy" && !health.verified ? "unverified" : "ok";
          break;
        case "degraded":
          testState = "degraded";
          testDetail = `${health.category} - ${health.last_error}`;
          break;
        case "needs_reauth":
          testState = "needs_reauth";
          testDetail = health.reason;
          break;
        case "unavailable":
          testState = "error";
          testError = health.reason;
          break;
      }
    } catch (err: unknown) {
      testState = "error";
      testError = err instanceof Error ? err.message : String(err);
    }
  }

  async function handleReconnect(): Promise<void> {
    if (selection?.kind !== "mcp") return;
    const name = selection.name;
    reconnecting = true;
    reconnectError = null;
    testState = "idle";
    testDetail = null;
    try {
      const updated = await invoke<McpServerStatusView>("restart_mcp_server", {
        name,
      });
      if (mcpDetail) {
        mcpDetail = { ...mcpDetail, status: updated };
      }
      approvalLevel = deriveApprovalLevel(updated.requires_approval);
    } catch (err: unknown) {
      reconnectError = err instanceof Error ? err.message : String(err);
    } finally {
      reconnecting = false;
    }
  }

  async function handleDisconnect(): Promise<void> {
    if (!mcpDetail || selection?.kind !== "mcp") return;
    const name = selection.name;
    disconnecting = true;
    disconnectError = null;
    try {
      for (const envKey of mcpDetail.config.env_keys) {
        await invoke("delete_mcp_secret", { serverName: name, envVar: envKey });
      }
      await invoke("remove_mcp_server", { name });
      selection = null;
      await loadAll();
    } catch (err: unknown) {
      disconnectError = err instanceof Error ? err.message : String(err);
    } finally {
      disconnecting = false;
    }
  }

  const lastActivityLabel = $derived(
    mcpDetail?.status.last_call_at
      ? formatRelativeTime(mcpDetail.status.last_call_at)
      : null,
  );

  // ── Native Apollia connectors (Google Workspace, Microsoft 365) ─────────────
  type ProviderId = "google" | "microsoft";

  interface NativeConnectorCard {
    id: ProviderId;
    name: string;
    description: string;
  }

  interface OauthAccountInfo {
    provider: ProviderId;
    account_id: string;
  }

  const NATIVE_CONNECTORS: NativeConnectorCard[] = $derived([
    {
      id: "google",
      name: "Google Workspace",
      description: $t("connections.native_google_description"),
    },
    {
      id: "microsoft",
      name: "Microsoft 365",
      description: $t("connections.native_microsoft_description"),
    },
  ]);

  let nativeAccounts = $state<OauthAccountInfo[]>([]);
  let nativeLoading = $state(false);
  let nativeError = $state<string | null>(null);
  let oauthDialogOpen = $state(false);
  let oauthDialogProvider = $state<ProviderId | null>(null);
  let oauthDialogAuthUrl = $state<string | null>(null);
  let oauthDialogState = $state<string | null>(null);
  let oauthDialogPastedCode = $state("");
  let oauthDialogError = $state<string | null>(null);
  /** True when the error returned by `oauth_start_flow` is
   *  `oauth_client_not_configured` - surface a CTA toward Settings → Integrations
   *  instead of the generic error message. */
  let oauthDialogErrorIsMissingClient = $state(false);
  let oauthDialogBusy = $state(false);
  /** True while the dialog is waiting for the loopback to fire after the
   *  browser was opened. Drives the "captured automatically" spinner. */
  let oauthDialogAwaitingCallback = $state(false);
  /** Cleanup handles for the Tauri event listeners registered during a flow. */
  let oauthDialogUnlistenFns: UnlistenFn[] = [];
  /** Post-OAuth Drive folder step (Google only). Drives the second screen
   *  of the wizard where the user picks the Drive root path Apollia will
   *  use for this account. Empty string keeps the legacy `Apollia` default. */
  let oauthDialogStep = $state<"auth" | "drive_folder">("auth");
  let oauthDialogConnectedAccountId = $state<string | null>(null);
  let oauthDialogDriveFolderDraft = $state("");
  let oauthDialogDriveFolderSaving = $state(false);
  let oauthDialogDriveFolderError = $state<string | null>(null);

  async function refreshNativeStatus() {
    nativeLoading = true;
    nativeError = null;
    try {
      nativeAccounts = await invoke<OauthAccountInfo[]>("oauth_get_status");
    } catch (e) {
      nativeError = formatTauriError(e);
    } finally {
      nativeLoading = false;
    }
  }

  function accountsForProvider(provider: ProviderId): OauthAccountInfo[] {
    return nativeAccounts.filter((a) => a.provider === provider);
  }

  function formatTauriError(e: unknown): string {
    if (typeof e === "string") return e;
    const anyE = e as { kind?: string; detail?: string; message?: string };
    if (anyE.kind === "sovereignty_blocked") {
      return $t("connections.error_sovereignty_blocked");
    }
    if (anyE.kind === "oauth_client_not_configured") {
      return $t("connections.error_oauth_client_missing");
    }
    if (anyE.kind && anyE.detail) {
      return `${anyE.kind}: ${anyE.detail}`;
    }
    if (anyE.message) return anyE.message;
    if (anyE.kind) return anyE.kind;
    return String(e);
  }

  /** Tear down any registered Tauri listeners - called when the dialog closes
   *  or before starting a new flow so we don't leak handlers across attempts. */
  async function clearOauthListeners(): Promise<void> {
    const fns = oauthDialogUnlistenFns;
    oauthDialogUnlistenFns = [];
    for (const fn of fns) {
      try {
        fn();
      } catch {
        // best-effort cleanup
      }
    }
  }

  function closeOauthDialog(): void {
    oauthDialogOpen = false;
    oauthDialogAwaitingCallback = false;
    void clearOauthListeners();
  }

  async function startNativeConnect(provider: ProviderId) {
    await clearOauthListeners();
    oauthDialogProvider = provider;
    oauthDialogAuthUrl = null;
    oauthDialogState = null;
    oauthDialogPastedCode = "";
    oauthDialogError = null;
    oauthDialogErrorIsMissingClient = false;
    oauthDialogBusy = true;
    oauthDialogAwaitingCallback = false;
    oauthDialogStep = "auth";
    oauthDialogConnectedAccountId = null;
    oauthDialogDriveFolderDraft = "";
    oauthDialogDriveFolderError = null;
    oauthDialogOpen = true;
    try {
      // Default scopes mirror connector_providers.rs defaults - let the
      // user trim later via "Gérer le compte" once we expose a scope picker.
      const defaultScopes =
        provider === "google"
          ? [
              "mail.send",
              "mail.compose",
              "calendar.read",
              "calendar.write",
              "drive.workspace",
            ]
          : [
              "mail.read",
              "mail.send",
              "calendar.read",
              "calendar.write",
              "files.read",
            ];
      const start = await invoke<{
        auth_url: string;
        state: string;
        callback_port: number;
      }>("oauth_start_flow", {
        provider,
        scopes: defaultScopes,
        sovereignty: "cloud_allowed",
      });
      oauthDialogAuthUrl = start.auth_url;
      oauthDialogState = start.state;
      oauthDialogAwaitingCallback = true;

      // Register listeners for the loopback callback BEFORE opening the
      // browser so we never race the redirect. Filter by `state` so concurrent
      // OAuth flows (unlikely, but possible) don't cross-talk.
      const expectedState = start.state;
      const codeUnlisten = await listen<{ state: string; code: string }>(
        "oauth://code-ready",
        (event) => {
          if (event.payload?.state !== expectedState) return;
          oauthDialogPastedCode = event.payload.code;
          oauthDialogAwaitingCallback = false;
          void completeNativeFlow();
        },
      );
      const errUnlisten = await listen<{ state: string; error: string }>(
        "oauth://error",
        (event) => {
          if (event.payload?.state !== expectedState) return;
          oauthDialogAwaitingCallback = false;
          oauthDialogError = event.payload.error;
        },
      );
      oauthDialogUnlistenFns = [codeUnlisten, errUnlisten];

      // The backend opens the system browser via tauri-plugin-opener (more
      // reliable than `window.open` from the Tauri webview, which is blocked
      // by sandboxing on some platforms). No-op here - the link is still
      // surfaced in the dialog as a manual fallback.
    } catch (e) {
      const formatted = formatTauriError(e);
      oauthDialogError = formatted;
      oauthDialogErrorIsMissingClient = isMissingClientError(e);
    } finally {
      oauthDialogBusy = false;
    }
  }

  function isMissingClientError(e: unknown): boolean {
    const anyE = e as { kind?: string } | null;
    return anyE?.kind === "oauth_client_not_configured";
  }

  function openSettingsIntegrations(): void {
    closeOauthDialog();
    navigateToSettings("integrations");
  }

  async function completeNativeFlow() {
    if (!oauthDialogState || !oauthDialogPastedCode) return;
    oauthDialogBusy = true;
    oauthDialogError = null;
    try {
      const result = await invoke<{
        provider: string;
        account_id: string;
        granted_scopes: string[];
      }>("oauth_complete_flow", {
        state: oauthDialogState,
        code: oauthDialogPastedCode.trim(),
      });
      await refreshNativeStatus();
      if (result.provider === "google") {
        // Google-specific second step: ask the user where in their Drive
        // they want agents to operate. Skipping this leaves the default
        // "Apollia" folder, which is fine for most users.
        oauthDialogConnectedAccountId = result.account_id;
        oauthDialogDriveFolderDraft = "Apollia";
        oauthDialogStep = "drive_folder";
      } else {
        closeOauthDialog();
      }
    } catch (e) {
      oauthDialogError = formatTauriError(e);
    } finally {
      oauthDialogBusy = false;
    }
  }

  /** Save the Drive folder the user picked in step 2 of the Google wizard.
   *  Always proceed past this step on success; an empty value clears any
   *  override and the agent falls back to the default `Apollia` folder. */
  async function saveDriveFolderAndFinish(): Promise<void> {
    if (!oauthDialogConnectedAccountId) {
      closeOauthDialog();
      return;
    }
    oauthDialogDriveFolderSaving = true;
    oauthDialogDriveFolderError = null;
    try {
      await invoke("oauth_set_drive_folder", {
        accountId: oauthDialogConnectedAccountId,
        folderPath: oauthDialogDriveFolderDraft.trim(),
      });
      closeOauthDialog();
    } catch (e) {
      oauthDialogDriveFolderError = formatTauriError(e);
    } finally {
      oauthDialogDriveFolderSaving = false;
    }
  }

  function skipDriveFolderStep(): void {
    // User declined to customise → keep the legacy default (no override
    // written to disk). Close the dialog cleanly.
    closeOauthDialog();
  }

  async function disconnectNative(account: OauthAccountInfo) {
    if (
      !confirm(
        $t("connections.disconnect_native_confirm", {
          values: { account: account.account_id, provider: account.provider },
        }),
      )
    ) {
      return;
    }
    try {
      await invoke("oauth_disconnect", {
        provider: account.provider,
        accountId: account.account_id,
      });
      await refreshNativeStatus();
    } catch (e) {
      nativeError = formatTauriError(e);
    }
  }

  // ── Loaders ────────────────────────────────────────────────────────────────

  /** Phase 1: load servers + enrichments immediately (fast, no network call).
   *  Phase 2 happens lazily - see `loadCuratedCatalogue` and `loadFullRegistry`.
   *  The previous version always blocked on `fetch_mcp_registry` (full network
   *  fetch of ~6000 entries, 30–60s on cold cache). That blocked the
   *  Catalogue Sheet open with no visible content - fixed by gating the
   *  heavy fetch behind a user opt-in. */
  async function loadAll(): Promise<void> {
    loading = true;
    loadError = null;
    try {
      const [serverList, enrichmentEntries, agentList] = await Promise.all([
        invoke<McpServerStatusView[]>("list_mcp_servers"),
        invoke<Array<{ package_identifier: string; enrichment: ConnectorEnrichmentView }>>(
          "list_mcp_enrichments",
        ),
        invoke<AgentListItem[]>("list_agents").catch(() => [] as AgentListItem[]),
      ]);
      servers = serverList;
      enrichmentMap = new Map(
        enrichmentEntries.map((e) => [e.package_identifier, e.enrichment]),
      );
      agents = agentList;
    } catch (err: unknown) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  // Catalogue load tiers - curated (~18, instant) vs full registry (~6k, slow).
  type CatalogueTier = "none" | "curated" | "full";
  let catalogueTier = $state<CatalogueTier>("none");

  /** Load the 18 curated enrichments instantly (no network). Called every
   *  time the Catalogue Sheet opens so the user sees content < 100ms even
   *  on a cold cache. */
  async function loadCuratedCatalogue(): Promise<void> {
    if (catalogueTier !== "none") return;
    registryLoading = true;
    try {
      registry = await invoke<RegistryServerView[]>("fetch_mcp_curated").catch(
        () => [] as RegistryServerView[],
      );
      catalogueTier = "curated";
      catalogLimit = CATALOG_PAGE;
    } finally {
      registryLoading = false;
    }
  }

  /** Explicit opt-in : fetch the full public MCP registry. May take 30–60s on
   *  a cold cache (the registry paginates through ~60 pages of 100 entries).
   *  Subsequent calls within 15 minutes hit the disk cache and return in
   *  ~100ms. */
  async function loadFullRegistry(): Promise<void> {
    if (catalogueTier === "full" || registryLoading) return;
    registryLoading = true;
    try {
      registry = await invoke<RegistryServerView[]>("fetch_mcp_registry").catch(
        () => [] as RegistryServerView[],
      );
      catalogueTier = "full";
      catalogLimit = CATALOG_PAGE;
    } finally {
      registryLoading = false;
    }
  }

  // ── Helpers ────────────────────────────────────────────────────────────────

  function resolveEnrichment(server: McpServerStatusView): ConnectorEnrichmentView | null {
    if (!server.package) return null;
    return enrichmentMap.get(server.package) ?? null;
  }

  function statusOf(server: McpServerStatusView): ConnectionStatus {
    switch (server.health.state) {
      case "healthy":
        return "active";
      case "degraded":
      case "needs_reauth":
        return "attention";
      case "unavailable":
        return "error";
    }
  }

  function syncLabel(server: McpServerStatusView): string | undefined {
    if (!server.last_call_at) return undefined;
    const diffMs = Date.now() - new Date(server.last_call_at).getTime();
    if (Number.isNaN(diffMs) || diffMs < 0) return undefined;
    const mins = Math.floor(diffMs / 60_000);
    if (mins < 1) return $t("connections.sync_just_now");
    if (mins < 60) return $t("connections.sync_minutes_ago", { values: { mins } });
    const hours = Math.floor(mins / 60);
    if (hours < 24) return $t("connections.sync_hours_ago", { values: { hours } });
    const days = Math.floor(hours / 24);
    return $t("connections.sync_days_ago", { values: { days } });
  }

  /** Stable color for the logo tile, derived from the server name. */
  function logoColorFor(name: string): string {
    const palette = [
      "#4285f4",
      "#611f69",
      "#1c1d2b",
      "#ea4335",
      "#0ea5e9",
      "#a855f7",
      "#16a34a",
      "#f59e0b",
    ];
    let hash = 0;
    for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) | 0;
    return palette[Math.abs(hash) % palette.length];
  }

  // ── Derived sets ───────────────────────────────────────────────────────────

  const installedNames = $derived(new Set(servers.map((s) => s.name)));

  /**
   * Registry entries are keyed by the package identifier (e.g.
   * `@modelcontextprotocol/server-filesystem`) but the installed-server
   * `name` is the sanitized operator label (e.g. `local-files`). To bridge
   * the two, we index the installed servers by their `package` field so we
   * can map a registry entry → its actual installed server name.
   */
  const installedByPackage = $derived.by(() => {
    const map = new Map<string, string>();
    for (const s of servers) {
      if (s.package) map.set(s.package, s.name);
    }
    return map;
  });

  /** Resolve the installed server name (if any) for a registry catalogue entry. */
  function installedNameFor(entry: RegistryServerView): string | null {
    if (installedNames.has(entry.name)) return entry.name;
    const byPkg = installedByPackage.get(entry.name);
    if (byPkg) return byPkg;
    const pkgId = entry.packages?.[0]?.identifier;
    if (pkgId) {
      const match = installedByPackage.get(pkgId);
      if (match) return match;
    }
    return null;
  }

  const activeCount = $derived(servers.filter((s) => statusOf(s) === "active").length);
  const attentionCount = $derived(
    servers.filter((s) => statusOf(s) === "attention").length,
  );
  const errorCount = $derived(servers.filter((s) => statusOf(s) === "error").length);
  const idleCount = $derived(servers.filter((s) => statusOf(s) === "idle").length);

  const filteredServers = $derived(
    servers.filter((s) => {
      const st = statusOf(s);
      if (statusFilter === "all") return true;
      return st === statusFilter;
    }),
  );

  /** MCP catalogue - combine ranked suggestions + remaining registry, dedup. */
  const mcpEntries = $derived.by(() => {
    const ranked = rankSuggestions(registry, agents, 12);
    const seen = new Set(ranked.map((r) => r.name));
    const rest = registry.filter((r) => !seen.has(r.name));
    return [...ranked, ...rest];
  });

  const officialCount = $derived(
    mcpEntries.filter(
      (e) =>
        e.trust_level === "verified_official" || e.trust_level === "community_verified",
    ).length,
  );
  const communityCount = $derived(
    mcpEntries.filter((e) => e.trust_level === "community" || e.trust_level === "custom")
      .length,
  );
  const installedRegCount = $derived(
    mcpEntries.filter((e) => e.is_installed || installedNameFor(e) !== null).length,
  );

  const filteredMcp = $derived(
    mcpEntries.filter((e) => {
      const installed = e.is_installed || installedNameFor(e) !== null;
      if (mcpFilter === "installed") return installed;
      if (mcpFilter === "official")
        return (
          e.trust_level === "verified_official" || e.trust_level === "community_verified"
        );
      if (mcpFilter === "community")
        return e.trust_level === "community" || e.trust_level === "custom";
      return true;
    }),
  );

  // Visible slice - reset catalogLimit when filter changes to avoid showing 0 results.
  $effect(() => {
    void mcpFilter;
    catalogLimit = CATALOG_PAGE;
  });

  const visibleMcp = $derived(filteredMcp.slice(0, catalogLimit));
  const hasMoreMcp = $derived(filteredMcp.length > catalogLimit);

  // ── CTAs & flows (preserved from old Connections.svelte) ───────────────────

  function openWizardFor(server: RegistryServerView) {
    selectedRegistryServer = server;
    wizardOpen = true;
  }

  function handleConnect(server: RegistryServerView) {
    // Close the Catalogue Sheet before opening the wizard - otherwise the
    // Sheet stays mounted under the wizard, leaking through the backdrop
    // (regression observed: wizard z-index doesn't escape the parent Sheet
    // portal stacking context).
    catalogueOpen = false;
    if (!isDisclaimerAccepted()) {
      selectedRegistryServer = server;
      disclaimerOpen = true;
      return;
    }
    openWizardFor(server);
  }

  function handleDisclaimerAccept() {
    disclaimerOpen = false;
    if (selectedRegistryServer) openWizardFor(selectedRegistryServer);
  }

  function handleWizardClose() {
    wizardOpen = false;
    selectedRegistryServer = null;
  }

  function handleWizardComplete() {
    wizardOpen = false;
    selectedRegistryServer = null;
    catalogueOpen = false;
    loadAll();
  }

  function handleManage(name: string) {
    selection = { kind: "mcp", name };
  }

  // ── Uninstall flow (Trash icon on installed cards) ─────────────────────────

  /**
   * Uninstall confirmation state. We surface a [`ConfirmDialog`] before the
   * destructive `remove_mcp_server` call - matching the pattern used by
   * Projects.svelte. The `target` carries the installed server name (the
   * sanitized identifier stored on disk, NOT the registry / package
   * identifier) so legacy entries like `modelcontextprotocol-server-filesystem`
   * remain addressable.
   */
  // Disconnect/uninstall is now handled inline inside the MCP detail's
  // "Paramètres" tab (`handleDisconnect`). The old uninstall ConfirmDialog
  // + uninstallTarget state were removed with the 3-section flat layout.

  // Sorted MCP servers for the sidebar (error → active → syncing → idle, by name).
  const sortedServers = $derived.by(() => {
    const score: Record<ConnectionStatus, number> = {
      error: 0,
      attention: 1,
      active: 2,
      syncing: 3,
      idle: 4,
    };
    return [...filteredServers].sort((a, b) => {
      const sa = score[statusOf(a)] ?? 99;
      const sb = score[statusOf(b)] ?? 99;
      if (sa !== sb) return sa - sb;
      return a.name.localeCompare(b.name);
    });
  });

  const sidebarFilteredServers = $derived.by(() => {
    const q = sidebarFilter.trim().toLowerCase();
    if (q === "") return sortedServers;
    return sortedServers.filter((s) => {
      const enr = resolveEnrichment(s);
      const label = enr?.operator_label ?? s.name;
      return (
        label.toLowerCase().includes(q) ||
        s.name.toLowerCase().includes(q)
      );
    });
  });

  // Auto-select the first available connector when the page mounts or the
  // selection target disappears (mirror Projets/Agents pattern).
  $effect(() => {
    if (loading) return;
    const current = selection;
    if (current?.kind === "mcp" && !servers.some((s) => s.name === current.name)) {
      selection = null;
    }
    if (selection !== null) return;
    if (servers.length > 0) {
      selection = { kind: "mcp", name: sortedServers[0]?.name ?? servers[0].name };
    } else {
      selection = { kind: "native", provider: "google" };
    }
  });

  const selectedServer = $derived.by(() => {
    const s = selection;
    if (s?.kind !== "mcp") return null;
    return servers.find((srv) => srv.name === s.name) ?? null;
  });
  const selectedServerEnrichment = $derived(
    selectedServer ? resolveEnrichment(selectedServer) : null,
  );
  const selectedNativeConnector = $derived.by(() => {
    const s = selection;
    if (s?.kind !== "native") return null;
    return NATIVE_CONNECTORS.find((c) => c.id === s.provider) ?? null;
  });
  const selectedNativeAccounts = $derived.by(() => {
    const s = selection;
    if (s?.kind !== "native") return [];
    return accountsForProvider(s.provider);
  });

  async function handleRetry(name: string) {
    errorModalOpen = false;
    try {
      await invoke("restart_mcp_server", { name });
    } catch {
      /* surfaced via reload */
    }
    loadAll();
  }

  function handleViewLogs(name: string) {
    errorModalOpen = false;
    handleManage(name);
  }

  function handleOpenCatalogue() {
    catalogueTab = "discover";
    catalogueOpen = true;
    // Curated (~18 entries, instant, no network) fills the visible area
    // immediately. The full registry (~6k entries) loads in background
    // straight after - appears as it arrives, no spinner blocking, no
    // explicit user click required. Subsequent opens hit the 15-min disk
    // cache and are near-instant.
    void (async () => {
      await loadCuratedCatalogue();
      if (catalogueTier === "curated") {
        void loadFullRegistry();
      }
    })();
  }

  /** Derive a publisher/vendor label from a registry entry.
   *
   *  Sources in order of preference :
   *    1. `repository.url` host org segment (most reliable for community)
   *    2. `packages[0].identifier` namespace (`@notionhq/...` → "notionhq",
   *       `com.notion/mcp` reverse-DNS → "notion")
   *    3. Domain of the curated `remote_url` if no packages
   *  Returns an empty string when nothing usable is found. */
  function vendorLabel(entry: RegistryServerView): string {
    // Repository URL: github.com/notion-hq/notion-mcp-server → "notion-hq"
    const repoUrl = entry.repository_url;
    if (repoUrl) {
      try {
        const u = new URL(repoUrl);
        const segments = u.pathname.split("/").filter(Boolean);
        if (segments.length > 0) return prettyVendor(segments[0]);
      } catch {
        /* fall through */
      }
    }

    // Package identifier
    const pkgId = entry.packages?.[0]?.identifier ?? entry.name;
    if (pkgId.startsWith("@")) {
      // npm scoped: @org/package
      const slash = pkgId.indexOf("/");
      if (slash > 1) return prettyVendor(pkgId.slice(1, slash));
    }
    // Reverse-DNS style: com.notion/mcp or io.foo/bar → take second segment
    const dot = pkgId.indexOf(".");
    const slash = pkgId.indexOf("/");
    if (dot > 0 && (slash === -1 || dot < slash)) {
      const after = pkgId.slice(dot + 1);
      const end = after.indexOf(".") >= 0
        ? after.indexOf(".")
        : after.indexOf("/") >= 0
          ? after.indexOf("/")
          : after.length;
      if (end > 0) return prettyVendor(after.slice(0, end));
    }
    // Remote URL host (e.g. mcp.notion.com → "notion")
    const remoteUrl = entry.remotes?.[0]?.url;
    if (remoteUrl) {
      try {
        const u = new URL(remoteUrl);
        const parts = u.hostname.split(".").filter(Boolean);
        if (parts.length >= 2) return prettyVendor(parts[parts.length - 2]);
      } catch {
        /* fall through */
      }
    }
    return "";
  }

  function prettyVendor(raw: string): string {
    const clean = raw.replace(/-mcp$|^mcp-|server[-_]?/gi, "").replace(/[-_]+/g, " ").trim();
    if (clean.length === 0) return raw;
    return clean.charAt(0).toUpperCase() + clean.slice(1);
  }

  // IntersectionObserver-based auto-pagination - Amazon-style infinite
  // scroll. Replaces the "Voir plus" button. The sentinel is mounted at
  // the bottom of the list; when it scrolls into view we bump
  // `catalogLimit` by a page (24).
  let catalogueSentinel = $state<HTMLDivElement | null>(null);
  $effect(() => {
    const el = catalogueSentinel;
    if (!el || !catalogueOpen || catalogueTab !== "discover") return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          if (catalogLimit < filteredMcp.length) {
            catalogLimit = Math.min(catalogLimit + CATALOG_PAGE, filteredMcp.length);
          }
        }
      },
      { rootMargin: "200px 0px" }, // pre-trigger 200px before reaching bottom
    );
    observer.observe(el);
    return () => observer.disconnect();
  });

  // Custom MCP server form state.
  let customForm = $state({
    name: "",
    transport: "stdio" as "stdio" | "streamable-http" | "sse",
    command: "",
    args: "",
    url: "",
    headers: "" as string, // free-form: "Header-Name=value" per line
    requires_approval: true,
  });
  let customBusy = $state(false);
  let customError = $state<string | null>(null);
  let customTestResult = $state<string | null>(null);

  function resetCustomForm() {
    customForm = {
      name: "",
      transport: "stdio",
      command: "",
      args: "",
      url: "",
      headers: "",
      requires_approval: true,
    };
    customError = null;
    customTestResult = null;
  }

  function sanitizeServerName(raw: string): string {
    const cleaned = raw
      .toLowerCase()
      .replace(/[^a-z0-9_-]+/g, "-")
      .replace(/-+/g, "-")
      .replace(/^-|-$/g, "");
    return cleaned.length > 0 ? cleaned : "custom-mcp";
  }

  function buildCustomConfig(): Record<string, unknown> | null {
    const env: Record<string, string> = {};
    for (const line of customForm.headers.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) continue;
      const eq = trimmed.indexOf("=");
      if (eq <= 0) continue;
      env[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
    }
    const base: Record<string, unknown> = {
      name: sanitizeServerName(customForm.name || "custom-mcp"),
      env,
      transport: customForm.transport,
      requires_approval: customForm.requires_approval,
      tags: [],
    };
    if (customForm.transport === "stdio") {
      if (!customForm.command.trim()) {
        customError = $t("connections.custom_error_command_required");
        return null;
      }
      base.command = customForm.command.trim();
      base.args = customForm.args
        .split(/\s+/)
        .map((a) => a.trim())
        .filter((a) => a.length > 0);
    } else {
      if (!customForm.url.trim()) {
        customError = $t("connections.custom_error_url_required");
        return null;
      }
      base.url = customForm.url.trim();
    }
    return base;
  }

  async function testCustomServer() {
    customError = null;
    customTestResult = null;
    const config = buildCustomConfig();
    if (!config) return;
    customBusy = true;
    try {
      const response = await invoke<McpConnectionTestResponse>(
        "test_mcp_connection",
        { config },
      );
      if (response.kind === "success") {
        customTestResult = $t("connections.custom_test_tools_detected", {
          values: { count: response.tools.length },
        });
      } else {
        // oauth_required → custom-form path. The user is supposed to pre-fill
        // env headers themselves here; we surface OAuth as an error pointing
        // them at the catalog wizard which will gain the proper button in
        // Phase 5.
        customError = $t("connections.custom_error_oauth_required");
      }
    } catch (e) {
      customError = formatTauriError(e);
    } finally {
      customBusy = false;
    }
  }

  async function installCustomServer() {
    customError = null;
    const config = buildCustomConfig();
    if (!config) return;
    customBusy = true;
    try {
      await invoke("add_mcp_server", { config });
      catalogueOpen = false;
      resetCustomForm();
      await loadAll();
    } catch (e) {
      customError = formatTauriError(e);
    } finally {
      customBusy = false;
    }
  }

  $effect(() => {
    loadAll();
    void refreshNativeStatus();
  });

  // Live health: reflect McpServerHealthChanged on the badge without a manual
  // refresh, so a server that degrades mid-session (e.g. a Notion 404 from an
  // agent tool call) turns amber on the spot.
  $effect(() => {
    type Envelope = {
      category: string;
      event_type: string;
      payload: Record<string, unknown>;
    };
    let unlisten: UnlistenFn | null = null;
    let disposed = false;
    void listen<Envelope>("runtime-event", (event) => {
      if (event.payload.event_type !== "McpServerHealthChanged") return;
      const inner = event.payload.payload?.McpServerHealthChanged as
        | { name: string; health: McpHealth }
        | undefined;
      if (!inner) return;
      servers = servers.map((s) =>
        s.name === inner.name ? { ...s, health: inner.health } : s,
      );
      if (
        mcpDetail &&
        selection?.kind === "mcp" &&
        selection.name === inner.name
      ) {
        mcpDetail = {
          ...mcpDetail,
          status: { ...mcpDetail.status, health: inner.health },
        };
      }
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  // Avoid unused-binding warning from prop.
  $effect(() => {
    void onNavigateTasks;
  });
</script>

<div class="flex h-full min-h-0 w-full flex-col" data-testid="connections-route">
  <SplitLayout sidebarTestid="connectors-sidebar" detailTestid="connector-detail">
    {#snippet sidebar()}
      <header class="px-4 pt-4 pb-2.5">
        <div class="mb-2.5 font-mono text-[10.5px] font-semibold tracking-[1.2px] uppercase text-muted-foreground">
          {$t("connections.sidebar_my_connectors", { values: { count: servers.length + NATIVE_CONNECTORS.length } })}
        </div>
        <div class="mb-2 flex items-center gap-2 px-2 py-1.5 rounded-md bg-surface-1 border border-border">
          <Search size={11} class="text-muted-foreground" />
          <Input
            type="search"
            unstyled
            bind:value={sidebarFilter}
            placeholder={$t("connections.sidebar_filter_placeholder")}
            class="flex-1 text-[11.5px] text-foreground"
          />
        </div>
        <FilterChipBar
          size="compact"
          chips={[
            { key: "all", label: $t("connections.status_chip_all"), color: "hsl(var(--muted-foreground))", count: servers.length },
            { key: "active", label: $t("connections.status_chip_active"), color: "hsl(var(--success))", count: activeCount, glowWhenActive: true },
            { key: "attention", label: $t("connections.status_chip_attention"), color: "hsl(var(--warning))", count: attentionCount },
            { key: "error", label: $t("connections.status_chip_error"), color: "hsl(var(--destructive))", count: errorCount },
            { key: "idle", label: $t("connections.status_chip_idle"), color: "hsl(var(--muted-foreground))", count: idleCount },
          ]}
          activeKey={statusFilter}
          onchange={(key) => (statusFilter = key as StatusFilter)}
          aria-label={$t("connections.status_filter_label")}
          testidPrefix="connections-sidebar-filter"
        />
      </header>

      <div class="flex-1 overflow-auto px-2.5 pb-3" data-testid="connections-sidebar-list">
        {#if loadError}
          <div class="mx-1.5 my-2 rounded-md border border-destructive/40 bg-destructive/5 px-2 py-1.5 text-[11px] text-destructive" data-testid="connections-error">
            {loadError}
          </div>
        {/if}

        <!-- ─── Native Apollia (always shown) ───────────────────────────── -->
        <div class="section-meta mt-1 mb-1.5 px-2 text-[10px] tracking-[1.4px]">
          {$t("connections.sidebar_native_section")}
        </div>
        {#each NATIVE_CONNECTORS as connector (connector.id)}
          {@const accounts = accountsForProvider(connector.id)}
          {@const isActiveSel = selection?.kind === "native" && selection.provider === connector.id}
          {@const accentColor = accounts.length > 0 ? "hsl(var(--success))" : "hsl(var(--muted-foreground))"}
          <Button variant="ghost" size="auto"
            type="button"
            onclick={() => (selection = { kind: "native", provider: connector.id })}
            class="w-full text-left flex items-start gap-2.5 px-2.5 py-2.5 rounded-lg mb-0.5 border-0 transition-colors {isActiveSel
              ? 'bg-primary/10'
              : 'bg-transparent hover:bg-muted/40'}"
            data-testid="sidebar-native-{connector.id}"
          >
            <div
              class="w-0.5 self-stretch rounded-sm shrink-0 my-0.5"
              style="background: {accentColor};"
            ></div>
            <div class="flex-1 min-w-0">
              <div class="text-[12.5px] truncate text-foreground" style:font-weight={isActiveSel ? 600 : 500}>
                {connector.name}
              </div>
              <div class="text-[10.5px] text-muted-foreground mt-0.5 truncate">
                {accounts.length > 0
                  ? $t("connections.sidebar_accounts_active", { values: { count: accounts.length } })
                  : $t("connections.not_connected")}
              </div>
            </div>
          </Button>
        {/each}

        <!-- ─── MCP servers installed ──────────────────────────────────── -->
        <div class="section-meta mt-4 mb-1.5 px-2 text-[10px] tracking-[1.4px]">
          {$t("connections.sidebar_mcp_section", { values: { count: sortedServers.length } })}
        </div>
        {#if loading && servers.length === 0}
          {#each Array(4) as _, i (i)}
            <div class="flex items-center gap-2.5 px-2.5 py-2.5">
              <Skeleton class="h-8 w-0.5 rounded-sm" />
              <div class="flex-1 space-y-1.5">
                <Skeleton class="h-3 w-3/5 rounded" />
                <Skeleton class="h-2 w-2/5 rounded" />
              </div>
            </div>
          {/each}
        {:else if sidebarFilteredServers.length === 0}
          <p class="px-2 py-3 text-[11px] text-muted-foreground/70">
            {servers.length === 0
              ? $t("connections.sidebar_no_servers")
              : sidebarFilter ? $t("connections.sidebar_no_results") : $t("connections.sidebar_no_servers_status")}
          </p>
        {:else}
          {#each sidebarFilteredServers as server (server.name)}
            {@const enr = resolveEnrichment(server)}
            {@const label = enr?.operator_label ?? server.name}
            {@const st = statusOf(server)}
            {@const isActiveSel = selection?.kind === "mcp" && selection.name === server.name}
            {@const accentColor = st === "active"
              ? "hsl(var(--success))"
              : st === "attention"
                ? "hsl(var(--warning))"
                : st === "error"
                  ? "hsl(var(--destructive))"
                  : "hsl(var(--muted-foreground))"}
            <Button variant="ghost" size="auto"
              type="button"
              onclick={() => (selection = { kind: "mcp", name: server.name })}
              class="w-full text-left flex items-start gap-2.5 px-2.5 py-2.5 rounded-lg mb-0.5 border-0 transition-colors {isActiveSel
                ? 'bg-primary/10'
                : 'bg-transparent hover:bg-muted/40'}"
              data-testid="sidebar-mcp-{server.name}"
            >
              <div
                class="w-0.5 self-stretch rounded-sm shrink-0 my-0.5"
                style="background: {accentColor};"
              ></div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5 min-w-0">
                  <span class="text-[12.5px] truncate text-foreground" style:font-weight={isActiveSel ? 600 : 500}>{label}</span>
                  {#if st === 'active'}
                    <StatusDot color="hsl(var(--success))" glow size={5} />
                  {:else if st === 'attention'}
                    <StatusDot color="hsl(var(--warning))" glow size={5} />
                  {:else if st === 'error'}
                    <StatusDot color="hsl(var(--destructive))" glow size={5} />
                  {/if}
                </div>
                <div class="text-[10.5px] text-muted-foreground mt-0.5 truncate">
                  {syncLabel(server) ?? (st === 'error' ? $t("connections.status_chip_error") : st === 'attention' ? $t("connections.status_chip_attention") : st === 'active' ? $t("connections.tools_count_label", { values: { count: server.tools_count } }) : $t("connections.status_chip_idle"))}
                </div>
              </div>
            </Button>
          {/each}
        {/if}
      </div>

      <div class="px-3 py-2 border-t border-border">
        <Button variant="outline" size="sm" onclick={handleOpenCatalogue} class="w-full justify-center" data-testid="connections-open-catalogue">
          {#snippet icon()}<Plus size={12} />{/snippet}
          {$t("connections.add_connector")}
        </Button>
      </div>
    {/snippet}

    <!-- ============ RIGHT - detail pane ============ -->
      {#if selection?.kind === "native" && selectedNativeConnector}
        {@const connector = selectedNativeConnector}
        {@const accounts = selectedNativeAccounts}

        <!-- Header -->
        <div class="px-8 pt-6 pb-4 border-b border-border/60">
          <div class="flex items-start gap-3.5">
            <div
              class="w-12 h-12 shrink-0 rounded-lg inline-flex items-center justify-center"
              style="background: {connector.id === 'google'
                ? 'linear-gradient(135deg, #4285f4, #1a73e8)'
                : 'linear-gradient(135deg, #0078d4, #005a9e)'}; color: white;"
            >
              <LinkIcon size={18} />
            </div>
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <Badge variant={accounts.length > 0 ? "success" : "neutral"} size="sm" outline={accounts.length === 0}>
                  {#snippet icon()}
                    <StatusDot color={accounts.length > 0 ? "hsl(var(--success))" : "hsl(var(--muted-foreground))"} glow={accounts.length > 0} />
                  {/snippet}
                  {accounts.length > 0 ? $t("connections.badge_active_count", { values: { count: accounts.length } }) : $t("connections.not_connected")}
                </Badge>
                <Badge variant="primary" size="sm">{$t("connections.native_section_tag")}</Badge>
              </div>
              <h2 class="m-0 text-foreground" style="font-size: 22px; font-weight: 600; letter-spacing: -0.3px; line-height: 1.2;">
                {connector.name}
              </h2>
              <p class="mt-1 max-w-[600px] text-[12.5px] leading-[1.5] text-muted-foreground">
                {connector.description}
              </p>
            </div>
            <div class="flex shrink-0 gap-1.5">
              <Button variant="primary-solid" size="sm" onclick={() => startNativeConnect(connector.id)} disabled={nativeLoading}>
                {#snippet icon()}<Plus size={12} />{/snippet}
                {accounts.length > 0 ? $t("connections.add_account") : $t("connections.connect")}
              </Button>
            </div>
          </div>
        </div>

        <!-- Tabs -->
        <div class="px-8 pt-3.5">
          <TabBar
            variant="underline"
            testidPrefix="native-detail"
            activeTab={nativeTab}
            items={[
              { key: "accounts", label: $t("connections.native_tab_accounts"), count: accounts.length },
              { key: "capabilities", label: $t("connections.native_tab_capabilities") },
              { key: "settings", label: $t("connections.native_tab_settings") },
            ]}
            ontabchange={(k) => (nativeTab = k as NativeTab)}
          />
        </div>

        <!-- Tab content -->
        <div class="flex-1 overflow-auto px-8 py-5">
          {#if nativeTab === "accounts"}
            {#if nativeError}
              <div class="mb-3 rounded-md border border-destructive/40 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {nativeError}
              </div>
            {/if}
            {#if accounts.length === 0}
              <Card class="p-6 max-w-2xl text-center">
                <p class="text-[12.5px] text-muted-foreground mb-3">
                  {$t("connections.no_account_for", { values: { name: connector.name } })}
                </p>
                <Button variant="primary-solid" size="sm" onclick={() => startNativeConnect(connector.id)} disabled={nativeLoading}>
                  {#snippet icon()}<Plus size={12} />{/snippet}
                  {$t("connections.connect_account")}
                </Button>
              </Card>
            {:else}
              <Card class="overflow-hidden max-w-2xl">
                {#each accounts as account, i (account.account_id)}
                  <div class="flex items-center gap-3 px-4 py-3 {i === accounts.length - 1 ? '' : 'border-b border-border/40'}">
                    <div class="flex-1 min-w-0">
                      <div class="text-[12.5px] font-medium text-foreground truncate">{account.account_id}</div>
                      <div class="text-[10.5px] text-muted-foreground mt-0.5">{$t("connections.token_encrypted_keyring")}</div>
                    </div>
                    <Button variant="ghost" size="sm"
                      onclick={() => disconnectNative(account)}
                      disabled={nativeLoading}
                      class="text-destructive hover:bg-destructive/10"
                    >
                      {$t("connections.disconnect")}
                    </Button>
                  </div>
                {/each}
              </Card>
            {/if}
          {:else if nativeTab === "capabilities"}
            <div class="space-y-4 max-w-3xl" data-testid="native-capabilities">
              <!-- Frame the user-facing reality up front: v0.1.0 Apollia uses
                   only free-tier OAuth scopes (no CASA audit), so some Gmail
                   and Drive verbs are intentionally out of reach. Surface this
                   clearly so the operator does not waste time asking the
                   agent to read inbox / list whole Drive - neither will work. -->
              <Card class="p-[14px_16px] border-primary/30 bg-primary/5">
                <div class="text-[10px] font-medium uppercase tracking-wider text-primary/80 mb-2">
                  {$t("connections.capabilities.scope_policy_title")}
                </div>
                <p class="text-[12.5px] text-foreground/85 leading-[1.5]">
                  {$t(`connections.capabilities.scope_policy_body_${connector.id}`)}
                </p>
              </Card>

              {#each capabilityGroupsFor(connector.id) as group (group.key)}
                <Card class="p-[14px_16px]">
                  <div class="text-[11px] font-semibold text-foreground mb-2 flex items-center gap-2">
                    <span>{$t(`connections.capabilities.group_${group.key}`)}</span>
                  </div>
                  <ul class="space-y-1.5">
                    {#each group.entries as entry}
                      <li class="flex items-start gap-2.5 text-[12.5px]">
                        <span class="mt-[3px] inline-flex h-3.5 w-3.5 items-center justify-center rounded-full {entry.supported ? 'bg-success/20 text-success' : 'bg-destructive/15 text-destructive'} shrink-0">
                          {entry.supported ? "✓" : "✕"}
                        </span>
                        <div class="flex-1">
                          <div class="text-foreground/90">
                            {$t(entry.labelKey)}
                          </div>
                          {#if entry.detailKey}
                            <div class="text-[11px] text-muted-foreground mt-0.5">
                              {$t(entry.detailKey)}
                            </div>
                          {/if}
                        </div>
                      </li>
                    {/each}
                  </ul>
                </Card>
              {/each}

              <Card class="p-[14px_16px] bg-muted/30">
                <p class="text-[11.5px] text-muted-foreground leading-[1.55]">
                  {$t("connections.capabilities.expert_mode_hint")}
                </p>
              </Card>
            </div>
          {:else if nativeTab === "settings"}
            <div class="space-y-4 max-w-2xl">
              <Card class="p-[14px_16px]">
                <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
                  {$t("connections.settings_permissions_title")}
                </div>
                <ul class="space-y-1 text-[12px] text-foreground/85">
                  {#each (connector.id === "google"
                    ? [$t("connections.settings_scope_gmail"), $t("connections.settings_scope_gcal"), $t("connections.settings_scope_gdrive")]
                    : [$t("connections.settings_scope_outlook_mail"), $t("connections.settings_scope_outlook_cal"), $t("connections.settings_scope_onedrive")]) as scope}
                    <li class="flex items-start gap-2">
                      <span class="mt-1.5 w-1 h-1 rounded-full bg-muted-foreground/60 shrink-0"></span>
                      <span>{scope}</span>
                    </li>
                  {/each}
                </ul>
              </Card>
              <Card class="p-[14px_16px]">
                <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
                  {$t("connections.settings_provider_title")}
                </div>
                <p class="text-[12px] text-foreground/85">
                  {connector.id === "google" ? "OAuth 2.0 - accounts.google.com" : "OAuth 2.0 - login.microsoftonline.com"}
                </p>
                <p class="text-[11px] text-muted-foreground mt-1.5">
                  {$t("connections.settings_token_rotation")}
                </p>
              </Card>
            </div>
          {/if}
        </div>
      {:else if selection?.kind === "mcp"}
        {#if mcpDetailLoading && !mcpDetail}
          <div class="flex-1 flex items-center justify-center text-[12.5px] text-muted-foreground">
            <Spinner size={14} class="mr-2" /> {$t("connections.loading")}
          </div>
        {:else if mcpDetailError}
          <div class="flex-1 flex items-center justify-center text-[12.5px] text-destructive px-8">
            {mcpDetailError}
          </div>
        {:else if mcpDetail && selectedServer}
          {@const server = selectedServer}
          {@const enr = selectedServerEnrichment}
          {@const label = enr?.operator_label ?? server.name}
          {@const st = statusOf(server)}

          <!-- Header -->
          <div class="px-8 pt-6 pb-4 border-b border-border/60">
            <div class="flex items-start gap-3.5">
              <div
                class="w-12 h-12 shrink-0 rounded-lg inline-flex items-center justify-center"
                style="background: {logoColorFor(label)}; color: white;"
              >
                <LinkIcon size={18} />
              </div>
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 mb-1">
                  <Badge variant={st === "active" ? "success" : st === "attention" ? "warning" : st === "error" ? "danger" : "neutral"} size="sm" outline={st === "idle"}>
                    {#snippet icon()}
                      <StatusDot
                        color={st === "active" ? "hsl(var(--success))" : st === "attention" ? "hsl(var(--warning))" : st === "error" ? "hsl(var(--destructive))" : "hsl(var(--muted-foreground))"}
                        glow={st === "active" || st === "attention"}
                      />
                    {/snippet}
                    {st === "active" ? $t("connections.badge_status_active") : st === "attention" ? $t("connections.badge_status_attention") : st === "error" ? $t("connections.badge_status_error") : $t("connections.badge_status_idle")}
                  </Badge>
                  {#if syncLabel(server)}
                    <span class="text-[10.5px] text-muted-foreground">{syncLabel(server)}</span>
                  {/if}
                </div>
                <h2 class="m-0 text-foreground" style="font-size: 22px; font-weight: 600; letter-spacing: -0.3px; line-height: 1.2;" data-testid="mcp-detail-title">
                  {label}
                </h2>
                <p class="mt-1 max-w-[600px] text-[12.5px] leading-[1.5] text-muted-foreground">
                  {enr?.category ?? server.server_info ?? $t("connections.mcp_installed_fallback")}
                </p>
              </div>
              <div class="flex shrink-0 gap-1.5">
                <Button variant="outline" size="sm" onclick={handleTest} disabled={testState === "testing"} data-testid="mcp-test-btn">
                  {#snippet icon()}
                    {#if testState === "testing"}<Spinner size={12} />{:else}<RefreshCw size={12} />{/if}
                  {/snippet}
                  {testState === "testing" ? $t("connections.testing") : $t("connections.test")}
                </Button>
                <Button variant="primary-solid" size="sm" onclick={handleReconnect} disabled={reconnecting} data-testid="mcp-reconnect-btn">
                  {#snippet icon()}
                    {#if reconnecting}<Spinner size={12} />{:else}<RefreshCw size={12} />{/if}
                  {/snippet}
                  {$t("connections.reconnect")}
                </Button>
              </div>
            </div>
          </div>

          <!-- Tabs -->
          <div class="px-8 pt-3.5">
            <TabBar
              variant="underline"
              testidPrefix="mcp-detail"
              activeTab={mcpTab}
              items={[
                { key: "overview", label: $t("connections.mcp_tab_overview") },
                { key: "tools", label: $t("connections.mcp_tab_tools"), count: mcpDetail.tools.length },
                { key: "settings", label: $t("connections.mcp_tab_settings") },
              ]}
              ontabchange={(k) => (mcpTab = k as McpTab)}
            />
          </div>

          <!-- Tab content -->
          <div class="flex-1 overflow-auto px-8 py-5">
            {#if mcpTab === "overview"}
              <div class="space-y-4 max-w-3xl">
                <Card class="p-[14px_16px]">
                  <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
                    {$t("connections.overview_connection")}
                  </div>
                  <ConnectionStatusIndicator health={mcpDetail.status.health} />
                  <div class="flex items-center gap-1.5 text-[11.5px] text-muted-foreground mt-2">
                    <Wrench size={11} />
                    <span data-testid="mcp-tools-count">
                      {$t("integrations.manage.tools_count", { values: { count: mcpDetail.status.tools_count } })}
                    </span>
                  </div>
                  {#if mcpDetail.status.error}
                    <p class="text-[11.5px] text-destructive mt-2" data-testid="mcp-error">{mcpDetail.status.error}</p>
                  {/if}
                </Card>

                <Card class="p-[14px_16px]">
                  <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
                    {$t("connections.overview_activity")}
                  </div>
                  {#if lastActivityLabel !== null}
                    <p class="text-[12.5px] text-foreground" data-testid="mcp-last-activity">
                      {$t("integrations.manage.last_call", { values: { time: lastActivityLabel } })}
                    </p>
                  {:else}
                    <p class="text-[12.5px] text-muted-foreground italic" data-testid="mcp-no-activity">
                      {$t("integrations.manage.no_activity")}
                    </p>
                  {/if}
                </Card>

                {#if testState === "ok"}
                  <p class="text-[12.5px] text-success" data-testid="mcp-test-ok">
                    ✓ {$t("integrations.manage.test_working", { values: { count: testToolCount } })}
                  </p>
                {:else if testState === "unverified"}
                  <p class="text-[12.5px] text-muted-foreground" data-testid="mcp-test-unverified">
                    {$t("integrations.manage.test_reachable", { values: { count: testToolCount } })}
                  </p>
                {:else if testState === "degraded" || testState === "needs_reauth"}
                  <p class="text-[12.5px] text-warning-a11y" data-testid="mcp-test-attention">
                    {#if isOperator}
                      {$t("integrations.manage.test_operator_attention")}
                    {:else}
                      {$t(
                        testState === "needs_reauth"
                          ? "integrations.manage.test_needs_reauth"
                          : "integrations.manage.test_degraded",
                      )}{#if testDetail} - {testDetail}{/if}
                    {/if}
                  </p>
                {:else if testState === "error"}
                  <p class="text-[12.5px] text-destructive" data-testid="mcp-test-error">
                    {testError ?? $t("integrations.manage.test_failed")}
                  </p>
                {/if}
                {#if reconnectError}
                  <p class="text-[12.5px] text-destructive" data-testid="mcp-reconnect-error">{reconnectError}</p>
                {/if}
              </div>
            {:else if mcpTab === "tools"}
              <Card class="p-[14px_16px] max-w-3xl">
                <div class="mb-2.5 flex items-center justify-between">
                  <span class="text-[12.5px] font-semibold text-foreground">{$t("connections.tools_exposed")}</span>
                  <span class="font-mono text-[10.5px] text-muted-foreground">{mcpDetail.tools.length}</span>
                </div>
                {#if mcpDetail.tools.length === 0}
                  <p class="py-4 text-center text-[11.5px] text-muted-foreground">
                    {$t("integrations.manage.tools_empty")}
                  </p>
                {:else}
                  <ul class="space-y-1.5 max-h-[calc(100vh-280px)] overflow-y-auto pr-1" data-testid="mcp-tools-list">
                    {#each mcpDetail.tools as tool (tool.full_name)}
                      <li class="rounded-md border border-border/50 bg-card px-3 py-2" data-testid={`mcp-tool-${tool.local_name}`}>
                        <details class="group">
                          <summary class="flex cursor-pointer items-start justify-between gap-2 list-none">
                            <div class="min-w-0 flex-1">
                              <div class="font-mono text-[12px] text-foreground truncate">{tool.local_name}</div>
                              {#if tool.description}
                                <p class="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{tool.description}</p>
                              {/if}
                            </div>
                            <span class="mt-0.5 text-[10px] uppercase tracking-wide text-muted-foreground group-open:hidden">schema ▾</span>
                            <span class="mt-0.5 hidden text-[10px] uppercase tracking-wide text-muted-foreground group-open:inline">schema ▴</span>
                          </summary>
                          <pre class="mt-2 max-h-48 overflow-auto rounded bg-muted/40 p-2 text-[11px] font-mono leading-relaxed text-muted-foreground">{JSON.stringify(tool.input_schema, null, 2)}</pre>
                        </details>
                      </li>
                    {/each}
                  </ul>
                {/if}
              </Card>
            {:else if mcpTab === "settings"}
              <div class="space-y-4 max-w-3xl">
                <!-- Generic launch-time configuration (command, args, env, timeouts).
                     Component handles stdio + remote transports uniformly and
                     fetches/persists via `get_mcp_server_raw_config` +
                     `update_mcp_server_config`, so the runtime restarts with the
                     new parameters as soon as the user saves. -->
                <McpServerSettingsEditor
                  serverName={server.name}
                  onSaved={() => void fetchMcpDetail(server.name)}
                />

                <Card class="p-[14px_16px]">
                  <div class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 mb-2">
                    {$t("integrations.manage.approval_section")}
                  </div>
                  <ApprovalLevelSelector level={approvalLevel} onchange={handleApprovalChange} />
                  {#if approvalPending}
                    <p class="mt-1.5 flex items-center gap-1 text-[11.5px] text-muted-foreground">
                      <Spinner size={11} />
                      {$t("integrations.manage.approval_saving")}
                    </p>
                  {/if}
                  {#if approvalError}
                    <p class="mt-1.5 text-[11.5px] text-destructive" data-testid="mcp-approval-error">{approvalError}</p>
                  {/if}
                </Card>

                <!-- Disconnect (destructive) -->
                {#if !confirmDisconnect}
                  <Button variant="destructive" size="sm" onclick={() => (confirmDisconnect = true)} data-testid="mcp-disconnect-btn">
                    {#snippet icon()}<Trash2 size={12} />{/snippet}
                    {$t("integrations.manage.disconnect_button")}
                  </Button>
                {:else}
                  <Card class="border-destructive/30 bg-destructive/5 p-[14px_16px] space-y-3" data-testid="mcp-disconnect-confirm">
                    <p class="text-[12.5px] text-foreground">
                      {$t("integrations.manage.disconnect_warning", { values: { name: selection.name } })}
                    </p>
                    {#if disconnectError}
                      <p class="text-[11px] text-destructive">{disconnectError}</p>
                    {/if}
                    <div class="flex gap-2">
                      <Button variant="outline" size="sm"
                        onclick={() => { confirmDisconnect = false; disconnectError = null; }}
                        disabled={disconnecting}
                      >
                        {$t("common.cancel")}
                      </Button>
                      <Button variant="destructive" size="sm"
                        onclick={handleDisconnect}
                        disabled={disconnecting}
                        data-testid="mcp-disconnect-confirm-btn"
                      >
                        {#if disconnecting}<Spinner size={12} class="mr-1.5" />{$t("connections.disconnecting")}{:else}{$t("connections.confirm_disconnect")}{/if}
                      </Button>
                    </div>
                  </Card>
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      {:else}
        <!-- No selection (shouldn't happen post-auto-select, fallback) -->
        <div class="flex-1 flex items-center justify-center text-[12.5px] text-muted-foreground">
          {$t("connections.select_connector_prompt")}
        </div>
      {/if}
  </SplitLayout>
</div>

{#if selectedRegistryServer}
  <ConnectorWizard
    server={selectedRegistryServer}
    open={wizardOpen}
    onclose={handleWizardClose}
    oncomplete={handleWizardComplete}
  />
{/if}

<McpDisclaimerDialog
  open={disclaimerOpen}
  onaccept={handleDisclaimerAccept}
  onclose={() => {
    disclaimerOpen = false;
    selectedRegistryServer = null;
  }}
/>

<ConnectionErrorModal
  open={errorModalOpen}
  server={errorServer}
  enrichment={errorServer ? resolveEnrichment(errorServer) : null}
  onclose={() => (errorModalOpen = false)}
  onretry={handleRetry}
  onviewLogs={handleViewLogs}
/>

<!-- ============ NATIVE OAUTH DIALOG ============ -->
{#if oauthDialogOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4" role="dialog" aria-modal="true">
    <div class="w-full max-w-md rounded-xl bg-surface-1 border border-border p-5 space-y-3">
      <h2 class="text-lg font-semibold">
        {#if oauthDialogStep === "drive_folder"}
          {$t("connections.oauth_drive_folder_title")}
        {:else}
          {$t("connections.oauth_connect_title", { values: { provider: oauthDialogProvider === "google" ? "Google Workspace" : "Microsoft 365" } })}
        {/if}
      </h2>

      {#if oauthDialogStep === "drive_folder"}
        <!-- Step 2 (Google only): pick the Drive root path Apollia agents
             will read/write inside. With the free-tier `drive.file` scope
             Apollia cannot list existing folders for selection - the user
             types the path and Apollia creates each missing segment. -->
        <div class="space-y-3" data-testid="oauth-dialog-drive-folder">
          <p class="text-sm text-muted-foreground leading-[1.5]">
            {@html $t("connections.oauth_drive_folder_body_html")}
          </p>
          <p class="text-[11.5px] text-muted-foreground leading-[1.5]">
            {@html $t("connections.oauth_drive_folder_expert_html")}
          </p>
          <label class="block text-xs font-medium text-foreground">
            {@html $t("connections.oauth_drive_folder_path_label_html")}
            <Input
              type="text"
              class="mt-1 w-full rounded-md border border-border bg-background px-2 py-1.5 text-sm font-mono"
              placeholder="Apollia"
              bind:value={oauthDialogDriveFolderDraft}
              disabled={oauthDialogDriveFolderSaving}
              autocomplete="off"
              data-testid="oauth-drive-folder-input"
            />
          </label>
          <p class="text-[11px] text-muted-foreground leading-[1.4]">
            {@html $t("connections.oauth_drive_folder_examples_html")}
          </p>
          {#if oauthDialogDriveFolderError}
            <p class="text-xs text-destructive" data-testid="oauth-drive-folder-error">
              {oauthDialogDriveFolderError}
            </p>
          {/if}
        </div>
      {:else if oauthDialogErrorIsMissingClient}
        <!-- Specific error: no client_id resolved. Send the user straight to
             Settings → Integrations rather than show a raw error string. -->
        <div class="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 space-y-2">
          <p class="text-sm font-medium text-foreground">
            {$t("connections.oauth_missing_client_title")}
          </p>
          <p class="text-xs text-muted-foreground leading-[1.5]">
            {@html $t("connections.oauth_missing_client_body_html", { values: { provider: oauthDialogProvider === "google" ? "Google" : "Microsoft", console: oauthDialogProvider === "google" ? "Google Cloud" : "Azure / Entra ID" } })}
          </p>
        </div>
      {:else if oauthDialogBusy && !oauthDialogAuthUrl}
        <p class="text-sm text-muted-foreground">{$t("connections.oauth_preparing")}</p>
      {:else if oauthDialogError && !oauthDialogAuthUrl}
        <p class="text-sm text-destructive">{oauthDialogError}</p>
      {:else if oauthDialogAuthUrl}
        <p class="text-sm text-muted-foreground">
          {$t("connections.oauth_consent_opened", { values: { provider: oauthDialogProvider === "google" ? "Google" : "Microsoft" } })}
        </p>

        {#if oauthDialogAwaitingCallback}
          <div class="flex items-center gap-2 rounded-md border border-border bg-muted/40 px-3 py-2">
            <Spinner size={14} />
            <span class="text-xs text-muted-foreground">
              {$t("connections.oauth_awaiting_callback", { values: { provider: oauthDialogProvider === "google" ? "Google" : "Microsoft" } })}
            </span>
          </div>
        {/if}

        <p class="text-xs text-muted-foreground">
          {$t("connections.oauth_window_not_opened")}
          <a
            class="underline"
            href={oauthDialogAuthUrl}
            target="_blank"
            rel="noopener noreferrer">{$t("connections.oauth_open_url_manually")}</a
          >.
        </p>

        <details class="text-xs">
          <summary class="cursor-pointer text-muted-foreground">
            {$t("connections.oauth_paste_code_summary")}
          </summary>
          <div class="mt-2 space-y-2">
            <p class="text-[11px] text-muted-foreground">
              {@html $t("connections.oauth_paste_code_hint_html")}
            </p>
            <Input
              type="text"
              class="font-mono"
              placeholder={$t("connections.custom_mcp.code_placeholder")}
              bind:value={oauthDialogPastedCode}
              disabled={oauthDialogBusy}
            />
          </div>
        </details>
        {#if oauthDialogError}
          <p class="text-xs text-destructive">{oauthDialogError}</p>
        {/if}
      {/if}

      <div class="flex justify-end gap-2 pt-1">
        {#if oauthDialogStep === "drive_folder"}
          <Button variant="ghost" size="sm" onclick={skipDriveFolderStep}
            disabled={oauthDialogDriveFolderSaving}>
            {$t("connections.keep_default")}
          </Button>
          <Button variant="primary-solid" size="sm"
            onclick={saveDriveFolderAndFinish}
            disabled={oauthDialogDriveFolderSaving}
            data-testid="oauth-drive-folder-save"
          >
            {oauthDialogDriveFolderSaving ? $t("connections.saving") : $t("connections.save")}
          </Button>
        {:else}
          <Button variant="outline" size="sm" onclick={closeOauthDialog}>{$t("common.cancel")}</Button>
          {#if oauthDialogErrorIsMissingClient}
            <Button variant="primary-solid" size="sm" onclick={openSettingsIntegrations}>
              {$t("connections.open_settings_integrations")}
            </Button>
          {:else if oauthDialogAuthUrl}
            <Button variant="primary-solid" size="sm"
              onclick={completeNativeFlow}
              disabled={oauthDialogBusy || oauthDialogPastedCode.trim().length === 0}
            >
              {oauthDialogBusy ? $t("common.finalizing") : $t("common.finalize")}
            </Button>
          {/if}
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- ============ CATALOGUE SHEET ============ -->
<!-- The public MCP catalogue + the "MCP personnalisé" form live together in
     a single Sheet. They share the same audience ("add a new connector"),
     and keeping them in one place avoids forking the entry-point UX.

     Load strategy (2026-05-15) :
       1. On Sheet open → `fetch_mcp_curated()` returns the 18 baked entries
          instantly (no network). User sees content in < 100ms.
       2. The full public catalogue (~6k entries, 30–60s cold-cache) is
          opt-in via the "Explorer le catalogue public" button at the
          bottom of the curated list. Subsequent calls within 15 min hit
          the disk cache and return in ~100ms.

     Layout : vertical row list (single column at narrow widths, 2-column
     grid wide). Cards on a Sheet contracted by the left rail were
     illegible (titles truncated to 1 char) - rows scale to any width. -->
<Sheet
  open={catalogueOpen}
  onclose={() => { catalogueOpen = false; resetCustomForm(); }}
  class="w-full sm:max-w-[920px]"
>
  <SheetHeader
    title={$t("connections.add_connector")}
    subtitle={$t("connections.catalogue_subtitle")}
    onclose={() => { catalogueOpen = false; resetCustomForm(); }}
  />
  <SheetContent padding="flush" class="flex flex-col">
    <div class="px-6 pt-3 shrink-0">
      <TabBar
        variant="underline"
        testidPrefix="catalogue"
        activeTab={catalogueTab}
        items={[
          { key: "discover", label: $t("connections.catalogue_tab_discover"), count: filteredMcp.length },
          { key: "custom", label: $t("connections.catalogue_tab_custom") },
        ]}
        ontabchange={(k) => (catalogueTab = k as CatalogueTab)}
      />
    </div>

    {#if catalogueTab === "discover"}
      <!-- Trust-level filter chips -->
      <FilterChipBar
        chips={[
          { key: "all", label: $t("connections.mcp_chip_all"), color: "hsl(var(--muted-foreground))", count: mcpEntries.length },
          { key: "installed", label: $t("connections.mcp_chip_installed"), color: "hsl(var(--success))", count: installedRegCount },
          { key: "official", label: $t("connections.mcp_chip_official"), color: "hsl(var(--primary))", count: officialCount },
          { key: "community", label: $t("connections.mcp_chip_community"), color: "hsl(var(--info))", count: communityCount },
        ]}
        activeKey={mcpFilter}
        onchange={(key) => (mcpFilter = key as "all" | "installed" | "official" | "community")}
        aria-label={$t("connections.mcp_filter_label")}
        testidPrefix="catalogue-filter"
        class="px-6 pt-4 pb-3"
      />

      <div class="flex-1 overflow-auto px-6 pb-6">
        {#if registryLoading && mcpEntries.length === 0}
          <div class="space-y-2" data-testid="catalogue-loading">
            <div class="flex items-center gap-2 text-xs text-muted-foreground">
              <Spinner size={12} />
              {$t("connections.catalogue_loading")}
            </div>
            {#each Array(6) as _, i (i)}
              <Skeleton class="h-16 rounded-lg bg-surface-1 border border-border" />
            {/each}
          </div>
        {:else if mcpEntries.length === 0}
          <EmptyState
            title={$t("connections.empty.no_suggestions_title")}
            desc={$t("connections.empty.no_suggestions_description")}
            tone="neutral"
          >
            {#snippet icon()}<Sparkles size={22} />{/snippet}
          </EmptyState>
        {:else if filteredMcp.length === 0}
          <EmptyState
            title={$t("connections.empty.no_results_title", { values: { query: "" } })}
            desc={$t("connections.empty.no_results_description")}
            tone="neutral"
          >
            {#snippet action()}
              <Button variant="outline" size="sm" onclick={() => (mcpFilter = "all")}>
                {$t("connections.filters.clear_all")}
              </Button>
            {/snippet}
          </EmptyState>
        {:else}
          <!-- Vertical row list - readable at any Sheet width, scales to
               long descriptions and long vendor names without truncation
               nightmares. -->
          <ul class="space-y-1.5" data-testid="catalogue-list">
            {#each visibleMcp as entry (entry.name)}
              {@const installedName = installedNameFor(entry)}
              {@const installed = entry.is_installed || installedName !== null}
              {@const isOfficial =
                entry.trust_level === "verified_official" ||
                entry.trust_level === "community_verified"}
              {@const displayName = entry.enrichment?.operator_label ?? entry.title ?? entry.name}
              {@const description = entry.description ?? entry.enrichment?.category ?? ""}
              {@const vendor = vendorLabel(entry)}
              <li>
                <button
                  type="button"
                  class="w-full text-left flex items-start gap-3 px-3 py-2.5 rounded-lg border border-border/60 bg-card hover:border-primary/40 hover:bg-primary/5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60"
                  onclick={() =>
                    installed
                      ? (() => { catalogueOpen = false; handleManage(installedName ?? entry.name); })()
                      : handleConnect(entry)}
                  data-testid="catalogue-entry-{entry.name}"
                >
                  <div
                    class="w-9 h-9 shrink-0 rounded-md inline-flex items-center justify-center text-white"
                    style="background: {logoColorFor(displayName)};"
                  >
                    <LinkIcon size={14} />
                  </div>
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2 min-w-0 mb-0.5">
                      <span class="text-[13px] font-medium text-foreground truncate">{displayName}</span>
                      {#if isOfficial}
                        <Badge size="sm" variant="primary" class="shrink-0 text-[9px] px-1 py-0 leading-[1.4]">{$t("connections.badge_official")}</Badge>
                      {:else}
                        <Badge size="sm" variant="neutral" outline class="shrink-0 text-[9px] px-1 py-0 leading-[1.4]">{$t("connections.badge_community")}</Badge>
                      {/if}
                      {#if installed}
                        <Badge size="sm" variant="success" class="shrink-0 text-[9px] px-1 py-0 leading-[1.4]">{$t("connections.badge_installed")}</Badge>
                      {/if}
                    </div>
                    {#if vendor}
                      <div class="text-[10.5px] text-muted-foreground mb-0.5">
                        {$t("connections.catalogue_by")} <span class="text-foreground/80 font-medium">{vendor}</span>
                      </div>
                    {/if}
                    {#if description}
                      <p class="text-[11.5px] text-muted-foreground leading-[1.45] line-clamp-2">{description}</p>
                    {/if}
                  </div>
                  <div class="shrink-0 self-center">
                    {#if installed}
                      <span class="text-[11px] text-primary font-medium">{$t("connections.catalogue_manage")}</span>
                    {:else}
                      <span class="text-[11px] text-primary font-medium inline-flex items-center gap-1">
                        <Plus size={11} /> {$t("connections.connect")}
                      </span>
                    {/if}
                  </div>
                </button>
              </li>
            {/each}
          </ul>

          <!-- IntersectionObserver sentinel - auto-paginates DOM rendering
               as the user scrolls (Amazon-style). When near the bottom,
               `catalogLimit` is bumped to reveal the next chunk. -->
          <div bind:this={catalogueSentinel} class="h-px" aria-hidden="true"></div>

          <!-- Live status footer : background catalogue load + scroll hint. -->
          {#if registryLoading && catalogueTier !== "full"}
            <div class="mt-4 flex items-center justify-center gap-2 text-[11.5px] text-muted-foreground" data-testid="catalogue-bg-load">
              <Spinner size={11} />
              {$t("connections.catalogue_loading_public", { values: { count: mcpEntries.length } })}
            </div>
          {:else if hasMoreMcp}
            <div class="mt-4 flex items-center justify-center text-[11px] text-muted-foreground/70" data-testid="catalogue-more-hint">
              {$t("connections.catalogue_more_hint", { values: { count: filteredMcp.length - catalogLimit } })}
            </div>
          {:else if catalogueTier === "full" && filteredMcp.length > 0}
            <div class="mt-4 flex items-center justify-center text-[11px] text-muted-foreground/60">
              {$t("connections.catalogue_end", { values: { count: filteredMcp.length } })}
            </div>
          {/if}
        {/if}
      </div>
    {:else if catalogueTab === "custom"}
      <div class="flex-1 overflow-auto px-6 pt-4 pb-6 max-w-2xl">
        <p class="text-[12px] text-muted-foreground mb-4">
          {$t("connections.custom_intro")}
        </p>

        <div class="space-y-3">
          <label class="block text-[11.5px] font-medium text-foreground">
            {$t("connections.custom_name_label")}
            <Input
              type="text"
              class="mt-1"
              placeholder={$t("connections.custom_name_placeholder")}
              bind:value={customForm.name}
              disabled={customBusy}
            />
          </label>

          <label class="block text-[11.5px] font-medium text-foreground">
            {$t("connections.custom_transport_label")}
            <Select
              class="mt-1"
              bind:value={customForm.transport}
              disabled={customBusy}
            >
              <option value="stdio">{$t("connections.custom_transport_stdio")}</option>
              <option value="streamable-http">{$t("connections.custom_transport_http")}</option>
              <option value="sse">{$t("connections.custom_transport_sse")}</option>
            </Select>
          </label>

          {#if customForm.transport === "stdio"}
            <label class="block text-[11.5px] font-medium text-foreground">
              {$t("connections.custom_command_label")}
              <Input
                type="text"
                class="mt-1 font-mono"
                placeholder="npx, uvx, /path/to/binary…"
                bind:value={customForm.command}
                disabled={customBusy}
              />
            </label>
            <label class="block text-[11.5px] font-medium text-foreground">
              {$t("connections.custom_args_label")}
              <Input
                type="text"
                class="mt-1 font-mono"
                placeholder="@modelcontextprotocol/server-filesystem /tmp"
                bind:value={customForm.args}
                disabled={customBusy}
              />
            </label>
          {:else}
            <label class="block text-[11.5px] font-medium text-foreground">
              {$t("connections.custom_url_label")}
              <Input
                type="url"
                class="mt-1 font-mono"
                placeholder="https://mcp.example.com/v1"
                bind:value={customForm.url}
                disabled={customBusy}
              />
            </label>
          {/if}

          <label class="block text-[11.5px] font-medium text-foreground">
            {customForm.transport === "stdio" ? $t("connections.custom_env_label") : $t("connections.custom_headers_label")}
            <span class="font-normal text-muted-foreground">{@html $t("connections.custom_env_hint_html")}</span>
            <Textarea
              class="mt-1 font-mono"
              rows={3}
              placeholder={customForm.transport === "stdio"
                ? "NOTION_TOKEN=ntn_xxxxx\nDEBUG=1"
                : "Authorization=Bearer xxx\nX-Custom-Header=value"}
              bind:value={customForm.headers}
              disabled={customBusy}
            />
          </label>

          <label class="flex items-center gap-2 pt-1 text-[11.5px] font-medium text-foreground">
            <Checkbox bind:checked={customForm.requires_approval} disabled={customBusy} />
            {$t("connections.custom_require_approval")}
          </label>
        </div>

        {#if customTestResult}
          <div class="mt-4 rounded-md border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-700 dark:text-emerald-300">
            ✓ {$t("connections.custom_test_ok", { values: { result: customTestResult } })}
          </div>
        {/if}
        {#if customError}
          <div class="mt-4 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {customError}
          </div>
        {/if}

        <div class="mt-5 flex justify-end gap-2">
          <Button variant="outline" size="sm" onclick={testCustomServer} disabled={customBusy}>
            {customBusy ? $t("connections.testing") : $t("connections.test")}
          </Button>
          <Button variant="primary-solid" size="sm" onclick={installCustomServer} disabled={customBusy}>
            {customBusy ? $t("connections.installing") : $t("connections.install")}
          </Button>
        </div>
      </div>
    {/if}
  </SheetContent>
</Sheet>
