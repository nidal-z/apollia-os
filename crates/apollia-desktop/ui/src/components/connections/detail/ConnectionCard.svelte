<script lang="ts">
  /**
   * ConnectionCard - the card of one connection, whatever its kind: whether
   * it works, since when, why not and the one action that repairs it; Check,
   * Pause or Resume, and Remove, confirmed in the page after the list of what
   * goes with it; the tools it brings; for a Google account, the Drive folder
   * its agents work in, for a Microsoft account, the OneDrive folders and
   * files Apollia may read; the agents that use it and Give to an agent; what
   * they did with it this week; its tools with their family. The technical detail is a section of the developer
   * mode, the OAuth client of an account with it; the settings of a service
   * stay on the card, their launch and transport for the developer mode.
   */
  import { t } from "svelte-i18n";
  import { Folder, Link as LinkIcon, Mail, Pause, Play, Plus, RefreshCw, Trash2, UserRound, Wrench } from "lucide-svelte";
  import { DetailHeader } from "$lib/components/operator";
  import { DeveloperOnly } from "$lib/components/shared";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Card } from "$lib/components/ui/card";
  import { InlineConfirm } from "$lib/components/ui/inline-confirm";
  import { addToast } from "$lib/components/ui/toast";
  import { createIdentityGuard } from "$lib/utils/identityGuard";
  import { messageOf } from "$lib/connections/errors";
  import {
    pauseGesture,
    removalLines,
    repairAction,
    type RepairKind,
  } from "$lib/connections/connectionCard";
  import { nativeProviderOf, serverNameOf } from "$lib/connections/connectionRows";
  import { brandLogoPath } from "$lib/connections/brandLogo";
  import { driveFolderFor, listDriveFolders } from "$lib/connections/oauthClient";
  import {
    checkConnection,
    getMcpServerDetail,
    getMcpServerRawConfig,
    mcpOauthLogin,
    pauseConnection,
    removeConnection,
    restartMcpServer,
    resumeConnection,
    type ConnectionRemoval,
  } from "$lib/ipc/connections";
  import type { DriveFolderStatus, OauthClientIdStatus } from "$lib/ipc/oauthClients";
  import type { Connection, McpToolSummaryView } from "$lib/types";
  import { connectionToolName } from "../../agents/sheet/connectionGrant";
  import ConnectionStateBadge from "../shared/ConnectionStateBadge.svelte";
  import BrandLogo from "../shared/BrandLogo.svelte";
  import McpToolsTab from "./McpToolsTab.svelte";
  import ConnectionAgentsSection from "./ConnectionAgentsSection.svelte";
  import ConnectionActivitySection from "./ConnectionActivitySection.svelte";
  import ToolFamilyControl from "./ToolFamilyControl.svelte";
  import AccountDriveFolder from "./AccountDriveFolder.svelte";
  import AccountOneDriveChoices from "./AccountOneDriveChoices.svelte";
  import NativeClientSection from "./NativeClientSection.svelte";
  import OauthAdminApproval from "../oauth/OauthAdminApproval.svelte";
  import McpServerSettingsEditor from "../../integrations/McpServerSettingsEditor.svelte";

  interface Props {
    connection: Connection;
    /** The name the list gives it. */
    label: string;
    /** The icon of the selection entry it was added from (`brandLogoPath`), null for none. */
    iconName?: string | null;
    /** The service is running, so its tools can be read with their schema. */
    serverRunning?: boolean;
    /** The OAuth client of an account's provider; `undefined` while it could not be read. */
    clientStatus?: OauthClientIdStatus;
    /** Why the client could not be read, if it could not. */
    clientError?: string | null;
    /** Read the client again. */
    onclientretry?: () => void;
    /** A gesture answered the connection as it now is. */
    onupdated: (connection: Connection) => void;
    /** The connection was removed. */
    onremoved: () => void;
    /** Something changed the runtime holds beyond the connection: read the page again. */
    onreload: () => void;
    /**
     * The repair that leaves this card: sign in again to an account, the
     * password, the folder, the rule. A service is started or signed in again
     * on the card itself.
     */
    onrepair: (kind: Exclude<RepairKind, "restart" | "admin_approval">) => void;
    /** "Try": a conversation limited to the tools of this connection. */
    ontry?: () => void;
    /** Connect another account of the same provider, for an account. */
    onaddaccount?: () => void;
  }

  let {
    connection,
    label,
    iconName = null,
    serverRunning = false,
    clientStatus,
    clientError = null,
    onclientretry,
    onupdated,
    onremoved,
    onreload,
    onrepair,
    ontry,
    onaddaccount,
  }: Props = $props();

  let busy = $state<"check" | "pause" | "repair" | "remove" | null>(null);
  let error = $state<string | null>(null);
  /** What the removal will erase, read before it is confirmed. */
  let removal = $state<ConnectionRemoval | null>(null);
  let serverTools = $state<McpToolSummaryView[] | null>(null);
  let driveFolders = $state<DriveFolderStatus[]>([]);

  const serverName = $derived(serverNameOf(connection.id));
  const provider = $derived(nativeProviderOf(connection));
  const repair = $derived(repairAction(connection));
  const gesture = $derived(pauseGesture(connection.status));
  const lines = $derived(removal ? removalLines(removal.removed) : []);
  const meta = $derived(connection.account ?? connection.path ?? "");
  const logo = $derived(brandLogoPath(iconName));
  const Icon = $derived(
    connection.kind === "native"
      ? UserRound
      : connection.kind === "imap"
        ? Mail
        : connection.kind === "folder"
          ? Folder
          : LinkIcon,
  );

  // An answer for the connection the user just left must not land on the one
  // just chosen: the list stays clickable while a call runs.
  const guard = createIdentityGuard(() => connection.id);

  $effect(() => {
    void connection.id;
    busy = null;
    error = null;
    removal = null;
    serverTools = null;
  });

  $effect(() => {
    const name = serverName;
    if (name === null || !serverRunning) return;
    const ticket = guard.begin();
    getMcpServerDetail(name)
      .then((detail) => {
        if (ticket.current) serverTools = detail.tools;
      })
      .catch(() => {
        if (ticket.current) serverTools = null;
      });
  });

  async function refreshDriveFolders(): Promise<void> {
    try {
      driveFolders = await listDriveFolders();
    } catch {
      driveFolders = [];
    }
  }

  $effect(() => {
    if (provider === "google" && connection.account) void refreshDriveFolders();
  });

  /** Run `call` as the gesture `kind`, its failure said on the card. */
  async function run(kind: NonNullable<typeof busy>, call: () => Promise<void>): Promise<void> {
    const ticket = guard.begin();
    busy = kind;
    error = null;
    try {
      await call();
    } catch (err: unknown) {
      if (ticket.current) error = messageOf(err);
    } finally {
      if (ticket.current) busy = null;
    }
  }

  function check(): void {
    void run("check", async () => onupdated(await checkConnection(connection.id)));
  }

  function togglePause(): void {
    void run("pause", async () => {
      const call = gesture === "resume" ? resumeConnection : pauseConnection;
      onupdated(await call(connection.id));
    });
  }

  function doRepair(): void {
    const kind = repair?.kind;
    if (!kind) return;
    const name = serverName;
    if (name !== null && kind === "sign_in") {
      void run("repair", async () => {
        const config = await getMcpServerRawConfig(name);
        await mcpOauthLogin({ serverName: config.name, serverUrl: config.url ?? "", wwwAuthenticate: null, scopes: [] });
        onupdated(await checkConnection(connection.id));
      });
      return;
    }
    if (kind === "admin_approval") {
      // Once the administrator approved, connecting again is what lifts the block.
      onrepair("sign_in");
      return;
    }
    if (kind !== "restart") {
      onrepair(kind);
      return;
    }
    if (name === null) return;
    void run("repair", async () => {
      await restartMcpServer(name);
      onreload();
    });
  }

  /** Read what the removal will erase, then ask in the page. */
  function askRemoval(): void {
    void run("remove", async () => {
      removal = await removeConnection(connection.id, true);
    });
  }

  function confirmRemoval(): void {
    void run("remove", async () => {
      const done = await removeConnection(connection.id, false);
      removal = null;
      // The card goes with the connection, so what could not be erased is
      // said where it stays visible.
      if (done.left.length > 0) {
        addToast(
          $t("connections.card.remove.left", {
            values: { items: done.left.map((item) => item.name).join(", ") },
          }),
          "warning",
          { "data-testid": "connection-remove-left" },
        );
      }
      onremoved();
    });
  }
