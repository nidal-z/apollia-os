/**
 * Words for a model launch the runtime's load guardrail refused.
 *
 * The runtime answers such a launch with a typed body: `code` is
 * `guardrail_refused`, `error` is an English sentence, and the estimate and
 * the limit travel as numbers. The interface writes its own sentence from
 * those figures, in the locale in force, rather than showing the English one.
 *
 * The translator is caller-supplied, as in `humanize`, so the mapper stays
 * pure and testable.
 */

/** Machine-readable code of a refusal, as the runtime sends it. */
export const GUARDRAIL_REFUSED_CODE = "guardrail_refused";

/** The body the runtime answers a refused launch with. */
export interface GuardrailRefusedBody {
  error: string;
  code: typeof GUARDRAIL_REFUSED_CODE;
  model: string;
  level: "disabled" | "lenient" | "balanced" | "strict" | "ceiling";
  estimatedGb: number;
  budgetGb: number;
}

/** Translator taking interpolated values, as `svelte-i18n`'s `$t` does. */
export type GuardrailTranslate = (
  key: string,
  options: { values: Record<string, string> },
) => string;

export interface GuardrailRefusalText {
  title: string;
  message: string;
  suggested_action: string;
}

/** Whether `body` is a guardrail refusal from the runtime. */
export function isGuardrailRefusal(body: unknown): body is GuardrailRefusedBody {
  if (typeof body !== "object" || body === null) return false;
  const candidate = body as Record<string, unknown>;
  return (
    candidate.code === GUARDRAIL_REFUSED_CODE &&
    typeof candidate.estimatedGb === "number" &&
    typeof candidate.budgetGb === "number"
  );
}

/** The refusal in the words of `locale`, figures to one decimal. */
export function guardrailRefusalText(
  body: GuardrailRefusedBody,
  t: GuardrailTranslate,
  locale: string,
): GuardrailRefusalText {
  const gb = (value: number): string =>
    value.toLocaleString(locale, {
      minimumFractionDigits: 1,
      maximumFractionDigits: 1,
    });
  const values = {
    model: body.model,
    estimated: gb(body.estimatedGb),
    budget: gb(body.budgetGb),
  };
  return {
    title: t("errors.guardrail_refused.title", { values }),
    message: t("errors.guardrail_refused.message", { values }),
    suggested_action: t("errors.guardrail_refused.suggested_action", { values }),
  };
}
