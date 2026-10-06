<script lang="ts">
  /**
   * Why a package cannot be installed, said in plain words: an invalid
   * signature, the organisation policy, an older version, or an agent that
   * already exists. The publisher stays visible and the detail can be copied
   * to send to whoever provided the file.
   */
  import { t } from "svelte-i18n";
  import { Copy } from "lucide-svelte";
  import { Button } from "$lib/components/ui/button";
  import { ErrorBanner } from "$lib/components/operator";
  import type { PackagePreview } from "$lib/ipc/packageFiles";
  import { refusalOf } from "./packagePreviewState";
  import PackagePublisher from "./PackagePublisher.svelte";

  interface Props {
    preview: PackagePreview;
    oncopy: (reason: string) => void;
  }

  let { preview, oncopy }: Props = $props();

  const refusal = $derived(refusalOf(preview));
  const code = $derived(refusal?.code ?? "invalid_signature");
  const reason = $derived(
    refusal?.code === "invalid_signature"
      ? refusal.reason
      : (preview.signature.reason ?? code),
  );
  const values = $derived.by((): Record<string, string> => {
    if (refusal?.code === "downgrade") return { installed: refusal.installed, incoming: refusal.incoming };
    if (refusal?.code === "agent_taken") return { agent: refusal.agent, owner: refusal.owner ?? "" };
    return {};
  });
  const bodyKey = $derived(
    refusal?.code === "agent_taken" && refusal.owner ? "agent_taken_owned" : code,
  );
</script>

<div class="flex flex-col gap-4" data-testid="package-preview-refusal" data-code={code}>
  <ErrorBanner tone="danger" data-testid="package-preview-refusal-reason">
    <p class="font-medium">{$t(`packages.preview.refusal.${code}.title`)}</p>
    <p class="opacity-90">{$t(`packages.preview.refusal.${bodyKey}.body`, { values })}</p>
  </ErrorBanner>
  <PackagePublisher {preview} allowTrust={false} />
  <div>
    <Button variant="outline" size="sm" onclick={() => oncopy(reason)} data-testid="package-preview-copy-detail">
      <Copy size={13} />
      {$t("packages.preview.action.copy_detail")}
    </Button>
  </div>
</div>
