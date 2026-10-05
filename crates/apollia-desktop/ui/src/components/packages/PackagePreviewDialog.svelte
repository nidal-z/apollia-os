<script lang="ts">
  /**
   * The preview of a `.apo` package before it is installed or updated.
   *
   * Mounted once above every screen and opened by `openPackagePreview(path)`,
   * or by `openExamplePreview(name)` for an example the application carries.
   * It reads the package through the runtime, nothing written, and shows what
   * it brings, the accesses it asks for by family, its publisher and
   * signature, and the connections it requires or brings. An unsigned package
   * warns; an invalid signature or the organisation policy refuses it, readably;
   * an update puts forward what it adds, to be accepted before updating.
   * An example is not signed and is not warned about for it: the application
   * that carries it vouches for it.
   */
  import { t } from "svelte-i18n";
  import { Package } from "lucide-svelte";
  import { Dialog } from "$lib/components/ui/dialog";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import { addToast } from "$lib/components/ui/toast/store";
  import { requestConnectionAdd } from "$lib/stores/connectionAdd";
  import { navigateTo } from "$lib/stores/navigation";
  import {
    PACKAGE_INSTALLED_EVENT,
    closePackagePreview,
    packagePreviewRequest,
    type PackagePreviewRequest,
  } from "$lib/stores/packagePreview";
  import { installExamplePackage, previewExamplePackage } from "$lib/ipc/packageExamples";
  import {
    installPackageFile,
    previewPackageFile,
    trustPackagePublisher,
    type PackagePreview,
  } from "$lib/ipc/packageFiles";
  import {
    canProceed,
    changesToAccept,
    fileName,
    installConfirmations,
    previewVariant,
    refusalDetail,
  } from "./packagePreviewState";
  import PackagePublisher from "./PackagePublisher.svelte";
  import PackageContent from "./PackageContent.svelte";
  import PackageAccess from "./PackageAccess.svelte";
  import PackageConnections from "./PackageConnections.svelte";
  import PackageUpdateChanges from "./PackageUpdateChanges.svelte";
  import PackageRefusal from "./PackageRefusal.svelte";

  const request = $derived($packagePreviewRequest);
  const shipped = $derived(request?.example !== undefined);

  let preview = $state<PackagePreview | null>(null);
  let loadError = $state<string | null>(null);
  let installError = $state<string | null>(null);
  let installing = $state(false);
  let trusting = $state(false);
  let accepted = $state<Set<string>>(new Set());

  async function load(asked: PackagePreviewRequest): Promise<void> {
    loadError = null;
    try {
      preview =
        asked.example !== undefined
          ? await previewExamplePackage(asked.example)
          : await previewPackageFile(asked.path);
    } catch (err) {
      preview = null;
      loadError = String(err);
    }
  }

  $effect(() => {
    const asked = request;
    preview = null;
    loadError = null;
    installError = null;
    accepted = new Set();
    if (asked !== null) void load(asked);
  });

  const variant = $derived(preview ? previewVariant(preview, shipped) : null);
  const changes = $derived(preview ? changesToAccept(preview) : []);
  const refused = $derived(variant === "invalid" || variant === "policy_refused" || variant === "refused");
  const enabled = $derived(preview !== null && !installing && canProceed(preview, accepted));

  const title = $derived.by(() => {
    if (!preview) return $t("packages.preview.loading_title");
    if (refused) return $t("packages.preview.title_refused");
    const key = variant === "update" ? "title_update" : "title_install";
    return $t(`packages.preview.${key}`, { values: { name: preview.name } });
  });

  const actionLabel = $derived(
    variant === "update"
      ? $t("packages.preview.action.update")
      : variant === "unsigned"
        ? $t("packages.preview.action.install_anyway")
        : $t("packages.preview.action.install"),
  );

  function toggle(key: string, checked: boolean): void {
    const next = new Set(accepted);
    if (checked) next.add(key);
    else next.delete(key);
    accepted = next;
  }

  function addConnection(name: string): void {
    closePackagePreview();
    requestConnectionAdd(name);
  }

  async function trust(): Promise<void> {
    if (!preview?.signature.publicKey || !request) return;
    trusting = true;
    try {
      const label = preview.signature.declaredPublisher || preview.signature.publisher;
      await trustPackagePublisher(preview.signature.publicKey, label);
      addToast($t("packages.preview.publisher.trusted_toast", { values: { name: label } }), "success");
      await load(request);
    } catch (err) {
      installError = String(err);
    } finally {
      trusting = false;
    }
  }

  async function install(): Promise<void> {
    if (!preview || !request || !enabled) return;
    installing = true;
    installError = null;
    const { depsConfirmed, changesConfirmed } = installConfirmations(preview, accepted);
    try {
      const report =
        request.example !== undefined
          ? await installExamplePackage(request.example, depsConfirmed, changesConfirmed)
          : await installPackageFile(request.path, depsConfirmed, changesConfirmed);
      const key = variant === "update" ? "updated_toast" : "installed_toast";
      addToast(
        $t(`packages.preview.${key}`, { values: { name: preview.name, version: report.version ?? preview.version } }),
        "success",
      );
      window.dispatchEvent(new CustomEvent(PACKAGE_INSTALLED_EVENT, { detail: { name: preview.name } }));
      closePackagePreview();
    } catch (err) {
      installError = String(err);
    } finally {
      installing = false;
    }
  }

  async function copyDetail(reason: string): Promise<void> {
    if (!preview || !request) return;
    try {
      await navigator.clipboard.writeText(refusalDetail(preview, request.path, reason));
      addToast($t("packages.preview.action.copied"), "success");
    } catch (err) {
      installError = String(err);
    }
  }

  function seePolicy(): void {
    closePackagePreview();
    navigateTo("authorizations");
  }
