<script lang="ts">
  /**
   * Clickable file reference shared by the file_* tool bodies.
   *
   * Absolute paths (POSIX root, `~`, or a Windows drive) render as a keyboard-
   * accessible button that reveals the file in the OS file manager. Relative
   * paths render as a styled but non-interactive ref (never a dead click): the
   * OS reveal needs an absolute location, and the chat session does not thread a
   * working directory down to this component yet. Making relative paths
   * clickable would mean resolving `workingDir + "/" + path`, which requires
   * threading the session/project workspace path (ChatSessionDetail carries only
   * `project_id`; workspace_path lives on the project) through
   * ChatMessageBubble -> ReasoningCard -> body.
   */

  import { t } from "svelte-i18n";
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
  }

  let { path, label, mono = false, class: cls = "" }: Props = $props();

  const display = $derived(label ?? path);
  const revealable = $derived(isRevealablePath(path));
</script>

{#if revealable}
  <button
    type="button"
    class="tb-fileref {mono ? 'font-mono' : ''} {cls}"
    title={path}
    aria-label={$t("tools.body.reveal_aria")}
    onclick={(e) => {
      e.stopPropagation();
      void revealPath(path);
    }}
  >{display}</button>
{:else}
  <span class="tb-fileref tb-fileref-static {mono ? 'font-mono' : ''} {cls}" title={path}
    >{display}</span
  >
{/if}
