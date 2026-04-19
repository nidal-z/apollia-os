<script lang="ts">
  /**
   * SessionNotFound — shown when a `/chat/:id` selection points at a
   * session id that no longer exists (US-SP42-034, B.28).
   *
   * Offers two paths forward: back to the session list, or start a new
   * chat. Fails fast but gracefully — principe 4.
   */
  import { t } from "svelte-i18n";
  import { FileQuestion } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** Missing session id — surfaced for debugging / support. */
    sessionId?: string;
    /** Back to the session list (clears selection). */
    onback: () => void;
    /** Opens the QuickPicker for a fresh chat. */
    onnewChat: () => void;
  }

  let { sessionId, onback, onnewChat }: Props = $props();
</script>

<section
  class="flex h-full flex-col items-center justify-center gap-4 px-6 py-12 text-center"
  role="alert"
  aria-live="polite"
  data-testid="session-not-found"
>
  <div
    class="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted/40 text-muted-foreground"
    aria-hidden="true"
  >
    <FileQuestion size={28} strokeWidth={1.5} />
  </div>

  <div class="max-w-md space-y-1">
    <h2 class="text-display-sm text-foreground">
      {$t("chat.session_not_found.title")}
    </h2>
    <p class="text-sm text-muted-foreground">
      {$t("chat.session_not_found.description")}
    </p>
    {#if sessionId}
      <p class="mt-2 font-mono text-[10px] text-muted-foreground/60">
        {sessionId}
      </p>
    {/if}
  </div>

  <div class="mt-1 flex flex-col items-stretch gap-2 sm:flex-row sm:items-center">
    <Button
      variant="primary-gradient"
      onclick={onback}
      data-testid="session-not-found-back"
      class="min-w-[10rem]"
    >
      {$t("chat.session_not_found.back")}
    </Button>
    <Button
      variant="ghost"
      onclick={onnewChat}
      data-testid="session-not-found-new"
    >
      {$t("chat.session_not_found.new_chat")}
    </Button>
  </div>
</section>
