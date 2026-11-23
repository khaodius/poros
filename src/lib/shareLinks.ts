// Share links: what a pane's menu offers, the choices a service allows, and the sentence that
// says what a link lets people do.

import { pluralize } from "./format";
import type {
  FileEntry,
  LinkKind,
  ShareAudience,
  ShareCapabilities,
  ShareLink,
  ShareOptions,
} from "./types";

/** How a server pane's menu shows sharing: offered, or shown with why it is not. */
export interface ShareMenuEntry {
  label: string;
  disabled: boolean;
}

type Shareable = Pick<FileEntry, "kind" | "linkTarget">;

export const isFolderLike = (entry: Shareable) =>
  entry.kind === "dir" || (entry.kind === "symlink" && entry.linkTarget === "dir");

/**
 * The menu entry for sharing `entries` from a server pane, or `null` with nothing selected.
 * Like Properties, the entry stays where it cannot be used and says why.
 */
export function shareMenuEntry(
  entries: readonly Shareable[],
  sharing: ShareCapabilities | undefined,
  inVault: boolean,
): ShareMenuEntry | null {
  if (entries.length === 0) return null;
  // The server holds a vault's files encrypted, so a link to one would be unreadable.
  if (inVault) return { label: "Share link (not in a vault)", disabled: true };
  if (!sharing) return { label: "Share link (cloud storage only)", disabled: true };
  if (!sharing.folders && entries.some(isFolderLike)) {
    return { label: "Share link (files only)", disabled: true };
  }
  return { label: "Share link...", disabled: false };
}

export const KIND_LABELS: Record<LinkKind, string> = {
  view: "View",
  edit: "Edit",
  download: "Download",
};

export function audienceLabel(audience: ShareAudience, organization?: string): string {
  if (audience === "anyone") return "Anyone with the link";
  if (audience === "invited") return "People already invited";
  return organization ? `People in ${organization}` : "People in your organization";
}

export type LifetimeUnit = "minutes" | "hours" | "days";

export const LIFETIME_UNITS: LifetimeUnit[] = ["minutes", "hours", "days"];

const UNIT_SECONDS: Record<LifetimeUnit, number> = { minutes: 60, hours: 3600, days: 86_400 };

export interface LifetimePreset {
  label: string;
  seconds: number;
}

const LIFETIME_PRESETS: LifetimePreset[] = [
  { label: "1 hour", seconds: 3600 },
  { label: "1 day", seconds: 86_400 },
  { label: "7 days", seconds: 604_800 },
];

/** The preset lifetimes the service takes. */
export function lifetimePresets(sharing: ShareCapabilities): LifetimePreset[] {
  const longest = sharing.maxLifetimeSecs;
  return LIFETIME_PRESETS.filter((preset) => longest === undefined || preset.seconds <= longest);
}

/** "7 days", "90 minutes": the largest unit that leaves no remainder. */
export function lifetimeText(seconds: number): string {
  if (seconds % 86_400 === 0) return pluralize(seconds / 86_400, "day");
  if (seconds % 3600 === 0) return pluralize(seconds / 3600, "hour");
  if (seconds % 60 === 0) return pluralize(seconds / 60, "minute");
  return pluralize(seconds, "second");
}

/** A preset's seconds, a lifetime typed by hand, or a link without an end. */
export type LifetimeChoice = number | "custom" | "never";

/** What the dialog's form holds. */
export interface ShareDraft {
  kind: LinkKind;
  audience: ShareAudience;
  lifetime: LifetimeChoice;
  customAmount: string;
  customUnit: LifetimeUnit;
  password: string;
}

export function initialDraft(sharing: ShareCapabilities): ShareDraft {
  return {
    kind: sharing.kinds[0] ?? "view",
    audience: sharing.audiences[0] ?? "anyone",
    lifetime: sharing.expiry === "required" ? LIFETIME_PRESETS[0].seconds : "never",
    customAmount: "12",
    customUnit: "hours",
    password: "",
  };
}

/** Seconds for a lifetime typed by hand; `null` unless it is a whole number above zero. */
export function customLifetime(amount: string, unit: LifetimeUnit): number | null {
  const typed = amount.trim();
  if (!/^\d{1,6}$/.test(typed)) return null;
  const count = Number(typed);
  return count > 0 ? count * UNIT_SECONDS[unit] : null;
}

/**
 * The lifetime a draft asks for in seconds: `undefined` for a link without an end, `null` while
 * the one typed by hand is not a number yet.
 */
export function draftLifetime(
  draft: ShareDraft,
  sharing: ShareCapabilities,
): number | undefined | null {
  if (sharing.expiry === "never" || draft.lifetime === "never") return undefined;
  if (draft.lifetime === "custom") return customLifetime(draft.customAmount, draft.customUnit);
  return draft.lifetime;
}

/** Only links that anyone can open take a password. */
export function takesPassword(draft: ShareDraft, sharing: ShareCapabilities): boolean {
  return sharing.password && draft.audience === "anyone";
}

