<script lang="ts">
  import { t } from "svelte-i18n";
  import { FolderOpen } from "lucide-svelte";
  import { open as openFilePicker } from "@tauri-apps/plugin-dialog";
  import { Input } from "$lib/components/ui/input";
  import { Button } from "$lib/components/ui/button";
  import type { RegistryPackageArgView } from "$lib/types";

  interface Props {
    args: RegistryPackageArgView[];
    values: Record<string, string>;
    onchange: (key: string, value: string) => void;
  }

  let { args, values, onchange }: Props = $props();

  function argKey(arg: RegistryPackageArgView, index: number): string {
    return arg.value_hint ?? String(index);
  }

  function isPathLike(valueHint: string | null): boolean {
    if (!valueHint) return false;
    const h = valueHint.toLowerCase();
    return (
      h.includes("path") ||
      h.includes("file") ||
      h.includes("dir") ||
      h.includes("folder") ||
      h.startsWith("/") ||
      h.startsWith("~")
    );
  }

  async function pickPath(key: string, valueHint: string | null): Promise<void> {
    const isDir =
      valueHint !== null &&
      (valueHint.toLowerCase().includes("dir") || valueHint.toLowerCase().includes("folder"));

    const selected = await openFilePicker({ directory: isDir, multiple: false });
    if (typeof selected === "string") {
      onchange(key, selected);
    }
  }
</script>

<div class="space-y-4" data-testid="wizard-step-params">
  {#if args.length === 0}
    <p class="text-sm text-muted-foreground" data-testid="params-no-args">
      {$t("integrations.wizard.no_params_required")}
    </p>
  {:else}
    <div class="space-y-3">
      {#each args as arg, index (argKey(arg, index))}
        {@const key = argKey(arg, index)}
        {@const pathLike = isPathLike(arg.value_hint)}
        <div class="space-y-1.5">
          <label class="block text-sm font-medium text-foreground" for={`arg-${key}`}>
            {arg.value_hint ?? arg.arg_type}
            {#if arg.is_required}
              <span class="ml-1 text-xs text-destructive">*</span>
            {:else}
              <span class="ml-1 text-xs text-muted-foreground">({$t("integrations.wizard.optional")})</span>
            {/if}
          </label>

          {#if arg.description}
            <p class="text-xs text-muted-foreground">{arg.description}</p>
          {/if}

          <div class="flex gap-2">
            <Input
              id={`arg-${key}`}
              type="text"
              value={values[key] ?? ""}
              oninput={(e) => onchange(key, (e.currentTarget as HTMLInputElement).value)}
              placeholder={arg.value_hint ?? ""}
              class="flex-1"
              data-testid={`arg-input-${key}`}
            />
            {#if pathLike}
              <Button
                variant="outline"
                size="icon"
                onclick={() => pickPath(key, arg.value_hint)}
                aria-label={$t("integrations.wizard.browse")}
                data-testid={`arg-browse-${key}`}
              >
                <FolderOpen size={16} />
              </Button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
