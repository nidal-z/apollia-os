/**
 * The scope names the Microsoft connection asks for, sent by the connection
 * dialog to `oauth_start_flow`.
 *
 * The one list: the tests of `apollia-auth` read this file to build the
 * consent from it and check that it asks for the user's own files only, so
 * the dialog and the consent cannot drift apart. Keep it a flat array of
 * double-quoted names.
 *
 * "mail.readwrite" carries outlook.move: Graph refuses the move action on
 * Mail.Read alone, so a consent without it left that operation advertised and
 * unusable.
 */
export const MICROSOFT_SCOPES = [
  "mail.read",
  "mail.readwrite",
  "mail.send",
  "calendar.read",
  "calendar.write",
  "files.read",
] as const;
