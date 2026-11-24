import type { MessageArea } from "../catalog";

export const share = {
  // components/ShareDialog
  share: "Share {name}",
  share2: "Share {selectionSummary}",
  onIn: "On {service} in {folder}",
  links: "Links",
  lookingForLinksItAlready: "Looking for links it already has...",
  changeLink: "Change link",
  newLink: "New link",
  letsPeople: "Lets people",
  whoCanOpenIt: "Who can open it",
  worksFor: "Works for",
  noEnd: "No end",
  custom: "Custom",
  howMany: "How many",
  unit: "Unit",
  passwordOptional: "Password (optional)",
  anyoneWithTheLinkCan: "Anyone with the link can open it",
  copyNewLink: "Copy new link",
  copyNewLinks: "Copy new links",
  linkAddress: "Link address",
  openInBrowser: "Open in browser",
  stopSharing: "Stop sharing",
  peopleWhoHaveThisLink: "People who have this link can no longer open it.",
  keepSharing: "Keep sharing",

  // lib/shareLinks
  shareLinkNotInA: "Share link (not in a vault)",
  shareLinkCloudStorageOnly: "Share link (cloud storage only)",
  shareLinkFilesOnly: "Share link (files only)",
  shareLink: "Share link...",
  days: { one: "{count} day", other: "{count} days" },
  hours: { one: "{count} hour", other: "{count} hours" },
  minutes: { one: "{count} minute", other: "{count} minutes" },
  seconds: { one: "{count} second", other: "{count} seconds" },
  files: { one: "{count} file", other: "{count} files" },
  folders: { one: "{count} folder", other: "{count} folders" },
  // components/ShareDialog
  minutes2: "Minutes",
  hours2: "Hours",
  days2: "Days",

  // lib/shareLinks
  view: "View",
  n1Hour: "1 hour",
  n1Day: "1 day",
  n7Days: "7 days",
} as const satisfies MessageArea;
