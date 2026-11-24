// Share links: what a pane's menu offers, the choices a service allows, and the sentence that
// says what a link lets people do.

import type {
  FileEntry,
  LinkKind,
  ShareAudience,
  ShareCapabilities,
  ShareLink,
  ShareOptions,
} from "./types";
import { dateWordFormat, formatList, t } from "../i18n";

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
  if (inVault) return { label: t("share.shareLinkNotInA"), disabled: true };
  if (!sharing) return { label: t("share.shareLinkCloudStorageOnly"), disabled: true };
  if (!sharing.folders && entries.some(isFolderLike)) {
    return { label: t("share.shareLinkFilesOnly"), disabled: true };
  }
  return { label: t("share.shareLink"), disabled: false };
}

export const KIND_LABELS: Record<LinkKind, string> = {
  get view() {
    return t("share.view");
  },
  get edit() {
    return t("common.edit");
  },
  get download() {
    return t("common.download");
  },
};

export function audienceLabel(audience: ShareAudience, organization?: string): string {
  if (audience === "anyone") return t("share.anyoneWithTheLink");
  if (audience === "invited") return t("share.peopleAlreadyInvited");
  return organization ? t("share.peopleIn", { organization }) : t("share.peopleInYourOrganization");
}

export type LifetimeUnit = "minutes" | "hours" | "days";

export const LIFETIME_UNITS: LifetimeUnit[] = ["minutes", "hours", "days"];

const UNIT_SECONDS: Record<LifetimeUnit, number> = { minutes: 60, hours: 3600, days: 86_400 };

export interface LifetimePreset {
  label: string;
  seconds: number;
}

const LIFETIME_PRESETS: LifetimePreset[] = [
  {
    get label() {
      return t("share.n1Hour");
    },
    seconds: 3600,
  },
  {
    get label() {
      return t("share.n1Day");
    },
    seconds: 86_400,
  },
  {
    get label() {
      return t("share.n7Days");
    },
    seconds: 604_800,
  },
];

/** The preset lifetimes the service takes. */
export function lifetimePresets(sharing: ShareCapabilities): LifetimePreset[] {
  const longest = sharing.maxLifetimeSecs;
  return LIFETIME_PRESETS.filter((preset) => longest === undefined || preset.seconds <= longest);
}

