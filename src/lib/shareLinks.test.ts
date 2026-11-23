import { describe, expect, it } from "vitest";
import {
  audienceLabel,
  customLifetime,
  draftLifetime,
  draftOptions,
  draftProblem,
  forEachLimited,
  formatDeadline,
  initialDraft,
  lifetimePresets,
  lifetimeText,
  linkKey,
  linkStatement,
  linksText,
  mergeLinks,
  selectionSummary,
  shareMenuEntry,
  takesPassword,
  withLink,
  type StatementSubject,
} from "./shareLinks";
import type { ShareCapabilities, ShareLink } from "./types";

/** S3 and Backblaze B2: signed links for files, which always end. */
const signed: ShareCapabilities = {
  kinds: ["view", "download"],
  audiences: ["anyone"],
  expiry: "required",
  maxLifetimeSecs: 604_800,
  password: false,
  revocable: false,
  folders: false,
};

/** OneDrive on a work account: links kept with the item. */
const kept: ShareCapabilities = {
  kinds: ["view", "edit"],
  audiences: ["anyone", "organization"],
  expiry: "optional",
  password: true,
  revocable: true,
  folders: true,
};

/** Google Drive on a Workspace domain. */
const drive: ShareCapabilities = {
  kinds: ["view", "edit", "download"],
  audiences: ["anyone", "organization"],
  expiry: "never",
  password: false,
  revocable: true,
  folders: true,
  organization: "example.com",
};

function link(id: string, url: string): ShareLink {
  return {
    id,
    url,
    kind: "view",
    audience: "anyone",
    hasPassword: false,
    revocable: id !== "",
    notes: [],
  };
}

/** Local time, so the expectations hold in any time zone. */
function localSeconds(year: number, month: number, day: number, hour: number, minute: number) {
  return new Date(year, month - 1, day, hour, minute).getTime() / 1000;
}

describe("shareMenuEntry", () => {
  const file = { kind: "file" } as const;
  const folder = { kind: "dir" } as const;
  const folderLink = { kind: "symlink", linkTarget: "dir" } as const;

  it("is absent with nothing selected", () => {
    expect(shareMenuEntry([], signed, false)).toBeNull();
  });

  it("offers files, and folders where the service shares them", () => {
    expect(shareMenuEntry([file], signed, false)).toEqual({
      label: "Share link...",
      disabled: false,
    });
    expect(shareMenuEntry([file, file], signed, false)?.disabled).toBe(false);
    expect(shareMenuEntry([folder], kept, false)).toEqual({
      label: "Share link...",
      disabled: false,
    });
  });

  it("says why a folder cannot be shared where only files can", () => {
    expect(shareMenuEntry([folder], signed, false)).toEqual({
      label: "Share link (files only)",
      disabled: true,
    });
    expect(shareMenuEntry([file, folder], signed, false)?.disabled).toBe(true);
    expect(shareMenuEntry([folderLink], signed, false)?.disabled).toBe(true);
  });

  it("says why a connection without share links cannot share, as Properties does", () => {
    expect(shareMenuEntry([file], undefined, false)).toEqual({
      label: "Share link (cloud storage only)",
      disabled: true,
    });
  });

  it("says why nothing in a vault can be shared", () => {
    expect(shareMenuEntry([file], undefined, true)).toEqual({
      label: "Share link (not in a vault)",
      disabled: true,
    });
  });
});

describe("lifetimes", () => {
  it("offers the presets the service takes", () => {
    expect(lifetimePresets(signed).map((preset) => preset.label)).toEqual([
      "1 hour",
      "1 day",
      "7 days",
    ]);
    expect(lifetimePresets({ ...signed, maxLifetimeSecs: 86_400 })).toHaveLength(2);
    expect(lifetimePresets(kept)).toHaveLength(3);
  });

  it("reads a lifetime typed by hand", () => {
    expect(customLifetime("12", "hours")).toBe(43_200);
    expect(customLifetime(" 90 ", "minutes")).toBe(5400);
    expect(customLifetime("3", "days")).toBe(259_200);
    for (const refused of ["", "0", "-1", "1.5", "two", "1e3", "1234567"]) {
      expect(customLifetime(refused, "hours")).toBeNull();
    }
  });

  it("names a lifetime in its largest whole unit", () => {
    expect(lifetimeText(604_800)).toBe("7 days");
    expect(lifetimeText(86_400)).toBe("1 day");
    expect(lifetimeText(5400)).toBe("90 minutes");
    expect(lifetimeText(45)).toBe("45 seconds");
  });
});