</script>

<DetailHeader title={label} titleTestid="connection-detail-title" {meta}>
  {#snippet leading()}
    {#if logo}
      <BrandLogo path={logo} size="lg" data-testid="connection-detail-logo" />
    {:else}
      <span class="inline-flex h-10 w-10 items-center justify-center rounded-lg bg-muted text-foreground">
        <Icon size={18} aria-hidden="true" />
      </span>
    {/if}
  {/snippet}
  {#snippet badges()}
    {#if connection.remote}
      <Badge size="sm" variant="info" data-testid="connection-detail-remote">
        {$t("connections.remote")}
      </Badge>
    {/if}
  {/snippet}
  {#snippet actions()}
    <Button
      variant="outline"
      size="sm"
      onclick={check}
      loading={busy === "check"}
      disabled={busy !== null}
      data-testid="connection-check-btn"
    >
      {#snippet icon()}<RefreshCw size={12} />{/snippet}
      {$t("connections.card.check")}
    </Button>
    {#if provider !== null && onaddaccount}
      <Button variant="outline" size="sm" onclick={onaddaccount} data-testid="oauth-add-account-{provider}">
        {#snippet icon()}<Plus size={12} />{/snippet}
        {$t("connections.card.add_account")}
      </Button>
    {/if}
    {#if ontry}
      <Button variant="primary-solid" size="sm" onclick={ontry} data-testid="connection-try-btn">
        {#snippet icon()}<Play size={12} />{/snippet}
        {$t("connections.try.action")}
      </Button>
    {/if}
  {/snippet}
</DetailHeader>

<div class="flex-1 space-y-4 overflow-auto px-8 py-5">
  <Card class="max-w-3xl space-y-3 px-4 py-3.5" data-testid="connection-card-state">
    <ConnectionStateBadge
      status={connection.status}
      reason={connection.reason}
      checkedAt={connection.checkedAt}
      inline
      data-testid="connection-detail-state"
    />
    {#if connection.status === "paused"}
      <p class="text-caption text-muted-foreground">{$t("connections.card.paused_note")}</p>
    {/if}
    {#if repair?.kind === "admin_approval" && connection.reason?.address}
      <OauthAdminApproval
        providerShort={provider === "google" ? "Google" : "Microsoft"}
        address={connection.reason.address}
      />
    {/if}
    {#if repair}
      <Button
        variant="primary-solid"
        size="sm"
        onclick={doRepair}
        loading={busy === "repair"}
        disabled={busy !== null}
        data-testid="connection-repair-btn"
        data-repair={repair.kind}
      >
        {$t(repair.labelKey)}
      </Button>
    {/if}
    {#if error}
      <p class="text-caption text-destructive" data-testid="connection-card-error">{error}</p>
    {/if}
  </Card>

  {#if removal}
    <div class="max-w-3xl">
      <InlineConfirm
        message={$t("connections.card.remove.message", { values: { name: label } })}
        confirmLabel={$t("connections.card.remove.confirm")}
        busyLabel={$t("connections.card.remove.removing")}
        busy={busy === "remove"}
        onconfirm={confirmRemoval}
        oncancel={() => (removal = null)}
        data-testid="connection-remove-confirm"
      >
        {#if lines.length > 0}
          <ul class="space-y-1 text-body-xs text-foreground/85" data-testid="connection-remove-items">
            {#each lines as line, i (i)}
              <li class="flex items-start gap-2">
                <span class="mt-1.5 h-1 w-1 shrink-0 rounded-full bg-muted-foreground/60"></span>
                <span>{$t(line.key, { values: line.values })}</span>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="text-caption text-muted-foreground">{$t("connections.card.remove.nothing_else")}</p>
        {/if}
      </InlineConfirm>
    </div>
  {:else}
    <div class="flex max-w-3xl flex-wrap gap-2">
      {#if gesture}
        <Button
          variant="outline"
          size="sm"
          onclick={togglePause}
          loading={busy === "pause"}
          disabled={busy !== null}
          data-testid={gesture === "resume" ? "connection-resume-btn" : "connection-pause-btn"}
        >
          {#snippet icon()}{#if gesture === "resume"}<Play size={12} />{:else}<Pause size={12} />{/if}{/snippet}
          {$t(gesture === "resume" ? "connections.card.resume" : "connections.card.pause")}
        </Button>
      {/if}
      <Button
        variant="ghost"
        size="sm"
        class="text-destructive hover:bg-destructive/10"
        onclick={askRemoval}
        loading={busy === "remove"}
        disabled={busy !== null}
        data-testid="connection-remove-btn"
      >
        {#snippet icon()}<Trash2 size={12} />{/snippet}
        {$t("connections.card.remove.action")}
      </Button>
    </div>
  {/if}

  <ConnectionAgentsSection {connection} />
  <ConnectionActivitySection {connection} {label} />

  {#if provider === "google" && connection.account}
    <Card class="max-w-3xl px-4 py-1">
      <AccountDriveFolder
        accountId={connection.account}
        folder={driveFolderFor(driveFolders, connection.account)}
        onchanged={() => void refreshDriveFolders()}
      />
    </Card>
  {/if}

  {#if provider === "microsoft" && connection.account}
    <Card class="max-w-3xl px-4 py-1">
      <AccountOneDriveChoices accountId={connection.account} />
    </Card>
  {/if}

  <div data-testid="connection-detail-tools">
  {#if serverName !== null && serverTools !== null}
    <McpToolsTab tools={serverTools} {serverName} {connection} {onupdated} />
  {:else}
    <Card class="max-w-3xl px-4 py-3.5">
      <div class="mb-2.5 flex items-center justify-between">
        <span class="text-label-md font-semibold text-foreground">{$t("connections.tools_exposed")}</span>
        <span class="text-caption text-muted-foreground">{connection.tools.length}</span>
      </div>
      {#if connection.tools.length === 0}
        <p class="py-4 text-center text-caption text-muted-foreground">{$t("integrations.manage.tools_empty")}</p>
      {:else}
        <ul class="space-y-1">
          {#each connection.tools as tool (tool.id)}
            <li class="flex flex-wrap items-center gap-2 text-body-xs text-foreground" data-testid="connection-tool-{tool.id}">
              <Wrench size={11} class="shrink-0 text-muted-foreground" aria-hidden="true" />
              <span data-testid="connection-tool-name-{tool.id}">{connectionToolName(tool.id)}</span>
              <DeveloperOnly>
                <span class="font-mono text-caption text-muted-foreground" data-testid="connection-tool-id-{tool.id}">{tool.id}</span>
              </DeveloperOnly>
              <ToolFamilyControl {connection} toolId={tool.id} {onupdated} />
            </li>
          {/each}
        </ul>
      {/if}
    </Card>
  {/if}
  </div>

  {#if serverName !== null}
    <!-- The editor keeps the launch and the transport for the developer mode
         itself; the arguments, the address, the secrets and the sign-in are
         the user's. -->
    <div class="max-w-3xl">
      <McpServerSettingsEditor {serverName} onSaved={onreload} />
    </div>
  {/if}

  <DeveloperOnly>
    <section class="max-w-3xl space-y-3" data-testid="connection-technical">
      <h3 class="text-overline text-muted-foreground/60">{$t("dev.connections.card.technical_title")}</h3>
      <Card class="space-y-1 px-4 py-3.5 font-mono text-caption text-muted-foreground">
        <p data-testid="connection-detail-provenance">
          {connection.id} · {connection.kind} · {connection.provenance}{connection.official
            ? ` · ${$t("connections.badge_official")}`
            : ""}
        </p>
        {#if connection.reason?.detail}
          <p data-testid="connection-detail-reason">{connection.reason.detail}</p>
        {/if}
        {#each connection.tools as tool (tool.id)}
          <p data-testid="connection-tool-provenance">{tool.id} · {tool.family} ({tool.familySource})</p>
        {/each}
      </Card>
      {#if provider !== null}
        <NativeClientSection
          {provider}
          status={clientStatus}
          loadError={clientError}
          loading={false}
          onretry={() => onclientretry?.()}
          onchanged={() => onclientretry?.()}
        />
      {/if}
    </section>
  </DeveloperOnly>
</div>
