<script lang="ts">
  /**
   * AcceleratedEngineCard - the one card at the head of the AI screen that
   * carries the accelerated engine from end to end: proposing it, verifying
   * its signature, downloading it, confirming it, refusing it, updating it.
   *
   * It shows what `cardView` decided and sends each gesture back; it holds
   * no state of its own. The developer mode adds the build number after the
   * day of a version, and the sentence of the runtime under a refusal.
   */
  import { locale, t } from "svelte-i18n";
  import { CircleAlert, CircleCheck, Zap } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ProgressBar } from "$lib/components/ui/progress";
  import { developerMode } from "$lib/stores/developerMode";
  import { cn } from "$lib/utils";
  import {
    PROGRESS_BODY_KEYS,
    REFUSAL_BODY_KEYS,
    REFUSAL_CAUSE_KEYS,
    REFUSAL_TITLE_KEYS,
    megabytes,
    releaseDay,
    type CardView,
    type EngineVersion,
  } from "./acceleratedEngine";
  import EngineStep from "./EngineStep.svelte";

  interface Props {
    view: CardView;
    /** A gesture is being sent: the buttons wait. */
    busy: boolean;
    onaccelerate: () => void;
    onimport: () => void;
    onlater: () => void;
    oncancel: () => void;
    onclose: () => void;
    onreload: () => void;
  }

  let {
    view,
    busy,
    onaccelerate,
    onimport,
    onlater,
    oncancel,
    onclose,
    onreload,
  }: Props = $props();

  const lang = $derived($locale ?? "fr");
  const tone = $derived(
    view.status === "refused" ? "danger" : view.status === "installed" ? "success" : "neutral",
  );

  function size(bytes: number): string {
    return $t("ai.engine.megabytes", { values: { n: megabytes(bytes, lang) } });
  }

  /** A build by its day, with its number in developer mode; by its number when the day is unknown. */
  function version(v: EngineVersion): string {
    const day = releaseDay(v.released, lang);
    if (day === null) return $t("ai.engine.version_number", { values: { tag: v.tag } });
    const dated = $t("ai.engine.version_day", { values: { day } });
    return $developerMode ? `${dated} (${v.tag})` : dated;
  }

  function eta(seconds: number | null): string | null {
    if (seconds === null) return null;
    if (seconds < 60) return $t("ai.engine.eta.under_minute");
    return $t("ai.engine.eta.minutes", { values: { n: Math.round(seconds / 60) } });
  }

  const title = $derived.by(() => {
    switch (view.status) {
      case "proposal":
        return $t("ai.engine.proposal.title");
      case "update_available":
        return $t("ai.engine.update.title");
      case "verifying":
      case "downloading":
      case "installing":
        if (view.flow === "import") return $t("ai.engine.progress.import_title");
        if (view.flow === "install") return $t("ai.engine.progress.install_title");
        return "latest" in view && view.latest
          ? $t("ai.engine.progress.update_title_version", {
              values: { version: version(view.latest) },
            })
          : $t("ai.engine.progress.update_title");
      case "installed":
        return $t(
          view.flow === "update" ? "ai.engine.installed.update_title" : "ai.engine.installed.title",
        );
      case "refused":
        return $t(REFUSAL_TITLE_KEYS[view.flow], {
          values: { cause: $t(REFUSAL_CAUSE_KEYS[view.reason]) },
        });
    }
  });

  const body = $derived.by(() => {
    switch (view.status) {
      case "proposal":
        return view.card
          ? $t("ai.engine.proposal.body", { values: { card: view.card } })
          : $t("ai.engine.proposal.body_unnamed");
      case "update_available":
        return view.installed
          ? $t("ai.engine.update.body", {
              values: { latest: version(view.latest), installed: version(view.installed) },
            })
          : $t("ai.engine.update.body_latest", { values: { latest: version(view.latest) } });
      case "verifying":
      case "downloading":
      case "installing":
        return $t(PROGRESS_BODY_KEYS[view.flow]);
      case "installed":
        if (view.loadedCount > 0) return $t("ai.engine.installed.body_loaded");
        return $t(
          view.flow === "update" ? "ai.engine.installed.update_body" : "ai.engine.installed.body",
        );
      case "refused": {
        const parts = [$t(REFUSAL_BODY_KEYS[view.reason])];
        if (view.flow === "import") parts.push($t("ai.engine.refusal.nothing_installed"));
        else if (view.nothingDownloaded) parts.unshift($t("ai.engine.refusal.nothing_downloaded"));
        else parts.push($t("ai.engine.refusal.nothing_installed"));
        if (view.flow === "update") parts.push($t("ai.engine.refusal.current_kept"));
        return parts.join(" ");
      }
    }
  });

  const weight = $derived.by(() => {
    if (view.status !== "proposal" && view.status !== "update_available") return null;
    return view.sizeBytes === null
      ? $t("ai.engine.signature_promise")
      : $t("ai.engine.download_weight", { values: { size: size(view.sizeBytes) } });
  });

  /** Whether the refusal came from the signature step, which then shows as refused. */
  const signatureRefused = $derived(
    view.status === "refused" &&
      view.flow !== "import" &&
      view.nothingDownloaded &&
      (view.reason === "list_altered" ||
        view.reason === "unknown_key" ||
        view.reason === "older_version"),
  );
</script>

<section
  class={cn(
    "space-y-3 rounded-lg border bg-card p-4",
    tone === "danger" ? "border-destructive" : "border-border",
  )}
  aria-live="polite"
  data-testid="ai-engine-card"
  data-status={view.status}
  data-flow={"flow" in view ? view.flow : undefined}
  data-reason={view.status === "refused" ? view.reason : undefined}
