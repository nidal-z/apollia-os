import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Which catalogue keys the product can still reach, and which are weight.
 *
 * `call-site-keys.test.ts` asks one direction: does the catalogue answer every
 * key the code requests. Nothing asked the other one, so a key survived the
 * death of its screen forever, and 1122 of 4857 leaves (23 %) had no reader
 * left. Dead entries are not neutral: they are what makes every other question
 * about the catalogue unreadable by eye, parity and duplicates included.
 *
 * A key counts as reached when one of five things holds, and the five are the
 * whole model:
 *
 *   1. a source file under `src/` spells it as a dotted string literal, in any
 *      quoting, whether or not the literal sits inside a `$t(...)` form. Keys
 *      travel through props and tables (`labelKey: "agents.start"`), so the
 *      call form is not where they are written down;
 *   2. a declared interpolation builds it. Those are listed by hand below, each
 *      with the module that builds it: a generated list would have to accept
 *      `` `${prefix}.${name}` `` from a JSON-schema walker, which matches 2128
 *      catalogue keys and blanks the whole measure;
 *   3. the desktop shell names it, `crates/apollia-desktop/src/**` in Rust or in
 *      `mcp/enrichments.json`, which is how `integrations.connectors.figma.auth_help`
 *      reaches the interface without any TypeScript mentioning it;
 *   4. a module that imports a catalogue reads it as a member chain rather than
 *      through `$t`. `lib/stores/hitl.ts` is the only one, and it renders the
 *      native notification of a pending approval: no quoted key anywhere, just
 *      `messages.notifications.native.approval_title`;
 *   5. it is one of the fixtures the guards themselves need, listed by name.
 *
 * Test files are deliberately not call sites. A key kept alive by the test that
 * measures it is the measure certifying itself.
 */

const HERE = dirname(fileURLToPath(import.meta.url));
/** `crates/apollia-desktop/ui/src` */
export const SOURCE_ROOT = resolve(HERE, "../..");
/** `crates/apollia-desktop/src`, the Rust shell of the same crate. */
export const SHELL_ROOT = resolve(HERE, "../../../../src");

/** Any quoted string shaped like a dotted key: `"a.b"`, `'a.b.c'`, `` `a.b` ``. */
const DOTTED_LITERAL = /["'`]([a-zA-Z0-9_]+(?:\.[a-zA-Z0-9_]+)+)["'`]/g;

/** A module that pulls a catalogue in as data rather than through `$t`. */
const CATALOGUE_IMPORT = /i18n\/(en|fr)\.json/;

/** A member chain hanging off an identifier: `messages.a.b.c` yields `a.b.c`. */
const MEMBER_CHAIN = /[A-Za-z0-9_)\]]\.((?:[a-zA-Z0-9_]+\.)+[a-zA-Z0-9_]+)/g;

/** One segment of an interpolated key: what `${...}` can expand to. */
const SEG = "[A-Za-z0-9_]+";

/**
 * The keys no literal spells out, built by interpolation at runtime.
 *
 * Each entry names the module that builds it. A new builder is not discovered
 * here: its keys read as dead until someone adds the line, which is the
 * direction this guard is meant to fail in.
 */