describe("drafts", () => {
  it("start with an hour where links must end and no end where they need not", () => {
    expect(initialDraft(signed)).toMatchObject({
      kind: "view",
      audience: "anyone",
      lifetime: 3600,
    });
    expect(initialDraft(kept).lifetime).toBe("never");
    expect(draftLifetime(initialDraft(drive), drive)).toBeUndefined();
  });

  it("become the options the backend takes", () => {
    expect(draftOptions(initialDraft(signed), signed)).toEqual({
      kind: "view",
      audience: "anyone",
      lifetimeSecs: 3600,
    });
    expect(draftOptions(initialDraft(drive), drive)).toEqual({ kind: "view", audience: "anyone" });
    const custom = {
      ...initialDraft(kept),
      kind: "edit" as const,
      lifetime: "custom" as const,
      customAmount: "3",
      customUnit: "days" as const,
      password: "Open-Sesame1",
    };
    expect(draftOptions(custom, kept)).toEqual({
      kind: "edit",
      audience: "anyone",
      lifetimeSecs: 259_200,
      password: "Open-Sesame1",
    });
  });

  it("leave the password out of links that are not for anyone", () => {
    const narrow = { ...initialDraft(kept), audience: "organization" as const, password: "secret" };
    expect(takesPassword(narrow, kept)).toBe(false);
    expect(draftOptions(narrow, kept)).toEqual({ kind: "view", audience: "organization" });
    expect(takesPassword(initialDraft(signed), signed)).toBe(false);
  });

  it("say what is wrong with a lifetime", () => {
    const typed = (customAmount: string, customUnit: "minutes" | "hours" | "days") => ({
      ...initialDraft(signed),
      lifetime: "custom" as const,
      customAmount,
      customUnit,
    });
    expect(draftProblem(typed("6", "days"), signed)).toBeNull();
    expect(draftProblem(typed("7", "days"), signed)).toBeNull();
    expect(draftProblem(typed("8", "days"), signed)).toBe("Links here last at most 7 days.");
    expect(draftProblem(typed("", "days"), signed)).toBe(
      "Enter how long the link should work, as a whole number.",
    );
    expect(draftOptions(typed("8", "days"), signed)).toBeNull();
    expect(draftProblem({ ...initialDraft(signed), lifetime: "never" }, signed)).toBe(
      "Choose how long the link should work.",
    );
    expect(draftProblem(typed("400", "days"), kept)).toBeNull();
    // A service whose links never end ignores what the form held before.
    expect(draftProblem(typed("", "days"), drive)).toBeNull();
  });
});

describe("linkStatement", () => {
  const now = new Date(2024, 4, 14, 17, 30).getTime();
  const subject: StatementSubject = {
    kind: "download",
    audience: "anyone",
    expiresAt: localSeconds(2024, 5, 14, 18, 30),
    revocable: false,
    count: 1,
    folder: false,
    nowMillis: now,
  };

  it("says who can do what until when", () => {
    expect(linkStatement(subject)).toBe(
      "Anyone with this link can download the file until 14 May, 18:30.",
    );
    expect(linkStatement({ ...subject, kind: "view" })).toBe(
      "Anyone with this link can open the file until 14 May, 18:30.",
    );
  });

  it("names the year of an end in another year", () => {
    expect(formatDeadline(localSeconds(2025, 1, 3, 9, 5), now)).toBe("3 January 2025, 09:05");
    expect(formatDeadline(localSeconds(2024, 12, 31, 23, 59), now)).toBe("31 December, 23:59");
  });

  it("says how a link without an end stops", () => {
    const lasting = { ...subject, kind: "view" as const, expiresAt: undefined };
    expect(linkStatement({ ...lasting, revocable: true })).toBe(
      "Anyone with this link can open the file until you stop sharing.",
    );
    expect(linkStatement(lasting)).toBe(
      "Anyone with this link can open the file for as long as it is there.",
    );
  });

  it("names the audience, folders and several items", () => {
    expect(
      linkStatement({
        ...subject,
        kind: "edit",
        audience: "organization",
        organization: "example.com",
        expiresAt: undefined,
        revocable: true,
        folder: true,
      }),
    ).toBe(
      "People in example.com with this link can open and change the folder and everything in it until you stop sharing.",
    );
    expect(linkStatement({ ...subject, audience: "organization" })).toContain(
      "People in your organization with this link",
    );
    expect(linkStatement({ ...subject, audience: "invited", kind: "view" })).toBe(
      "People already invited who have this link can open the file until 14 May, 18:30.",
    );
    expect(linkStatement({ ...subject, count: 3 })).toBe(
      "Anyone with one of these links can download the file it is for until 14 May, 18:30.",
    );
    expect(linkStatement({ ...subject, count: 2, folder: true, kind: "view" })).toContain(
      "can open the item it is for",
    );
  });

  it("says when the link asks for a password", () => {
    expect(linkStatement({ ...subject, kind: "view", hasPassword: true })).toBe(
      "Anyone with this link can open the file until 14 May, 18:30. It asks for the password first.",
    );
  });
});

