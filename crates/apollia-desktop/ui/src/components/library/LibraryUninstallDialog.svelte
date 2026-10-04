<script lang="ts">
  /**
   * The uninstallation of a package, asked before anything is removed: what
   * goes with it (its agents, workflows and tools, the automations that start
   * them, the connections it brought), what stays (the connections it only
   * required), and whether the memory and the history of its agents are kept,
   * the default, or erased.
   */
  import { t } from "svelte-i18n";
  import { Dialog, DialogFooter } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { RadioGroup, RadioItem } from "$lib/components/ui/radio";
  import { ErrorBanner } from "$lib/components/operator";
  import {
    whatGoes,
    whatStays,
    type DataChoice,
    type LibraryDetail,
  } from "$lib/library/libraryState";

  interface Props {
    /** The package to uninstall; `null` closes the dialog. */
    detail: LibraryDetail | null;
    busy?: boolean;
    error?: string | null;
    oncancel: () => void;
    onconfirm: (choice: DataChoice) => void;
  }

  let { detail, busy = false, error = null, oncancel, onconfirm }: Props = $props();

  let choice = $state<DataChoice>("keep");

  // Each opening starts from the default: keeping is never a leftover choice.
  $effect(() => {
    if (detail !== null) choice = "keep";
  });

  const leaving = $derived(detail ? whatGoes(detail) : []);
  const staying = $derived(detail ? whatStays(detail) : []);
  const title = $derived(detail ? $t("library.uninstall.title", { values: { name: detail.key } }) : "");
</script>

<Dialog open={detail !== null} onclose={oncancel} size="md" label={title} role="alertdialog" data-testid="library-uninstall-dialog">
  {#if detail}
    <div class="flex flex-col gap-5">
      <h2 class="text-heading-lg font-semibold text-foreground" data-testid="library-uninstall-title">{title}</h2>

      <section class="flex flex-col gap-2" data-testid="library-uninstall-leaving">
        <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
          {$t("library.uninstall.leaving_title")}
        </h3>
        <ul class="flex flex-col gap-1.5">
          {#each leaving as line (line.kind)}
            <li class="text-body-sm text-foreground" data-testid="library-uninstall-leaving-{line.kind}">
              <span class="font-medium">{$t(`library.uninstall.leaving.${line.kind}`, { values: { count: line.names.length } })}</span><span class="text-muted-foreground">{$t("library.uninstall.names", { values: { names: line.names.join(", ") } })}</span>
            </li>
          {/each}
        </ul>
      </section>

      {#if staying.length > 0}
        <section class="flex flex-col gap-2" data-testid="library-uninstall-staying">
          <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
            {$t("library.uninstall.staying_title")}
          </h3>
          <p class="text-body-sm text-foreground">
            <span class="font-medium">{$t("library.uninstall.staying_connections", { values: { count: staying.length } })}</span><span class="text-muted-foreground">{$t("library.uninstall.names", { values: { names: staying.join(", ") } })}</span>
          </p>
        </section>
      {/if}

      <section class="flex flex-col gap-2">
        <h3 class="text-caption font-medium uppercase tracking-wide text-muted-foreground">
          {$t("library.uninstall.data_title")}
        </h3>
        <RadioGroup value={choice} data-testid="library-uninstall-choice">
          <RadioItem
            value="keep"
            checked={choice === "keep"}
            onchange={() => (choice = "keep")}
            disabled={busy}
            data-testid="library-uninstall-keep"
          >
            <span class="block font-medium text-foreground">{$t("library.uninstall.keep_title")}</span>
            <span class="block text-caption text-muted-foreground">{$t("library.uninstall.keep_body")}</span>
          </RadioItem>
          <RadioItem
            value="erase"
            checked={choice === "erase"}
            onchange={() => (choice = "erase")}
            disabled={busy}
            data-testid="library-uninstall-erase"
          >
            <span class="block font-medium text-foreground">{$t("library.uninstall.erase_title")}</span>
            <span class="block text-caption {choice === 'erase' ? 'text-danger-a11y' : 'text-muted-foreground'}">
              {$t("library.uninstall.erase_body")}
            </span>
          </RadioItem>
        </RadioGroup>
      </section>

      {#if error}
        <ErrorBanner tone="danger" data-testid="library-uninstall-error">
          <p class="font-medium">{$t("library.uninstall.error")}</p>
          <p class="break-all opacity-80">{error}</p>
        </ErrorBanner>
      {/if}
    </div>

    <DialogFooter>
      <Button variant="outline" size="sm" onclick={oncancel} disabled={busy} data-testid="library-uninstall-cancel">
        {$t("common.cancel")}
      </Button>
      <Button
        variant="destructive"
        size="sm"
        onclick={() => onconfirm(choice)}
        loading={busy}
        disabled={busy}
        data-testid="library-uninstall-confirm"
      >
        {choice === "erase" ? $t("library.uninstall.confirm_erase") : $t("library.uninstall.confirm")}
      </Button>
    </DialogFooter>
  {/if}
</Dialog>
