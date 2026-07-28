/**
 * Capability matrix data for the native connectors (Google, Microsoft).
 *
 * Pure data: every label is an i18n key resolved by the caller. The booleans
 * mirror the actual scope set exposed in the Rust connector providers for
 * v0.1.0 (free-tier OAuth scopes, no CASA audit), so some Gmail / Drive verbs
 * are intentionally out of reach. Keep both sides in sync.
 */

/** Native connector provider identifier. */
export type ProviderId = "google" | "microsoft";

export interface CapabilityEntry {
  labelKey: string;
  detailKey?: string;
  supported: boolean;
}

export interface CapabilityGroup {
  key: string;
  entries: CapabilityEntry[];
}

const E = "connections.capabilities.entries";

const GOOGLE_GROUPS: CapabilityGroup[] = [
  {
    key: "gmail",
    entries: [
      { labelKey: `${E}.gmail_send`, supported: true },
      { labelKey: `${E}.gmail_compose`, supported: true },
      {
        labelKey: `${E}.gmail_read_inbox`,
        detailKey: `${E}.gmail_read_inbox_detail`,
        supported: false,
      },
      {
        labelKey: `${E}.gmail_search`,
        detailKey: `${E}.gmail_read_inbox_detail`,
        supported: false,
      },
    ],
  },
  {
    key: "gcal",
    entries: [
      { labelKey: `${E}.gcal_list`, supported: true },
      { labelKey: `${E}.gcal_get`, supported: true },
      { labelKey: `${E}.gcal_create`, supported: true },
      { labelKey: `${E}.gcal_update`, supported: true },
      { labelKey: `${E}.gcal_delete`, supported: true },
    ],
  },
  {
    key: "gdrive",
    entries: [
      {
        labelKey: `${E}.gdrive_workspace_list`,
        detailKey: `${E}.gdrive_workspace_detail`,
        supported: true,
      },
      { labelKey: `${E}.gdrive_workspace_read`, supported: true },
      { labelKey: `${E}.gdrive_workspace_write`, supported: true },
      { labelKey: `${E}.gdrive_workspace_share`, supported: true },
      {
        labelKey: `${E}.gdrive_picker`,
        detailKey: `${E}.gdrive_picker_detail`,
        supported: true,
      },
      {
        labelKey: `${E}.gdrive_full`,
        detailKey: `${E}.gdrive_full_detail`,
        supported: false,
      },
      {
        labelKey: `${E}.gdrive_write_anywhere`,
        detailKey: `${E}.gdrive_write_anywhere_detail`,
        supported: false,
      },
    ],
  },
  {
    key: "gsheets",
    entries: [
      { labelKey: `${E}.gsheets_create`, supported: true },
      { labelKey: `${E}.gsheets_read_values`, supported: true },
      {
        labelKey: `${E}.gsheets_write_values`,
        detailKey: `${E}.gsheets_detail`,
        supported: true,
      },
    ],
  },
  {
    key: "gdocs",
    entries: [
      { labelKey: `${E}.gdocs_create`, supported: true },
      { labelKey: `${E}.gdocs_read_text`, supported: true },
      {
        labelKey: `${E}.gdocs_append_text`,
        detailKey: `${E}.gdocs_detail`,
        supported: true,
      },
    ],
  },
  {
    key: "gslides",
    entries: [
      { labelKey: `${E}.gslides_create`, supported: true },
      {
        labelKey: `${E}.gslides_append_slide`,
        detailKey: `${E}.gslides_detail`,
        supported: true,
      },
    ],
  },
  {
    key: "gtasks",
    entries: [
      { labelKey: `${E}.gtasks_list`, supported: true },
      { labelKey: `${E}.gtasks_create`, supported: true },
      { labelKey: `${E}.gtasks_complete`, supported: true },
      {
        labelKey: `${E}.gtasks_delete`,
        detailKey: `${E}.gtasks_detail`,
        supported: true,
      },
    ],
  },
  {
    key: "gforms",
    entries: [
      {
        labelKey: `${E}.gforms_create`,
        detailKey: `${E}.gforms_detail`,
        supported: true,
      },
    ],
  },
  {
    key: "youtube",
    entries: [
      { labelKey: `${E}.youtube_search`, supported: true },
      {
        labelKey: `${E}.youtube_video_details`,
        detailKey: `${E}.youtube_detail`,
        supported: true,
      },
    ],
  },
];

const MICROSOFT_GROUPS: CapabilityGroup[] = [
  {
    key: "outlook_mail",
    entries: [
      { labelKey: `${E}.outlook_search`, supported: true },
      { labelKey: `${E}.outlook_read`, supported: true },
      { labelKey: `${E}.outlook_send`, supported: true },
      { labelKey: `${E}.outlook_reply`, supported: true },
      { labelKey: `${E}.outlook_folders`, supported: true },
    ],
  },
  {
    key: "outlook_calendar",
    entries: [
      { labelKey: `${E}.outlook_cal_read`, supported: true },
      { labelKey: `${E}.outlook_cal_create`, supported: true },
      { labelKey: `${E}.outlook_cal_update`, supported: true },
      { labelKey: `${E}.outlook_cal_delete`, supported: true },
    ],
  },
  {
    key: "onedrive",
    entries: [
      { labelKey: `${E}.onedrive_search`, supported: true },
      {
        labelKey: `${E}.onedrive_read`,
        detailKey: `${E}.onedrive_detail`,
        supported: true,
      },
      { labelKey: `${E}.onedrive_recent`, supported: true },
      {
        labelKey: `${E}.onedrive_write`,
        detailKey: `${E}.onedrive_write_detail`,
        supported: false,
      },
    ],
  },
];

/** Capability groups for a native connector provider. */
export function capabilityGroupsFor(providerId: ProviderId): CapabilityGroup[] {
  return providerId === "google" ? GOOGLE_GROUPS : MICROSOFT_GROUPS;
}

/** OAuth provider settings copy shown on the native "Settings" tab. */
export function scopeLabelKeys(providerId: ProviderId): string[] {
  return providerId === "google"
    ? [
        "connections.settings_scope_gmail",
        "connections.settings_scope_gcal",
        "connections.settings_scope_gdrive",
      ]
    : [
        "connections.settings_scope_outlook_mail",
        "connections.settings_scope_outlook_cal",
        "connections.settings_scope_onedrive",
      ];
}

/** Human-readable OAuth provider endpoint (not user-facing copy, stable). */
export function providerEndpoint(providerId: ProviderId): string {
  return providerId === "google"
    ? "OAuth 2.0 - accounts.google.com"
    : "OAuth 2.0 - login.microsoftonline.com";
}