describe("labels and lists", () => {
  it("names audiences", () => {
    expect(audienceLabel("anyone")).toBe("Anyone with the link");
    expect(audienceLabel("organization")).toBe("People in your organization");
    expect(audienceLabel("organization", "example.com")).toBe("People in example.com");
    expect(audienceLabel("invited")).toBe("People already invited");
  });

  it("sums up a selection", () => {
    const file = { kind: "file" } as const;
    const folder = { kind: "dir" } as const;
    expect(selectionSummary([file, file, file])).toBe("3 files");
    expect(selectionSummary([folder])).toBe("1 folder");
    expect(selectionSummary([file, folder, file])).toBe("2 files and 1 folder");
  });

  it("puts links on the clipboard one per line", () => {
    expect(linksText([link("", "https://a.example/1"), link("", "https://a.example/2")])).toBe(
      "https://a.example/1\nhttps://a.example/2",
    );
    expect(linksText([])).toBe("");
  });

  it("puts a new link first, in place of the one it changes", () => {
    const first = link("anyoneWithLink", "https://drive.example/file");
    const other = link("domain", "https://drive.example/file?domain");
    const changed = { ...first, kind: "edit" as const };
    expect(withLink([other, first], changed)).toEqual([changed, other]);
    // Signed links have no id: each is its own, unless it is the very same address.
    const signedOnce = link("", "https://s3.example/a?X-Amz-Signature=1");
    const signedAgain = link("", "https://s3.example/a?X-Amz-Signature=2");
    expect(withLink([signedOnce], signedAgain)).toEqual([signedAgain, signedOnce]);
    expect(withLink([signedOnce], signedOnce)).toEqual([signedOnce]);
  });

  it("keeps another link of the same address, as Google Drive gives every link of an item", () => {
    const anyone = link("anyoneWithLink", "https://drive.example/file");
    const domain = link("11112222", "https://drive.example/file");
    expect(withLink([domain], anyone)).toEqual([anyone, domain]);
    expect(linkKey(anyone)).not.toBe(linkKey(domain));
    expect(linkKey(link("", "https://s3.example/a"))).toBe("url:https://s3.example/a");
  });

  it("adds the links read from the service after the ones already shown", () => {
    const made = link("", "https://s3.example/a?X-Amz-Signature=1");
    const kept = link("anyoneWithLink", "https://drive.example/file");
    const changed = { ...kept, kind: "edit" as const };
    expect(mergeLinks([made], [kept])).toEqual([made, kept]);
    // A link made while the list was read stays as it was made.
    expect(mergeLinks([changed], [kept])).toEqual([changed]);
    expect(mergeLinks([], [])).toEqual([]);
  });
});

describe("forEachLimited", () => {
  it("works on every item, never more than the limit at once", async () => {
    let running = 0;
    let most = 0;
    const done: number[] = [];
    await forEachLimited([1, 2, 3, 4, 5, 6, 7], 3, async (item, index) => {
      running += 1;
      most = Math.max(most, running);
      await new Promise((resolve) => setTimeout(resolve, item % 2 === 0 ? 5 : 1));
      expect(index).toBe(item - 1);
      done.push(item);
      running -= 1;
    });
    expect([...done].sort()).toEqual([1, 2, 3, 4, 5, 6, 7]);
    expect(most).toBe(3);
    await forEachLimited([], 3, () => Promise.reject(new Error("never called")));
  });
});
