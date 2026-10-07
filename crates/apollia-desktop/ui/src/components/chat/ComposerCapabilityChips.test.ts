import { describe, test, expect, vi, beforeAll, beforeEach } from "vitest";
import { render } from "svelte/server";
import { locale, waitLocale } from "svelte-i18n";
import "$lib/i18n";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

// The tooltip of the application needs the provider mounted at its root and
// shows nothing until hovered. Here it is replaced by a plain wrapper that
// writes what it says next to what it explains, so a test can read both.
type Renderer = { push: (html: string) => void };
vi.mock("$lib/components/ui/tooltip", () => ({
  Tooltip: (
    renderer: Renderer,
    props: {
      content: string | ((renderer: Renderer) => void);
      children?: (renderer: Renderer) => void;
      trigger?: (renderer: Renderer, props: Record<string, unknown>) => void;
    },
  ) => {
    if (props.trigger) props.trigger(renderer, {});
    else props.children?.(renderer);
    renderer.push('<div data-tooltip="">');
    if (typeof props.content === "string") renderer.push(props.content);
    else props.content(renderer);
    renderer.push("</div>");
  },
}));

import ComposerCapabilityChips from "./ComposerCapabilityChips.svelte";
import { loadSessionSettings } from "$lib/stores/sessionSettings";
import type { ChatModelCapabilities, SessionSettings } from "$lib/types";
import fr from "$lib/i18n/fr.json";

/** The French interface text of a chip. */
const french = fr.chat.chips;

function model(
  reasoning_profile: ChatModelCapabilities["reasoning_profile"],
  vision = false,
): ChatModelCapabilities {
  return {
    model: "local-model",
    local: true,
    reasoning_profile,
    vision,
    image_formats: vision ? ["png"] : [],
  };
}

/** Stores `settings` for `sessionId` as the runtime would answer them. */
async function conversation(sessionId: string, settings: SessionSettings): Promise<void> {
  invoke.mockResolvedValueOnce(settings);
  await loadSessionSettings(sessionId);
}

function chips(
  sessionId: string,
  capabilities: ChatModelCapabilities | null,
  disabled = false,
): string {
  return render(ComposerCapabilityChips, { props: { sessionId, capabilities, disabled } }).body;
}

/** The element carrying `data-testid="<testid>"`, tag and text, or `null`. */
function element(html: string, testid: string): string | null {
  const match = html.match(
    new RegExp(`<(button|span)[^>]*data-testid="${testid}"[^>]*>[\\s\\S]*?</\\1>`),
  );
  return match ? match[0] : null;
}

beforeAll(async () => {
  await waitLocale();
});

beforeEach(async () => {
  invoke.mockReset();
  await locale.set("fr");
});

describe("ComposerCapabilityChips - the chips follow the model", () => {
  test("a model with no capability shows no chip at all", () => {
    // GIVEN a model with no reasoning setting and no vision
    // WHEN the composer renders its chips
    const html = chips("c-none", model("none"));

    // THEN there is no chip, nor the row that holds them
    expect(html).not.toContain("composer-capability-chips");
    expect(html).not.toContain("composer-chip-");
  });

  test("a model that reasons on demand shows the switch, pressed", () => {
    // GIVEN a model whose reasoning is switched on or off
    // WHEN the composer renders its chips
    const html = chips("c-toggle", model("toggle"));
    const chip = element(html, "composer-chip-thinking");

    // THEN the « Réfléchir » switch is there, on, and no level chip
    expect(chip).toContain(french.thinking);
    expect(chip).toContain('aria-pressed="true"');
    expect(html).not.toContain("composer-chip-effort");
    expect(html).not.toContain("composer-chip-vision");
  });

  test("the switch shows off in a conversation that switched reasoning off", async () => {
    // GIVEN a conversation that stored reasoning off
    await conversation("c-off", { thinking: false });

    // WHEN the composer renders its chips
    const chip = element(chips("c-off", model("toggle")), "composer-chip-thinking");

    // THEN the switch is not pressed
    expect(chip).toContain('aria-pressed="false"');
  });

  test("a model that reads a level shows « Raisonnement : » and its value", async () => {
    // GIVEN a model with levels, in a conversation that chose the middle one
    await conversation("c-medium", { reasoning_effort: "medium" });

    // WHEN the composer renders its chips for it and for an untouched one
    const chosen = element(chips("c-medium", model("levels")), "composer-chip-effort");
    const html = chips("c-default", model("levels"));
    const byDefault = element(html, "composer-chip-effort");

    // THEN the chip names the value, and the default when nothing was chosen
    expect(chosen).toContain(
      french.effort.replace("{value}", french.levels.medium.toLowerCase()),
    );
    expect(chosen).toContain(": moyen");
    expect(chosen).toContain('data-level="medium"');
    expect(byDefault).toContain(french.effort.replace("{value}", french.effort_default));
    expect(byDefault).toContain('data-level="default"');
    // AND the switch is not offered beside it
    expect(html).not.toContain("composer-chip-thinking");
  });

  test("the vision chip is there with vision and absent without", () => {
    // GIVEN a model that reads images and one that does not, both with a switch
    // WHEN the composer renders the chips of each
    const seeing = chips("c-vision", model("toggle", true));
    const blind = chips("c-blind", model("toggle", false));

    // THEN only the first carries « Vision », as a statement and not a control
    expect(element(seeing, "composer-chip-vision")).toMatch(/^<span[\s\S]*Vision/);
    expect(blind).not.toContain("composer-chip-vision");
  });

  test("an unknown profile shows no reasoning chip, vision still follows", () => {
    // GIVEN a remote model whose reasoning profile is unknown but reads images
    // WHEN the composer renders its chips, and those of unread capabilities
    const remote = chips("c-remote", model(null, true));
    const unread = chips("c-unread", null);

    // THEN no reasoning chip is simulated
    expect(remote).not.toContain("composer-chip-thinking");
    expect(remote).not.toContain("composer-chip-effort");
    expect(remote).toContain("composer-chip-vision");
    expect(unread).not.toContain("composer-chip-");
  });

  test("the chips of a closed conversation are disabled", () => {
    // GIVEN a conversation that takes no input
    // WHEN the composer renders its chips
    const chip = element(chips("c-closed", model("toggle"), true), "composer-chip-thinking");

    // THEN the switch cannot be pressed
    expect(chip).toMatch(/\sdisabled/);
  });

  test("the chips speak English in the English interface", async () => {
    // GIVEN the English interface and a conversation on the low level
    await conversation("c-en", { reasoning_effort: "low" });
    await locale.set("en");

    // WHEN the composer renders the chips of a model with levels and vision
    const html = chips("c-en", model("levels", true));

    // THEN both chips are in English
    expect(element(html, "composer-chip-effort")).toContain("Reasoning: low");
    expect(element(html, "composer-chip-vision")).toContain("Vision");
  });
});
