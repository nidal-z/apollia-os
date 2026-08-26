<!--
  What a conversation shows when its history will not load.

  Two ways out, and both belong here rather than in the conversation: dump
  whatever the backend can still surface into a JSON envelope the operator can
  forward, or drop the session.
-->
<script lang="ts">
  import { t } from "svelte-i18n";
  import { AlertOctagon } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { getChatSession } from "$lib/ipc/chat";

  interface Props {
    sessionId: string;
    /** The message the failed load produced; carried into the envelope. */
    loadError: string | null;
    ondelete: () => void;
  }

  let { sessionId, loadError, ondelete }: Props = $props();

  async function exportRaw(): Promise<void> {
    // The UX contract is "give me something to send to support". We dump
    // whatever the backend was able to surface - even if only the load
    // error - into a JSON envelope so the user can forward it. The session
    // detail is the only stored form the backend exposes; when the load that
    // set `loadError` fails again here, the envelope carries the error alone.
    let raw: unknown = null;
    try {
      raw = await getChatSession(sessionId);
    } catch {
      raw = null;
    }
    const envelope = {
      session_id: sessionId,
      captured_at: new Date().toISOString(),
      load_error: loadError,
      raw,
    };
    try {
      // Browser-native download - works inside the Tauri webview without
      // pulling an extra plugin dependency. The user picks the destination
      // via the OS "Save as" dialog emitted by the anchor click.
      const blob = new Blob([JSON.stringify(envelope, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `apollia-session-${sessionId.slice(0, 8)}-raw.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.warn("export corrupted session failed", err);
    }
  }
</script>

<div
  class="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-8 text-center"
  role="alert"
  aria-live="assertive"
  data-testid="session-corrupted"
>
  <div
    class="flex h-12 w-12 items-center justify-center rounded-full bg-destructive/15 text-destructive"
    aria-hidden="true"
  >
    <AlertOctagon size={22} />
  </div>
  <div class="max-w-md">
    <p class="text-sm font-medium text-destructive">
      {$t("chat.session_corrupted.title")}
    </p>
    <p class="mt-1 text-xs text-muted-foreground">
      {$t("chat.session_corrupted.description")}
    </p>
    <p class="mt-2 font-mono text-micro text-muted-foreground/60">
      {loadError}
    </p>
  </div>
  <div class="flex gap-2">
    <Button
      size="sm"
      variant="outline"
      onclick={exportRaw}
      data-testid="session-corrupted-export"
    >
      {$t("chat.session_corrupted.export_raw")}
    </Button>
    <Button
      size="sm"
      variant="destructive"
      onclick={ondelete}
      data-testid="session-corrupted-delete"
    >
      {$t("chat.session_corrupted.delete")}
    </Button>
  </div>
</div>
