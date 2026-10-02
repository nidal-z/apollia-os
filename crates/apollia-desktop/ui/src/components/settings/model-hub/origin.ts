/**
 * The origin mark of an installed model: a model whose weights matched no
 * published or already verified digest is shown as of unverified origin.
 */
import type { InstalledModel } from "$lib/ipc/modelHub";

export const UNVERIFIED_ORIGIN_KEY = "settings.model_hub.installed.unverified_origin";

/** The i18n key of the mark to show beside `model`, or `null` for none. */
export function unverifiedOriginKey(model: Pick<InstalledModel, "origin_verified">): string | null {
  return model.origin_verified ? null : UNVERIFIED_ORIGIN_KEY;
}
