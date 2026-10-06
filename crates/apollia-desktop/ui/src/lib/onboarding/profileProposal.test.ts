import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, test, expect, vi } from "vitest";
import type { ProfileFieldView } from "$lib/ipc/profile";
import type { ChatMessageView, ToolCallView } from "$lib/types";
import {
  applyReview,
  extractProposal,
  knownFieldsLine,
  layProposal,
  proposalChanges,
  reviewValues,
} from "./profileProposal";

const SIX = ["name", "role", "function", "tools", "preferences", "free_text"];

function field(key: string, value = ""): ProfileFieldView {
  return {
    key,
    label_fr: `fr:${key}`,
    label_en: `en:${key}`,
    help_fr: `aide:${key}`,
    help_en: `help:${key}`,
    long_text: key === "free_text",
    value,
  };
}

function call(
  output: unknown,
  status: ToolCallView["status"] = "executed",
  tool = "pkg:onboarding-agent/propose_profile",
): ToolCallView {
  return {
    tool_name: tool,
    input: {},
    output: typeof output === "string" ? output : JSON.stringify(output),
    status,
  };
}

function message(seq: number, calls: ToolCallView[] | null, content = ""): ChatMessageView {
  return {
    id: `m${seq}`,
    role: "assistant",
    content,
    tool_calls: calls,
    tool_name: null,
    seq,
    created_at: "2026-10-04T00:00:00Z",
  };
}

describe("extractProposal - the proposal is read in the conversation", () => {
  test("a conversation where the agent has not proposed holds no proposal", () => {
    // GIVEN an exchange of plain messages
    const messages = [message(1, null, "Hello"), message(2, [], "What do you do?")];

    // WHEN the proposal is looked for
    // THEN there is none
    expect(extractProposal(messages)).toBeNull();
  });

  test("the proposal is the output of the tool call, not the prose around it", () => {
    // GIVEN a closing message whose tool call answered with a proposal
    const messages = [
      message(1, null, "Hello"),
      message(
        2,
        [call({ proposal: { name: "Camille", role: "Responsable commerciale" }, completed: true })],
        "Your profile is ready to review.",
      ),
    ];

    // WHEN the proposal is looked for
    // THEN it is the structured block of that last message
    expect(extractProposal(messages)).toEqual({
      name: "Camille",
      role: "Responsable commerciale",
    });
  });

  test("the latest proposal wins when the agent proposed twice", () => {
    // GIVEN two proposals in the same conversation
    const messages = [
      message(1, [call({ proposal: { role: "sales" } })]),
      message(2, [call({ proposal: { role: "sales lead" } })]),
    ];

    // WHEN the proposal is looked for
    // THEN the second one is read
    expect(extractProposal(messages)).toEqual({ role: "sales lead" });
  });

  test("a call that did not run proposes nothing", () => {
    // GIVEN a refused call, a failed call and a call still pending
    const messages = [
      message(1, [call({ proposal: { role: "a" } }, "refused")]),
      message(2, [call({ proposal: { role: "b" } }, "failed")]),
      message(3, [call({ proposal: { role: "c" } }, "pending")]),
    ];

    // WHEN the proposal is looked for
    // THEN none of them counts
    expect(extractProposal(messages)).toBeNull();
  });

  test("another tool, an unreadable output and blank values are ignored", () => {
    // GIVEN the output of another tool and a broken output of the right one
    const messages = [
      message(1, [call({ proposal: { role: "x" } }, "executed", "pkg:other/remember")]),
      message(2, [call("tool error: boom")]),
    ];

    // WHEN the proposal is looked for
    // THEN nothing is read
    expect(extractProposal(messages)).toBeNull();

    // AND a proposal keeps only its non-blank text values
    const mixed = [message(3, [call({ proposal: { name: " Lea ", role: "  ", tools: 3 } })])];
    expect(extractProposal(mixed)).toEqual({ name: "Lea" });
  });

  test("an empty proposal is a proposal: the person skipped every question", () => {
    // GIVEN a call that ran and proposed nothing
    const messages = [message(1, [call({ proposal: {}, completed: true })])];

    // WHEN the proposal is looked for
    // THEN it is there, and empty
    expect(extractProposal(messages)).toEqual({});
  });
});

