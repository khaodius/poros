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
  minutes2: "Minutes",
  hours2: "Hours",
  days2: "Days",
  link: "Link",
  links2: { one: "{count} links", other: "{count} links" },
  keepsOneLinkPerItem:
    "{service} keeps one link per item, so this changes the link above for everyone who has it.",
  keepsOneLinkPerItemSo:
    "{service} keeps one link per item, so an item that has one gets it changed for everyone who has it.",
  changing: "Changing...",
  creating: "Creating...",
  copied: "Copied",
  stopping: "Stopping...",
  couldNotReadTheLinksItHas: "Could not read the links it has: {message}",
  createLink: "Create link",
  createLinks: { one: "Create {count} link", other: "Create {count} links" },
  copyLink: "Copy link",
  checking: "Checking...",
  makingTheLink: "Making the link...",
  noLinkYet: "No link yet",

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
  view: "View",
  n1Hour: "1 hour",
  n1Day: "1 day",
  n7Days: "7 days",
  anyoneWithTheLink: "Anyone with the link",
  peopleAlreadyInvited: "People already invited",
  peopleIn: "People in {organization}",
  peopleInYourOrganization: "People in your organization",
  enterHowLongTheLinkShouldWork: "Enter how long the link should work, as a whole number.",
  chooseHowLongTheLinkShouldWork: "Choose how long the link should work.",
  linksHereLastAtMost: "Links here last at most {lifetime}.",
  anyoneWithThisLinkCanDownload: "Anyone with this link can download {what} {until}.",
  anyoneWithThisLinkCanEdit: "Anyone with this link can open and change {what} {until}.",
  anyoneWithThisLinkCanView: "Anyone with this link can open {what} {until}.",
  anyoneWithOneOfTheseLinksCanDownload:
    "Anyone with one of these links can download {what} {until}.",
  anyoneWithOneOfTheseLinksCanEdit:
    "Anyone with one of these links can open and change {what} {until}.",
  anyoneWithOneOfTheseLinksCanView: "Anyone with one of these links can open {what} {until}.",
  peopleInWithThisLinkCanDownload:
    "People in {organization} with this link can download {what} {until}.",
  peopleInWithThisLinkCanEdit:
    "People in {organization} with this link can open and change {what} {until}.",
  peopleInWithThisLinkCanView: "People in {organization} with this link can open {what} {until}.",
  peopleInWithOneOfTheseLinksCanDownload:
    "People in {organization} with one of these links can download {what} {until}.",
  peopleInWithOneOfTheseLinksCanEdit:
    "People in {organization} with one of these links can open and change {what} {until}.",
  peopleInWithOneOfTheseLinksCanView:
    "People in {organization} with one of these links can open {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanDownload:
    "People in your organization with this link can download {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanEdit:
    "People in your organization with this link can open and change {what} {until}.",
  peopleInYourOrganizationWithThisLinkCanView:
    "People in your organization with this link can open {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanDownload:
    "People in your organization with one of these links can download {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanEdit:
    "People in your organization with one of these links can open and change {what} {until}.",
  peopleInYourOrganizationWithOneOfTheseLinksCanView:
    "People in your organization with one of these links can open {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanDownload:
    "People already invited who have this link can download {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanEdit:
    "People already invited who have this link can open and change {what} {until}.",
  peopleAlreadyInvitedWhoHaveThisLinkCanView:
    "People already invited who have this link can open {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanDownload:
    "People already invited who have one of these links can download {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanEdit:
    "People already invited who have one of these links can open and change {what} {until}.",
  peopleAlreadyInvitedWhoHaveOneOfTheseLinksCanView:
    "People already invited who have one of these links can open {what} {until}.",
  statementWithPassword: "{statement} It asks for the password first.",
  theItemItIsFor: "the item it is for",
  theFileItIsFor: "the file it is for",
  theFolderAndEverythingInIt: "the folder and everything in it",
  theFile: "the file",
  untilDeadline: "until {deadline}",
  untilYouStopSharing: "until you stop sharing",
  forAsLongAsItIsThere: "for as long as it is there",
  sharedBefore: "Shared before: {statement}",
  notWhatWasAsked: "Not what was asked: {statement}",
} as const satisfies MessageArea;