</script>

<Dialog
  open={request !== null}
  onclose={closePackagePreview}
  size={refused ? "md" : "xl"}
  label={title}
  data-testid="package-preview-dialog"
>
  <div class="flex flex-col gap-4" data-variant={variant ?? "loading"}>
    <header class="flex items-start gap-3">
      <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-primary/10 text-primary">
        <Package size={20} />
      </div>
      <div class="min-w-0 flex-1">
        <h2 class="text-heading-lg font-semibold text-foreground" data-testid="package-preview-title">{title}</h2>
        {#if request}
          <p class="truncate text-caption text-muted-foreground" data-testid="package-preview-origin">
            {#if !shipped}{fileName(request.path)}{/if}
            {#if preview}
              {#if !shipped}·{/if}
              {preview.update
                ? $t("packages.preview.version_change", {
                    values: { from: preview.update.fromVersion, to: preview.update.toVersion },
                  })
                : $t("packages.preview.version", { values: { version: preview.version } })}
            {/if}
            · {$t(`packages.preview.origin_${request.origin}`)}
          </p>
        {/if}
        {#if preview?.description && !refused}
          <p class="mt-1 text-body-sm text-foreground">{preview.description}</p>
        {/if}
      </div>
    </header>

    {#if loadError}
      <ErrorBanner tone="danger" data-testid="package-preview-load-error">
        <p class="font-medium">{$t("packages.preview.load_error")}</p>
        <p class="break-all opacity-80">{loadError}</p>
      </ErrorBanner>
    {:else if !preview}
      <p class="text-caption text-muted-foreground" data-testid="package-preview-loading">
        {$t("packages.preview.loading")}
      </p>
    {:else if refused}
      <PackageRefusal {preview} oncopy={copyDetail} />
    {:else}
      {#if variant === "unsigned" || (variant === "update" && preview.signature.status === "unsigned" && !shipped)}
        <ErrorBanner tone="warning" data-testid="package-preview-unsigned-warning">
          <p class="font-medium">{$t("packages.preview.unsigned.title")}</p>
          <p class="opacity-90">{$t("packages.preview.unsigned.body")}</p>
        </ErrorBanner>
      {/if}
      {#if variant === "update"}
        <PackageUpdateChanges {changes} {accepted} ontoggle={toggle} onadd={addConnection} />
      {/if}
      <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
        <div class="flex flex-col gap-4">
          <PackagePublisher {preview} {shipped} {trusting} ontrust={trust} />
          <PackageContent {preview} />
        </div>
        <div class="flex flex-col gap-4">
          <PackageAccess {preview} />
          <PackageConnections {preview} onadd={addConnection} />
        </div>
      </div>
    {/if}

    {#if installError}
      <ErrorBanner tone="danger" data-testid="package-preview-install-error">
        <p class="font-medium">{$t("packages.preview.install_error")}</p>
        <p class="break-all opacity-80">{installError}</p>
      </ErrorBanner>
    {/if}

    <footer class="flex flex-wrap items-center justify-end gap-2 border-t border-border pt-3">
      {#if preview && !refused}
        <p class="mr-auto text-caption text-muted-foreground">{$t("packages.preview.nothing_before_consent")}</p>
      {/if}
      {#if variant === "policy_refused"}
        <Button variant="outline" size="sm" onclick={seePolicy} data-testid="package-preview-see-policy">
          {$t("packages.preview.policy.see_policy")}
        </Button>
      {/if}
      <Button variant="outline" size="sm" onclick={closePackagePreview} data-testid="package-preview-cancel">
        {refused || loadError ? $t("common.close") : $t("common.cancel")}
      </Button>
      {#if preview && !refused}
        <Button
          variant="primary-solid"
          size="sm"
          onclick={install}
          disabled={!enabled}
          data-testid="package-preview-install"
        >
          {installing ? $t("packages.preview.action.installing") : actionLabel}
        </Button>
      {/if}
    </footer>
  </div>
</Dialog>