describe("reviewValues - the form opens on the proposal", () => {
  test("the proposed values fill the fields of an empty profile", () => {
    // GIVEN the six empty fields and a proposal for three of them
    const fields = SIX.map((k) => field(k));
    const proposal = { name: "Camille", role: "Responsable commerciale", tools: "Gmail, Excel" };

    // WHEN the form values are computed
    const values = reviewValues(fields, proposal);

    // THEN the three are filled and no field is required
    expect(values).toEqual({
      name: "Camille",
      role: "Responsable commerciale",
      function: "",
      tools: "Gmail, Excel",
      preferences: "",
      free_text: "",
    });
  });

  test("what the profile already holds stays where the agent proposed nothing", () => {
    // GIVEN a profile with a name and a role, and a proposal for the tools only
    const fields = [field("name", "Claire"), field("role", "Formatrice"), field("tools")];

    // WHEN the form values are computed
    const values = reviewValues(fields, { tools: "Google Slides, Zoom", unknown_key: "x" });

    // THEN the existing values are kept, the proposal is added, and a key the
    // profile does not declare is dropped
    expect(values).toEqual({ name: "Claire", role: "Formatrice", tools: "Google Slides, Zoom" });
  });

  test("a proposal that repeats the profile changes nothing", () => {
    // GIVEN a proposal equal to what is stored, and one that differs
    const fields = [field("role", "Formatrice"), field("tools")];

    // WHEN each is compared to the profile
    // THEN only the second one is worth a review
    expect(proposalChanges(fields, { role: "Formatrice" })).toBe(false);
    expect(proposalChanges(fields, {})).toBe(false);
    expect(proposalChanges(fields, null)).toBe(false);
    expect(proposalChanges(fields, { tools: "Zoom" })).toBe(true);
  });
});

describe("knownFieldsLine - the agent is told what the profile holds", () => {
  test("only the filled fields are listed, on one line", () => {
    // GIVEN a profile with two filled fields, one of them on two lines
    const fields = [field("name", "Claire"), field("role"), field("free_text", "Deux sessions\npar mois")];

    // WHEN the line is built with the French labels
    const line = knownFieldsLine(fields, (f) => f.label_fr);

    // THEN it names them and nothing else
    expect(line).toBe("fr:name: Claire; fr:free_text: Deux sessions par mois");
    expect(knownFieldsLine([field("name")], (f) => f.label_fr)).toBe("");
  });
});

describe("applyReview - Save writes, Skip writes nothing", () => {
  test("saving the reviewed proposal writes the corrected role to the default profile", async () => {
    // GIVEN the empty default profile, a proposal, and a role the person corrected
    const fields = SIX.map((k) => field(k));
    const values = reviewValues(fields, { name: "Camille", role: "Commerciale" });
    values.role = "Responsable commerciale";
    const update = vi.fn(async () => undefined);

    // WHEN the person saves
    const wrote = await applyReview("save", "default", fields, values, { update });

    // THEN one write reaches the default profile, with the corrected role
    expect(wrote).toBe(true);
    expect(update).toHaveBeenCalledTimes(1);
    expect(update).toHaveBeenCalledWith("default", {
      fields: { name: "Camille", role: "Responsable commerciale" },
    });
  });

  test("skipping writes nothing, even with a proposal on screen", async () => {
    // GIVEN a proposal laid over the empty default profile
    const fields = SIX.map((k) => field(k));
    const values = reviewValues(fields, { name: "Camille", role: "Commerciale" });
    const update = vi.fn(async () => undefined);

    // WHEN the person leaves without saving
    const wrote = await applyReview("skip", "default", fields, values, { update });

    // THEN the profile received no write
    expect(wrote).toBe(false);
    expect(update).not.toHaveBeenCalled();
  });

  test("saving a form the person emptied back writes nothing", async () => {
    // GIVEN a proposal the person erased field by field
    const fields = SIX.map((k) => field(k));
    const values = reviewValues(fields, { name: "Camille" });
    values.name = "  ";
    const update = vi.fn(async () => undefined);

    // WHEN the person saves
    const wrote = await applyReview("save", "default", fields, values, { update });

    // THEN nothing is written
    expect(wrote).toBe(false);
    expect(update).not.toHaveBeenCalled();
  });

  test("a failed write is not swallowed", async () => {
    // GIVEN a runtime that refuses the write
    const fields = [field("role")];
    const update = vi.fn(async () => {
      throw new Error("profile store unavailable");
    });

    // WHEN the person saves
    // THEN the refusal reaches the caller, which keeps the form open
    await expect(applyReview("save", "default", fields, { role: "x" }, { update })).rejects.toThrow(
      "profile store unavailable",
    );
  });
});

