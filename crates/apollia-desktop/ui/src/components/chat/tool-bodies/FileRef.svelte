<script lang="ts">
  /**
   * Clickable file reference shared by the file_* tool bodies.
   *
   * Every path reveals the file in the OS file manager on click. Absolute paths
   * (POSIX root, `~`, or a Windows drive) resolve directly through the opener
   * helper. Relative paths resolve against the chat session's working directory
   * on the backend: the `reveal_session_path` command mirrors the sandbox root
   * the agent used for its file tools, so `sessionId` is threaded down from the
   * message bubble. When neither an absolute path nor a `sessionId` is
   * available (e.g. a live-stream context, which renders no file bodies), the
   * ref falls back to a styled but non-interactive span rather than a dead
   * click.
   */

  import { t } from "svelte-i18n";
  import { invoke } from "@tauri-apps/api/core";
  import { revealPath, isRevealablePath } from "$lib/utils/fileRef";

  interface Props {
    /** Raw path used for the reveal call and the `title` tooltip. */
    path: string;
    /** Display text; defaults to the raw path. */
    label?: string;
    /** Render the label in the mono type ramp. */
    mono?: boolean;
    /** Extra classes forwarded to the label element. */
    class?: string;
    /**
     * Chat session id, used to resolve a relative path against the session
     * working directory. Optional: live-stream contexts render no file bodies,
     * so a missing id there only affects relative paths (rendered static).
     */
    sessionId?: string;
  }

  let {
    path,
    label,
    mono = false,
    class: cls = "",
    sessionId,
  }: Props = $props();

  const display = $derived(label ?? path);
  const revealable = $derived(isRevealablePath(path));
  // Interactive when the path is absolute (resolves without a working dir) or
  // when a session id lets the backend resolve a relative path.
  const clickable = $derived(revealable || sessionId !== undefined);

  /** True when running inside the Tauri runtime (vs. plain browser dev). */
  function isTauri(): boolean {
    return globalThis.window !== undefined && "__TAURI_INTERNALS__" in globalThis.window;
  }

  async function reveal(): Promise<void> {
    // Absolute paths resolve without a working directory: route them through
    // the shared opener helper. Relative paths need the session workspace, so
    // hand off to the backend command that mirrors the agent's sandbox root.
    if (revealable) {
      await revealPath(path);
      return;
    }
    if (sessionId === undefined) return;
    try {
      if (isTauri()) {
        await invoke("reveal_session_path", { sessionId, path });
      } else {
        console.info("[fileRef] reveal relative (dev no-op)", path);
      }
    } catch (err) {
      console.error("[fileRef] failed to reveal relative path", path, err);
    }
  }
</script>

{#if clickable}
  <button
    type="button"
    class="tb-fileref {mono ? 'font-mono' : ''} {cls}"
    title={path}
    aria-label={$t("tools.body.reveal_aria")}
    onclick={(e) => {
      e.stopPropagation();
      void reveal();
    }}
  >{display}</button>
{:else}
  <span class="tb-fileref tb-fileref-static {mono ? 'font-mono' : ''} {cls}" title={path}
    >{display}</span
  >
{/if}
