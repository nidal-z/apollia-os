<script lang="ts">
  /**
   * CopyValue - a monospace value paired with a one-click copy button.
   *
   * The workhorse of the About page's key/value rows: it renders the value
   * (truncated with a full-text tooltip), copies it to the clipboard on click,
   * swaps the glyph to a check for ~1.5s, and confirms with a toast. Copy
   * failures route through the shared humanized-error reporter.
   */
  import { t } from "svelte-i18n";
  import { Copy, Check } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { addToast } from "$lib/components/ui/toast";
  import { reportError } from "$lib/errors/reportError";

  interface Props {
    value: string;
    /** Human label of the field, used to build the copy button's aria-label. */
    label: string;
    mono?: boolean;
    "data-testid"?: string;
  }

  let { value, label, mono = true, "data-testid": testid }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(value);
      copied = true;
      addToast($t("settings.about.copy_success"), "success");
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 1500);
    } catch (err) {
      reportError(err, { surface: "toast" });
    }
  }
</script>

<span class="inline-flex min-w-0 items-center gap-2">
  <span
    class="min-w-0 truncate text-body-sm text-foreground {mono ? 'font-mono' : ''}"
    title={value}
  >
    {value}
  </span>
  <Button
    variant="ghost"
    size="icon-sm"
    onclick={copy}
    aria-label={copied
      ? $t("settings.about.copied")
      : $t("settings.about.copy_field", { values: { field: label } })}
    data-testid={testid}
  >
    {#if copied}
      <Check size={13} strokeWidth={2.2} class="text-success" aria-hidden="true" />
    {:else}
      <Copy size={13} strokeWidth={1.9} aria-hidden="true" />
    {/if}
  </Button>
</span>