/** Why the draft cannot be sent yet, in words for the dialog; `null` when it can. */
export function draftProblem(draft: ShareDraft, sharing: ShareCapabilities): string | null {
  const lifetime = draftLifetime(draft, sharing);
  if (lifetime === null) return "Enter how long the link should work, as a whole number.";
  if (lifetime === undefined) {
    return sharing.expiry === "required" ? "Choose how long the link should work." : null;
  }
  if (sharing.maxLifetimeSecs !== undefined && lifetime > sharing.maxLifetimeSecs) {
    return `Links here last at most ${lifetimeText(sharing.maxLifetimeSecs)}.`;
  }
  return null;
}

/** The options to send for a draft; `null` while something in it is wrong. */
export function draftOptions(draft: ShareDraft, sharing: ShareCapabilities): ShareOptions | null {
  if (draftProblem(draft, sharing) !== null) return null;
  const lifetime = draftLifetime(draft, sharing);
  const password = takesPassword(draft, sharing) ? draft.password : "";
  return {
    kind: draft.kind,
    audience: draft.audience,
    ...(typeof lifetime === "number" ? { lifetimeSecs: lifetime } : {}),
    ...(password ? { password } : {}),
  };
}

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

function twoDigits(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/** Local time as `14 May, 18:30`, with the year when it is not the current one. */
export function formatDeadline(epochSeconds: number, nowMillis: number): string {
  const date = new Date(epochSeconds * 1000);
  const year =
    date.getFullYear() === new Date(nowMillis).getFullYear() ? "" : ` ${date.getFullYear()}`;
  const time = `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}`;
  return `${date.getDate()} ${MONTHS[date.getMonth()]}${year}, ${time}`;
}

export interface StatementSubject {
  kind: LinkKind;
  audience: ShareAudience;
  /** When the link ends, in seconds since the Unix epoch; absent for one without an end. */
  expiresAt?: number;
  /** The link can be removed, which is how one without an end stops. */
  revocable: boolean;
  organization?: string;
  /** How many items get a link each. */
  count: number;
  /** A folder is among them. */
  folder: boolean;
  /** Opening the link asks for a password. */
  hasPassword?: boolean;
  nowMillis: number;
}

/** In plain words: `Anyone with this link can download the file until 14 May, 18:30.` */
export function linkStatement(subject: StatementSubject): string {
  const several = subject.count > 1;
  const links = several ? "one of these links" : "this link";
  const who =
    subject.audience === "anyone"
      ? `Anyone with ${links}`
      : subject.audience === "organization"
        ? `People in ${subject.organization ?? "your organization"} with ${links}`
        : `People already invited who have ${links}`;
  const action =
    subject.kind === "download"
      ? "can download"
      : subject.kind === "edit"
        ? "can open and change"
        : "can open";
  const item = subject.folder ? "folder" : "file";
  const what = several
    ? `the ${subject.folder ? "item" : "file"} it is for`
    : subject.folder
      ? "the folder and everything in it"
      : `the ${item}`;
  const until =
    subject.expiresAt !== undefined
      ? `until ${formatDeadline(subject.expiresAt, subject.nowMillis)}`
      : subject.revocable
        ? "until you stop sharing"
        : "for as long as it is there";
  const password = subject.hasPassword ? " It asks for the password first." : "";
  return `${who} ${action} ${what} ${until}.${password}`;
}

/** `3 files`, `1 folder`, `2 files and 1 folder`. */
export function selectionSummary(entries: readonly Pick<FileEntry, "kind">[]): string {
  const folders = entries.filter((entry) => entry.kind === "dir").length;
  const files = entries.length - folders;
  return [files > 0 && pluralize(files, "file"), folders > 0 && pluralize(folders, "folder")]
    .filter(Boolean)
    .join(" and ");
}

/** The addresses of links, one per line, for the clipboard. */
export function linksText(links: readonly Pick<ShareLink, "url">[]): string {
  return links.map((link) => link.url).join("\n");
}

/**
 * A list of an item's links with `link` first, in place of the one it replaces, if any. Links
 * the service keeps are told apart by id, since Google Drive gives every link of an item the
 * same address; signed links have no id and differ by address.
 */
export function withLink(links: readonly ShareLink[], link: ShareLink): ShareLink[] {
  return [link, ...links.filter((other) => !sameLink(other, link))];
}

/** Whether two answers describe the same link: by id where the service keeps links. */
export function sameLink(first: Pick<ShareLink, "id" | "url">, second: ShareLink): boolean {
  return second.id !== "" ? first.id === second.id : first.url === second.url;
}

/** A key for a link among an item's links. */
export function linkKey(link: Pick<ShareLink, "id" | "url">): string {
  return link.id !== "" ? `id:${link.id}` : `url:${link.url}`;
}

/** The links an item has: those already shown first, then the ones read since that are new. */
export function mergeLinks(shown: readonly ShareLink[], read: readonly ShareLink[]): ShareLink[] {
  return [...shown, ...read.filter((link) => !shown.some((other) => sameLink(other, link)))];
}

/** Runs `work` on every item, at most `limit` at a time, so many files do not flood a service. */
export async function forEachLimited<T>(
  items: readonly T[],
  limit: number,
  work: (item: T, index: number) => Promise<void>,
): Promise<void> {
  let next = 0;
  const worker = async () => {
    while (next < items.length) {
      const index = next;
      next += 1;
      await work(items[index], index);
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
}