export const DECLARED_INTERPOLATIONS: { pattern: RegExp; builtBy: string }[] = [
  {
    pattern: new RegExp(`^agents\\.update_restart_(stop|start)_failed_action$`),
    builtBy: "components/agents/useAgentActions.svelte.ts",
  },
  {
    pattern: new RegExp(`^(approvals\\.risk|hitl\\.impact)\\.${SEG}$`),
    builtBy:
      "lib/components/operator/badges/RiskBadge.svelte, components/chat/HitlFilesystemModal.svelte",
  },
  {
    pattern: new RegExp(`^approvals\\.scope\\.${SEG}$`),
    builtBy: "components/chat/ApprovalScopeSelect.svelte",
  },
  {
    pattern: new RegExp(`^automations\\.humanize\\.${SEG}$`),
    builtBy: "lib/automations/humanize.ts",
  },
  {
    pattern: new RegExp(`^chat\\.agent_status\\.${SEG}$`),
    builtBy: "components/chat/AgentStatusCard.svelte",
  },
  {
    pattern: new RegExp(
      `^companion\\.error\\.${SEG}\\.(cause_1|cause_2|cause_3|message|title)$`,
    ),
    builtBy: "components/companion/CompanionErrorState.svelte",
  },
  {
    pattern: new RegExp(
      `^connections\\.capabilities\\.(group|scope_policy_body)_${SEG}$`,
    ),
    builtBy: "components/connections/detail/CapabilityMatrix.svelte",
  },
  {
    pattern: new RegExp(`^connections\\.capabilities\\.entries\\.${SEG}$`),
    builtBy: "lib/connections/capabilities.ts",
  },
  {
    pattern: new RegExp(
      `^errors\\.${SEG}\\.(title|friendly_message|suggested_action)$`,
    ),
    builtBy: "lib/errors/humanize.ts",
  },
  {
    pattern: new RegExp(`^inbox\\.group\\.${SEG}$`),
    builtBy: "components/inbox/InboxPendingList.svelte",
  },
  {
    pattern: new RegExp(`^memory\\.namespaces\\.cat_${SEG}_header$`),
    builtBy: "components/memory/NamespaceSidebar.svelte",
  },
  {
    // The event type carries its own dot (`task.completed`), so this one
    // segment is the only place a dot is allowed inside an expansion.
    pattern: /^notifications\.events(_desc)?\.[A-Za-z0-9_.]+$/,
    builtBy: "lib/notifications/event-labels.ts",
  },
  {
    pattern: new RegExp(
      `^observability\\.plan_mutation\\.(field|kind)\\.${SEG}$`,
    ),
    builtBy: "components/observability/PlanMutationRow.svelte",
  },
  {
    pattern: new RegExp(`^projects\\.provider_type_${SEG}$`),
    builtBy:
      "components/project/ProviderEditDialog.svelte, components/project/ProviderCard.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.about\\.local_${SEG}$`),
    builtBy: "routes/settings/About.svelte",
  },
  {
    pattern: new RegExp(
      `^settings\\.danger\\.${SEG}\\.(button|description|dialog_desc|dialog_title|title)$`,
    ),
    builtBy: "routes/settings/Danger.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.help\\.faq_(a|q)_${SEG}$`),
    builtBy: "routes/settings/Help.svelte",
  },
  {
    pattern: new RegExp(
      `^settings\\.integrations\\.placeholder\\.client_id_${SEG}$`,
    ),
    builtBy: "routes/settings/Integrations.svelte",
  },
  {
    pattern: new RegExp(
      `^settings\\.integrations\\.provider(_meta)?\\.${SEG}$`,
    ),
    builtBy: "routes/settings/Integrations.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.integrations\\.source\\.${SEG}$`),
    builtBy: "components/integrations/OauthCredentialHeader.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.llm_dialog\\.err_${SEG}$`),
    builtBy: "components/settings/LlmBackendDialog.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.nav\\.${SEG}$`),
    builtBy:
      "lib/command-palette/paletteIndex.ts, lib/navigation/breadcrumbSegments.ts",
  },
  {
    pattern: new RegExp(`^settings\\.observability\\.${SEG}$`),
    builtBy: "routes/settings/Observability.svelte",
  },
  {
    pattern: new RegExp(`^settings\\.tools_page\\.${SEG}$`),
    builtBy: "components/settings/tools/toolCatalog.ts",
  },
  {
    pattern: new RegExp(`^sidebar\\.mode\\.${SEG}\\.(description|label)$`),
    builtBy: "lib/components/app/ModeChip.svelte",
  },
  {
    pattern: new RegExp(`^stt_failure\\.${SEG}$`),
    builtBy: "lib/stt/dictationFailure.ts",
  },
  {
    pattern: new RegExp(`^tools\\.body\\.${SEG}$`),
    builtBy:
      "components/chat/tool-bodies/TodoBody.svelte, HttpFetchBody.svelte, FileListBody.svelte",
  },
  {
    pattern: new RegExp(`^tools\\.body\\.bash_describe\\.${SEG}$`),
    builtBy: "lib/chat/toolBodies.ts",
  },
  {
    pattern: new RegExp(`^tools\\.(descriptions|labels)\\.${SEG}$`),
    builtBy: "lib/tools/tool-display.ts",
  },
  {
    pattern: new RegExp(
      `^tour\\.band\\.milestone\\.${SEG}\\.(cta|hint|label)$`,
    ),
    builtBy: "components/dashboard/GettingStartedBand.svelte",
  },
  {
    pattern: new RegExp(`^trace\\.describe\\.${SEG}$`),
    builtBy: "lib/utils/bashDescriber.ts",
  },
  {
    pattern: new RegExp(`^transcriptions\\.source_${SEG}$`),
    builtBy: "components/stt/TranscriptCard.svelte",
  },
];

