<script lang="ts">
  /**
   * First-launch step 2, "use a server": the address, an optional key, the
   * test, then the models of the server, one of which becomes the default
   * model. Everything happens inside the step.
   *
   * The decisions live in `serverSetup.ts`; this file renders its state.
   */
  import { t } from "svelte-i18n";
  import { CheckCircle2 } from "lucide-svelte";
  import { Banner } from "$lib/components/ui/banner";
  import { Button } from "$lib/components/ui/button";
  import { FormField } from "$lib/components/ui/form-field";
  import { Input } from "$lib/components/ui/input";
  import { Spinner } from "$lib/components/ui/progress";
  import { RadioItem } from "$lib/components/ui/radio";
  import { SERVER_ADDRESS_EXAMPLE, SERVER_FAILURE_TEXT, type ServerSetup } from "./serverSetup";

  interface Props {
    /** The setup the form drives, shared with the step that reads its outcome. */
    setup: ServerSetup;
  }

  const { setup }: Props = $props();

  // The address of a server already chosen is shown again when the person
  // comes back to the step. The key is not: it left for the secret store.
  let address = $state($setup.chosen?.baseUrl ?? "");
  let apiKey = $state("");

  const testing = $derived($setup.phase === "testing");
  const chosenModel = $derived(
    $setup.chosen !== null && $setup.models.includes($setup.chosen.model)
      ? $setup.chosen.model
      : "",
  );

  function runTest(event?: Event): void {
    event?.preventDefault();
    void setup.test(address, apiKey);
  }
</script>

<section class="setup-section server-form" data-testid="onboarding-server-form">
  <form class="server-fields" novalidate onsubmit={runTest}>
    <FormField
      id="onboarding-server-address"
      label={$t("onboarding.ai_setup.server.address_label")}
      hint={$t("onboarding.ai_setup.server.address_hint", {
        values: { example: SERVER_ADDRESS_EXAMPLE },
      })}
    >
      <Input
        id="onboarding-server-address"
        type="url"
        bind:value={address}
        oninput={() => setup.edited()}
        placeholder={SERVER_ADDRESS_EXAMPLE}
        autocomplete="off"
        spellcheck={false}
        data-testid="onboarding-server-address"
      />
    </FormField>
    <FormField
      id="onboarding-server-key"
      label={$t("onboarding.ai_setup.server.key_label")}
      hint={$t("onboarding.ai_setup.server.key_hint")}
      optional
    >
      <Input
        id="onboarding-server-key"
        type="password"
        bind:value={apiKey}
        oninput={() => setup.edited()}
        autocomplete="off"
        spellcheck={false}
        data-testid="onboarding-server-key"
      />
    </FormField>
    <div class="server-actions">
      <Button
        type="submit"
        variant="outline"
        size="sm"
        disabled={testing || address.trim() === ""}
        data-testid="onboarding-server-test"
      >
        {#if testing}<Spinner size={12} />{/if}
        {testing
          ? $t("onboarding.ai_setup.server.testing")
          : $t("onboarding.ai_setup.server.test")}
      </Button>
    </div>
  </form>

  {#if $setup.failure}
    {@const text = SERVER_FAILURE_TEXT[$setup.failure.kind]}
    <Banner
      variant="destructive"
      surface="card"
      title={$t(text.title)}
      data-testid="onboarding-server-error"
    >
      <p>{$t(text.body)}</p>
      {#if $setup.failure.detail}
        <p class="server-error-detail" data-testid="onboarding-server-error-detail">
          {$setup.failure.detail}
        </p>
      {/if}
    </Banner>
  {/if}

  {#if $setup.phase === "listed"}
    <div class="server-models" data-testid="onboarding-server-models">
      <p class="server-models-found" role="status">
        {$t("onboarding.ai_setup.server.models_found", { values: { count: $setup.models.length } })}
      </p>
      <div
        class="server-model-list"
        role="radiogroup"
        aria-label={$t("onboarding.ai_setup.server.models_label")}
      >
        {#each $setup.models as model (model)}
          <RadioItem
            value={model}
            checked={chosenModel === model}
            loading={$setup.choosing === model}
            disabled={$setup.choosing !== null && $setup.choosing !== model}
            onchange={(value) => void setup.choose(value)}
            class="server-model-row"
            data-testid="onboarding-server-model"
          >
            {model}
          </RadioItem>
        {/each}
      </div>
    </div>
  {/if}

  {#if $setup.chosen}
    <p class="server-chosen" role="status" data-testid="onboarding-server-chosen">
      <CheckCircle2 size={13} aria-hidden="true" />
      {$t("onboarding.ai_setup.server.chosen", {
        values: { model: $setup.chosen.model, address: $setup.chosen.baseUrl },
      })}
    </p>
  {/if}
</section>