/** "7 days", "90 minutes": the largest unit that leaves no remainder. */
export function lifetimeText(seconds: number): string {
  if (seconds % 86_400 === 0) return t("share.days", { count: seconds / 86_400 });
  if (seconds % 3600 === 0) return t("share.hours", { count: seconds / 3600 });
  if (seconds % 60 === 0) return t("share.minutes", { count: seconds / 60 });
  return t("share.seconds", { count: seconds });
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
  if (lifetime === null) return t("share.enterHowLongTheLinkShouldWork");
  if (lifetime === undefined) {
    return sharing.expiry === "required" ? t("share.chooseHowLongTheLinkShouldWork") : null;
  }
  if (sharing.maxLifetimeSecs !== undefined && lifetime > sharing.maxLifetimeSecs) {
    return t("share.linksHereLastAtMost", { lifetime: lifetimeText(sharing.maxLifetimeSecs) });
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

function twoDigits(value: number): string {
  return value < 10 ? `0${value}` : String(value);
}

/** Local time as `14 May, 18:30`, with the year when it is not the current one. */
export function formatDeadline(epochSeconds: number, nowMillis: number): string {
  const date = new Date(epochSeconds * 1000);
  const parts = {
    day: String(date.getDate()),
    month: dateWordFormat({ month: "long" }).format(date),
    time: `${twoDigits(date.getHours())}:${twoDigits(date.getMinutes())}`,
  };
  return date.getFullYear() === new Date(nowMillis).getFullYear()
    ? t("share.deadline", parts)
    : t("share.deadlineWithYear", { ...parts, year: String(date.getFullYear()) });
}

const STATEMENTS = {
  download: "share.statementDownload",
  edit: "share.statementEdit",
  view: "share.statementView",
} as const;

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
  const { organization } = subject;
  const who =
    subject.audience === "anyone"
      ? several
        ? t("share.anyoneWithOneOfTheseLinks")
        : t("share.anyoneWithThisLink")
      : subject.audience === "organization"
        ? organization
          ? several
            ? t("share.peopleInWithOneOfTheseLinks", { organization })
            : t("share.peopleInWithThisLink", { organization })
          : several
            ? t("share.peopleInYourOrganizationWithOneOfTheseLinks")
            : t("share.peopleInYourOrganizationWithThisLink")
        : several
          ? t("share.peopleAlreadyInvitedWhoHaveOneOfTheseLinks")
          : t("share.peopleAlreadyInvitedWhoHaveThisLink");
  const what = several
    ? subject.folder
      ? t("share.theItemItIsFor")
      : t("share.theFileItIsFor")
    : subject.folder
      ? t("share.theFolderAndEverythingInIt")
      : t("share.theFile");
  const until =
    subject.expiresAt !== undefined
      ? t("share.untilDeadline", {
          deadline: formatDeadline(subject.expiresAt, subject.nowMillis),
        })
      : subject.revocable
        ? t("share.untilYouStopSharing")
        : t("share.forAsLongAsItIsThere");
  const statement = t(STATEMENTS[subject.kind], { who, what, until });
  return subject.hasPassword ? t("share.statementWithPassword", { statement }) : statement;
}

/** `3 files`, `1 folder`, `2 files and 1 folder`. */
export function selectionSummary(entries: readonly Pick<FileEntry, "kind">[]): string {
  const folders = entries.filter((entry) => entry.kind === "dir").length;
  const files = entries.length - folders;
  const parts = [
    files > 0 && t("share.files", { count: files }),
    folders > 0 && t("share.folders", { count: folders }),
  ].filter((part) => part !== false);
  return formatList(parts);
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

/** What a Create asked for, and when, which the links it made are checked against. */
export interface ShareRequest {
  options: ShareOptions;
  /** Seconds since the Unix epoch. */
  at: number;
}

/** A request for `options`, made now. */
export function requestNow(options: ShareOptions): ShareRequest {
  return { options, at: Math.floor(Date.now() / 1000) };
}

/**
 * How far a link's end may be from the one asked for: the time the request took, and the
 * difference between this computer's clock and the service's.
 */
export const END_TOLERANCE_SECS = 120;

/**
 * Whether `link` is what `request` asked for: what it allows, for whom, until when, and
 * whether it asks for a password.
 */
export function isAsked(link: ShareLink, request: ShareRequest): boolean {
  const { options } = request;
  if (link.kind !== options.kind || link.audience !== options.audience) return false;
  if (link.hasPassword !== Boolean(options.password)) return false;
  if (options.lifetimeSecs === undefined) return link.expiresAt === undefined;
  if (link.expiresAt === undefined) return false;
  return Math.abs(link.expiresAt - (request.at + options.lifetimeSecs)) <= END_TOLERANCE_SECS;
}

/** Whether two sets of options ask for the same link. */
export function sameOptions(first: ShareOptions, second: ShareOptions): boolean {
  return (
    first.kind === second.kind &&
    first.audience === second.audience &&
    first.lifetimeSecs === second.lifetimeSecs &&
    (first.password ?? "") === (second.password ?? "")
  );
}

/**
 * What a row among several says about its link, where the statement above the rows, which
 * describes the draft, does not describe it: `null` for a link the last Create made as asked
 * while the draft still asks for the same. `made` is the request that made the link, or
 * `null` for a link the item had before; `statement` describes the link itself.
 */
export function batchLinkLabel(
  link: ShareLink,
  made: ShareRequest | null,
  draft: ShareOptions | null,
  statement: string,
): string | null {
  if (made === null) return t("share.sharedBefore", { statement });
  if (!isAsked(link, made)) return t("share.notWhatWasAsked", { statement });
  if (draft === null || !sameOptions(draft, made.options)) return statement;
  return null;
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