/**
 * Keys a guard needs and the product does not call.
 *
 * Empty, and that is the intended state. The three tests that pinned entries
 * the product had stopped rendering (`common.workspace`, `tools.output.*`,
 * `tools.status.*`) were moved onto live keys rather than granted an exemption,
 * because a key kept alive by the test that measures it is the measure
 * certifying itself. The list exists so that the next one is declared by name
 * instead of hidden inside a scanner rule, and the guard reports when a name
 * here outlives the entry it excuses.
 */
export const GUARD_FIXTURE_KEYS: string[] = [];

/** Comments hold worked examples; a key quoted in prose is not a call site. */
export function withoutComments(text: string): string {
  return text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^[ \t]*\/\/.*$/gm, "");
}

function walk(root: string, keep: (path: string) => boolean): string[] {
  const found: string[] = [];
  for (const entry of readdirSync(root)) {
    if (entry === "node_modules" || entry === "target") continue;
    const full = join(root, entry);
    if (statSync(full).isDirectory()) {
      found.push(...walk(full, keep));
    } else if (keep(full)) {
      found.push(full);
    }
  }
  return found;
}

/**
 * Files that name catalogue keys to support a guard rather than to render
 * them. `identicalLocales.ts` lists 226 keys as string literals, and counting
 * it as a call site would make every one of them immortal: the exemption list
 * of one guard would silently answer another guard's question.
 */
const GUARD_SUPPORT = ["lib/i18n/identicalLocales.ts"];

/** Product sources under `src/`: tests and guard support excluded, on purpose. */
export function productFiles(): string[] {
  return walk(
    SOURCE_ROOT,
    (path) =>
      (path.endsWith(".svelte") ||
        path.endsWith(".ts") ||
        path.endsWith(".js")) &&
      !path.endsWith(".test.ts") &&
      !path.endsWith(".spec.ts") &&
      !GUARD_SUPPORT.some((name) => path.endsWith(name)),
  );
}

/** Rust shell sources and the MCP enrichment table they ship. */
export function shellFiles(): string[] {
  return walk(
    SHELL_ROOT,
    (path) => path.endsWith(".rs") || path.endsWith(".json"),
  );
}

/** Every dotted literal the product spells out, with the files that spell it. */
export function literalKeys(): Map<string, string[]> {
  const seen = new Map<string, string[]>();
  const record = (name: string, file: string) => {
    const where = seen.get(name) ?? [];
    where.push(file);
    seen.set(name, where);
  };
  for (const file of productFiles()) {
    const text = withoutComments(readFileSync(file, "utf8"));
    for (const match of text.matchAll(DOTTED_LITERAL)) {
      record(match[1], relative(SOURCE_ROOT, file));
    }
    if (!CATALOGUE_IMPORT.test(text)) continue;
    for (const match of text.matchAll(MEMBER_CHAIN)) {
      record(match[1], relative(SOURCE_ROOT, file));
    }
  }
  for (const file of shellFiles()) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(DOTTED_LITERAL)) {
      record(match[1], relative(SHELL_ROOT, file));
    }
  }
  return seen;
}

export type JsonObject = Record<string, unknown>;

/** Flatten a catalogue to its dotted leaf keys and their strings. */
export function catalogueLeaves(
  catalogue: JsonObject,
  prefix = "",
): Map<string, string> {
  const out = new Map<string, string>();
  for (const [key, value] of Object.entries(catalogue)) {
    const full = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object") {
      for (const [k, v] of catalogueLeaves(value as JsonObject, full))
        out.set(k, v);
    } else if (typeof value === "string") {
      out.set(full, value);
    }
  }
  return out;
}

/** The catalogue keys nothing in the product reaches, sorted. */
export function deadKeys(
  keys: Iterable<string>,
  reached = literalKeys(),
): string[] {
  const fixtures = new Set(GUARD_FIXTURE_KEYS);
  const dead: string[] = [];
  for (const key of keys) {
    if (reached.has(key) || fixtures.has(key)) continue;
    if (DECLARED_INTERPOLATIONS.some(({ pattern }) => pattern.test(key)))
      continue;
    dead.push(key);
  }
  return dead.sort();
}
