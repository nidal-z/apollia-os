<script lang="ts">
  /**
   * ServersSection - what the content of the AI screen shows when the column
   * points at the servers: the saved server that was chosen, the form of a new
   * one or of a change, or, while none is connected, what a server brings. It carries the gestures that need
   * a confirmation or a toast; the decisions live in `servers.ts`.
   */
  import { get } from "svelte/store";
  import { t } from "svelte-i18n";
  import { Server } from "lucide-svelte";
  import type { DeclaredServer, ServerModel } from "$lib/ipc/llmServers";
  import { reportError } from "$lib/errors/reportError";
  import { addToast } from "$lib/components/ui/toast";
  import { Button } from "$lib/components/ui/button";
  import ConfirmDialog from "$lib/components/ui/dialog/ConfirmDialog.svelte";
  import { EmptyState, ErrorBanner } from "$lib/components/operator";
  import ServerForm from "./ServerForm.svelte";
  import ServerPanel from "./ServerPanel.svelte";
  import type { Servers, ServersView } from "./servers";

  interface Props {
    servers: Servers;
    view: ServersView;
    onview: (view: ServersView | null) => void;
  }

  let { servers, view, onview }: Props = $props();

  let deleteTarget = $state<DeclaredServer | null>(null);
  let deleting = $state(false);

  const current = $derived(
    view.kind === "add" || view.kind === "none"
      ? null
      : ($servers.servers.find((s) => s.name === view.name) ?? null),
  );

  async function attempt(gesture: () => Promise<void>): Promise<void> {
    try {
      await gesture();
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    }
  }

  async function copy(text: string, okKey: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
      addToast(get(t)(okKey), "success");
    } catch {
      addToast(get(t)("ai.toast.copy_failed"), "error");
    }
  }

  function toggle(server: DeclaredServer, enabled: boolean): void {
    void attempt(async () => {
      await servers.setEnabled(server.name, enabled);
      addToast(
        get(t)(enabled ? "ai.servers.toast.enabled" : "ai.servers.toast.disabled", {
          values: { name: server.name },
        }),
        "success",
      );
    });
  }

  function useDefault(server: DeclaredServer, model: ServerModel): void {
    void attempt(async () => {
      await servers.setDefault(server.name, model.id);
      addToast(get(t)("ai.toast.default_set", { values: { name: model.id } }), "success");
    });
  }

  function saved(server: DeclaredServer): void {
    addToast(get(t)("ai.servers.toast.saved", { values: { name: server.name } }), "success");
    onview({ kind: "server", name: server.name });
  }

  async function confirmDelete(): Promise<void> {
    const server = deleteTarget;
    if (!server || deleting) return;
    deleting = true;
    try {
      await servers.remove(server.name);
      addToast(get(t)("ai.servers.toast.deleted", { values: { name: server.name } }), "success");
      deleteTarget = null;
      onview(null);
    } catch (err: unknown) {
      reportError(err, { surface: "toast" });
    } finally {
      deleting = false;
    }
  }
</script>

{#snippet emptyIcon()}
  <Server size={24} strokeWidth={1.8} aria-hidden="true" />
{/snippet}

{#snippet emptyAction()}
  <Button variant="primary-solid" size="sm" onclick={() => onview({ kind: "add" })} data-testid="ai-servers-empty-add">
    {$t("ai.servers.add")}
  </Button>
{/snippet}

<div class="flex min-h-0 flex-1 flex-col gap-4" data-testid="ai-servers-section">
  {#if $servers.error}
    <ErrorBanner
      message={$t("ai.servers.list_error")}
      onretry={() => void servers.refresh()}
      retryLabel={$t("common.retry")}
      data-testid="ai-servers-error"
    />
  {/if}

  {#if view.kind === "add"}
    <ServerForm {servers} editing={null} onsaved={saved} oncancel={() => onview(null)} />
  {:else if current === null}
    <div data-testid="ai-servers-empty">
      <EmptyState
        icon={emptyIcon}
        title={$t("ai.servers.empty_title")}
        desc={$t("ai.servers.empty_hint")}
        action={emptyAction}
      />
    </div>
  {:else if view.kind === "edit"}
    {#key current.name}
      <ServerForm
        {servers}
        editing={current}
        onsaved={saved}
        oncancel={() => onview({ kind: "server", name: current.name })}
      />
    {/key}
  {:else}
    <ServerPanel
      server={current}
      models={$servers.models[current.name]}
      defaultModel={$servers.defaultModel}
      busy={$servers.busy === current.name}
      ontest={() => void servers.loadModels(current.name)}
      onedit={() => onview({ kind: "edit", name: current.name })}
      ontoggle={(enabled) => toggle(current, enabled)}
      oncopyaddress={() => void copy(current.base_url, "ai.servers.toast.address_copied")}
      ondelete={() => (deleteTarget = current)}
      onusedefault={(model) => useDefault(current, model)}
      oncopymodel={(model) => void copy(model.route, "ai.toast.copied_id")}
    />
  {/if}
</div>

<ConfirmDialog
  open={deleteTarget !== null}
  onclose={() => (deleteTarget = null)}
  onconfirm={() => void confirmDelete()}
  title={$t("ai.servers.delete_dialog.title", { values: { name: deleteTarget?.name ?? "" } })}
  message={$t("ai.servers.delete_dialog.message")}
  confirmLabel={$t("ai.servers.delete_dialog.confirm")}
  requireText={deleteTarget?.name ?? ""}
  requireTextLabel={$t("ai.servers.delete_dialog.type_label", {
    values: { name: deleteTarget?.name ?? "" },
  })}
  loading={deleting}
  data-testid="ai-server-delete-dialog"
/>