>
  <div class="flex flex-wrap items-start gap-3">
    <span
      class="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-muted"
      aria-hidden="true"
    >
      {#if tone === "danger"}
        <CircleAlert class="h-5 w-5 text-danger-a11y" />
      {:else if tone === "success"}
        <CircleCheck class="h-5 w-5 text-success-a11y" />
      {:else}
        <Zap class="h-5 w-5 text-foreground" aria-hidden="true" />
      {/if}
    </span>
    <div class="min-w-0 flex-1 space-y-1">
      <h2 class="text-sm font-semibold text-foreground" data-testid="ai-engine-title">{title}</h2>
      <p class="text-sm text-muted-foreground" data-testid="ai-engine-body">{body}</p>
      {#if weight}
        <p class="text-xs text-muted-foreground" data-testid="ai-engine-weight">{weight}</p>
      {/if}
      {#if view.status === "refused" && $developerMode}
        <p class="break-words font-mono text-xs text-muted-foreground" data-testid="ai-engine-detail">
          {$t("dev.ai.engine.refusal_detail", { values: { detail: view.detail } })}
        </p>
      {/if}
    </div>
    {#if view.status === "downloading"}
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        onclick={oncancel}
        data-testid="ai-engine-cancel"
      >
        {$t("ai.engine.action.cancel")}
      </Button>
    {/if}
  </div>

  {#if view.status === "verifying" && view.flow !== "import"}
    <ul class="space-y-2">
      <EngineStep
        status="running"
        label={$t("ai.engine.step.signature")}
        testid="ai-engine-step-signature"
      />
      <EngineStep
        status="pending"
        label={$t("ai.engine.step.download")}
        detail={view.sizeBytes === null
          ? $t("ai.engine.step.after_verification")
          : $t("ai.engine.step.download_pending", { values: { size: size(view.sizeBytes) } })}
        testid="ai-engine-step-download"
      />
    </ul>
  {:else if view.status === "downloading"}
    {@const remaining = eta(view.etaSeconds)}
    {@const quantities = $t("ai.engine.step.download_progress", {
      values: { received: size(view.received), total: size(view.total) },
    })}
    <ul class="space-y-2">
      <EngineStep
        status="done"
        label={$t("ai.engine.step.signature_done")}
        detail={$t("ai.engine.step.signature_key")}
        testid="ai-engine-step-signature"
      />
      <EngineStep
        status="running"
        label={$t("ai.engine.step.download")}
        detail={remaining ? `${quantities}, ${remaining}` : quantities}
        testid="ai-engine-step-download"
      />
    </ul>
    <ProgressBar
      size="sm"
      value={view.total > 0 ? (view.received / view.total) * 100 : undefined}
    />
  {:else if view.status === "verifying" || view.status === "installing"}
    <ul class="space-y-2">
      <EngineStep
        status={view.status === "verifying" ? "running" : "done"}
        label={$t(
          view.status === "verifying" ? "ai.engine.step.signature" : "ai.engine.step.signature_done",
        )}
        detail={view.status === "installing" ? $t("ai.engine.step.signature_key") : null}
        testid="ai-engine-step-signature"
      />
      <EngineStep
        status={view.status === "verifying" ? "pending" : "running"}
        label={$t("ai.engine.step.installation")}
        detail={view.status === "verifying" ? $t("ai.engine.step.after_verification") : null}
        testid="ai-engine-step-installation"
      />
    </ul>
  {:else if signatureRefused}
    <ul class="space-y-2">
      <EngineStep
        status="refused"
        label={$t("ai.engine.step.signature")}
        detail={$t("ai.engine.step.refused")}
        testid="ai-engine-step-signature"
      />
      <EngineStep
        status="pending"
        label={$t("ai.engine.step.download")}
        detail={$t("ai.engine.step.not_started")}
        testid="ai-engine-step-download"
      />
    </ul>
  {/if}

  {#if view.status === "proposal" || view.status === "update_available"}
    <div class="flex flex-wrap items-center gap-2">
      <Button variant="ghost" size="sm" onclick={onlater} data-testid="ai-engine-later">
        {$t("ai.engine.action.later")}
      </Button>
      <span class="flex-1"></span>
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        onclick={onimport}
        data-testid="ai-engine-import"
      >
        {$t("ai.engine.action.import")}
      </Button>
      <Button
        variant="primary-solid"
        size="sm"
        disabled={busy}
        onclick={onaccelerate}
        data-testid="ai-engine-start"
      >
        {$t(
          view.status === "proposal" ? "ai.engine.action.accelerate" : "ai.engine.action.update",
        )}
      </Button>
    </div>
  {:else if view.status === "installed" && view.loadedCount > 0}
    <div class="flex flex-wrap items-center justify-end gap-2">
      <Button
        variant="primary-solid"
        size="sm"
        loading={busy}
        disabled={busy}
        onclick={onreload}
        data-testid="ai-engine-reload"
      >
        {$t("ai.engine.action.reload")}
      </Button>
    </div>
  {:else if view.status === "refused"}
    <div class="flex flex-wrap items-center justify-end gap-2">
      <Button
        variant="outline"
        size="sm"
        disabled={busy}
        onclick={onclose}
        data-testid="ai-engine-close"
      >
        {$t("ai.engine.action.close")}
      </Button>
      {#if view.flow === "import"}
        <Button
          variant="primary-solid"
          size="sm"
          disabled={busy}
          onclick={onimport}
          data-testid="ai-engine-other-folder"
        >
          {$t("ai.engine.action.other_folder")}
        </Button>
      {/if}
    </div>
  {/if}
</section>
