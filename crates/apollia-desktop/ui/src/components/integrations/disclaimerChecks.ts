/**
 * What the acknowledgement boxes of the connector wizard should hold once the
 * accepted-version lookup comes back.
 *
 * That lookup hashes the disclaimer text, so it is a promise that resolves
 * after the dialog is already on screen. It used to assign the boxes
 * unconditionally, which raced the person in front of it: on a machine where
 * the hash landed after the four boxes had been ticked, it wiped them and
 * "next" stayed disabled for good, with no way back other than closing the
 * wizard. Measured on 2026-09-08 on Windows, where it cost every connector
 * install of the run.
 *
 * The rule is one line: what someone has already touched is never overwritten.
 */
const ACKNOWLEDGEMENTS = [
  "code_on_machine",
  "external_data",
  "revocable",
  "read_capabilities",
] as const;

export function nextDisclaimerChecks(
  touched: boolean,
  accepted: boolean,
  current: Record<string, boolean>,
): Record<string, boolean> {
  if (touched) return current;
  if (!accepted) return {};
  return Object.fromEntries(ACKNOWLEDGEMENTS.map((key) => [key, true]));
}
