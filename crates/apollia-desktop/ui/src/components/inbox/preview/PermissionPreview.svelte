<script lang="ts" module>
  /**
   * Lazy-loaded permission preview dispatcher (US-SP42-054).
   *
   * The preview is not calculated until the operator clicks "Load
   * preview" — this avoids paying the dry-run cost for every item they
   * simply scroll past. Results are cached per `tool_call_id` for 5 min
   * in-memory so navigating away and back doesn't re-invoke the backend.
   */
  const CACHE_TTL_MS = 5 * 60 * 1000;
  const cache: Map<string, { at: number; payload: PermissionPreviewPayload }> = new Map();

  export function clearPreviewCache(): void {
    cache.clear();
  }
</script>

<script lang="ts">
  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { Loader2, Eye } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import PreviewFilesystem from "./PreviewFilesystem.svelte";
  import PreviewBash from "./PreviewBash.svelte";
  import PreviewMcp from "./PreviewMcp.svelte";
  import PreviewNetwork from "./PreviewNetwork.svelte";
  import PermissionErrorBanner from "../../permissions/PermissionErrorBanner.svelte";
  import { emitPreviewLoaded } from "$lib/permissions/telemetry";
  import type { PermissionPreviewPayload, PreviewKind } from "./types";

  interface Props {
    /** Unique id for the tool call — used as the cache key. */
    toolCallId: string;
    /** Backend preview dispatcher kind. */
    kind: PreviewKind;
    /** Tool name for telemetry. */
    toolName: string;
    /** Arguments to forward to the backend preview command. */
    args?: Record<string, unknown>;
  }

  let { toolCallId, kind, toolName, args = {} }: Props = $props();

  let loading = $state(false);
  let payload = $state<PermissionPreviewPayload | null>(null);
  let error = $state<string | null>(null);

  function commandFor(k: PreviewKind): string | null {
    switch (k) {
      case "filesystem": return "preview_filesystem_action";
      case "bash": return "preview_bash_action";
      case "mcp": return "preview_mcp_action";
      case "network": return "preview_network_action";
      default: return null;
    }
  }

  async function load() {
    error = null;
    const cached = cache.get(toolCallId);
    if (cached && Date.now() - cached.at < CACHE_TTL_MS) {
      payload = cached.payload;
      return;
    }
    const cmd = commandFor(kind);
    if (!cmd) {
      error = "unsupported preview kind";
      return;
    }
    loading = true;
    try {
      const res = await invoke<PermissionPreviewPayload>(cmd, { toolCallId, args });
      payload = res;
      cache.set(toolCallId, { at: Date.now(), payload: res });
      void emitPreviewLoaded(kind, toolName);
    } catch (err: unknown) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }
</script>

<div class="space-y-3" data-testid="permission-preview" data-kind={kind}>
  {#if payload === null && !loading && !error}
    <div class="flex items-center justify-between rounded-md border border-dashed border-border px-3 py-2">
      <div class="flex items-center gap-2 text-xs text-muted-foreground">
        <Eye size={14} />
        <span>{$t("permissions.preview.lazy_hint")}</span>
      </div>
      <Button
        size="sm"
        variant="outline"
        onclick={load}
        data-testid="permission-preview-load"
      >
        {$t("permissions.preview.load")}
      </Button>
    </div>
  {/if}

  {#if loading}
    <div class="flex items-center gap-2 text-xs text-muted-foreground">
      <Loader2 size={14} class="animate-spin" />
      {$t("permissions.preview.loading")}
    </div>
  {/if}

  {#if error}
    <PermissionErrorBanner {error} />
    <Button size="sm" variant="outline" onclick={load}>{$t("common.retry")}</Button>
  {/if}

  {#if payload}
    {#if payload.kind === "filesystem"}
      <PreviewFilesystem {payload} />
    {:else if payload.kind === "bash"}
      <PreviewBash {payload} />
    {:else if payload.kind === "mcp"}
      <PreviewMcp {payload} />
    {:else if payload.kind === "network"}
      <PreviewNetwork {payload} />
    {:else}
      <pre class="overflow-auto rounded bg-muted px-3 py-2 text-[11px]">{JSON.stringify(payload, null, 2)}</pre>
    {/if}
  {/if}
</div>