describe("layProposal - the banner's proposal comes back to the displayed profile", () => {
  const fields = [field("name", "Claire"), field("role", "Formatrice"), field("tools")];
  const shown = { name: "Claire", role: "Formatrice", tools: "" };

  test("the proposal fills the form of the profile it was asked for, unsaved", () => {
    // GIVEN the profile "formateur" on screen and a proposal asked for it
    const pending = { profileId: "formateur", proposal: { tools: "Google Slides, Zoom" } };

    // WHEN the proposal comes back
    const next = layProposal("formateur", fields, shown, pending);

    // THEN the form shows it over what the profile holds, for the same review
    expect(next).toEqual({ name: "Claire", role: "Formatrice", tools: "Google Slides, Zoom" });
  });

  test("an edit in progress on another field survives the proposal", () => {
    // GIVEN a name the person was typing when the proposal came back
    const typing = { ...shown, name: "Claire Lambert" };
    const pending = { profileId: "formateur", proposal: { tools: "Zoom" } };

    // WHEN the proposal is laid over the form
    const next = layProposal("formateur", fields, typing, pending);

    // THEN the typed name is kept
    expect(next?.name).toBe("Claire Lambert");
  });

  test("a proposal asked for another profile, or one that changes nothing, opens no review", () => {
    // GIVEN a proposal for another profile, an empty one and one equal to the profile
    // WHEN each comes back
    // THEN none opens a review
    expect(
      layProposal("formateur", fields, shown, { profileId: "perso", proposal: { tools: "Zoom" } }),
    ).toBeNull();
    expect(layProposal("formateur", fields, shown, { profileId: "formateur", proposal: {} })).toBeNull();
    expect(
      layProposal("formateur", fields, shown, {
        profileId: "formateur",
        proposal: { role: "Formatrice" },
      }),
    ).toBeNull();
  });
});

// DOM rendering is exercised by the gestural corpus of scripts/automation
// (vitest runs in `node`). These tests close the link the ones above leave
// open, by reading the templates: a decision function is only worth its tests
// if the markup actually hangs off it.
describe("the templates hang off these decisions", () => {
  const read = (path: string): string =>
    readFileSync(fileURLToPath(new URL(`../../${path}`, import.meta.url)), "utf8");

  test("the onboarding conversation ends on the review, with no permission step", () => {
    // GIVEN the chat step of the onboarding
    const step = read("components/onboarding/OnboardingChatStep.svelte");

    // WHEN its template is read
    // THEN it mounts the review of the proposal read in the conversation
    expect(step).toMatch(/<OnboardingProfileReview\s+\{proposal\}/);
    expect(step).toContain("extractProposal(detail.messages)");
    // AND the proposed permission rules are no longer part of the onboarding
    expect(step).not.toContain("OnboardingPermissionStep");
    expect(step).not.toContain("onboarding_chat.permission_banner");
  });

  test("the review draws the profile form, and its two buttons end it through applyReview", () => {
    // GIVEN the review of the onboarding
    const review = read("components/onboarding/OnboardingProfileReview.svelte");

    // WHEN its template is read
    // THEN the form is the one of Settings > Profile, over the default profile
    expect(review).toContain("<ProfileFieldGrid");
    expect(review).toContain("getProfileById(DEFAULT_PROFILE)");
    expect(read("components/settings/profile/ProfileFieldsCard.svelte")).toContain(
      "<ProfileFieldGrid",
    );
    // AND "Save" and "Skip without saving" both go through the tested decision
    expect(review).toMatch(/onclick=\{\(\) => end\("save"\)\}[^>]*data-testid="onboarding-review-save"/s);
    expect(review).toMatch(/onclick=\{\(\) => end\("skip"\)\}[^>]*data-testid="onboarding-review-skip"/s);
    expect(review).toContain("applyReview(choice, DEFAULT_PROFILE, fields, values");
  });

  test("the banner relaunches the agent for the displayed profile, and the page reviews its proposal", () => {
    // GIVEN the banner of Settings > Profile and the page that holds it
    const banner = read("components/settings/profile/ProfileEnrichmentBanner.svelte");
    const page = read("routes/settings/Profile.svelte");

    // WHEN their sources are read
    // THEN the banner asks for the profile on screen, not the default one
    expect(banner).toContain("profileEnrichment.set({ profileId, fields })");
    expect(page).toContain("profileId={detail.profile.id}");
    // AND the proposal that comes back is laid over the form, saved only by
    // the Save of the page and dropped by its Reset
    expect(page).toContain("layProposal(detail.profile.id, detail.fields, values, pending)");
    expect(page).toContain("onsave={() => void save()}");
    expect(page).toContain("onskip={reset}");
    expect(page).not.toMatch(/updateProfile\([^)]*pending/);
  });
});
