import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { capsOf } from "../lib/previewPlan";
import { DEFAULT_FILE_PREVIEW } from "../lib/previewSettings";
import type { FileEntry, SessionInfo } from "../lib/types";
import {
  cachedFileView,
  cachedThumbnail,
  forgetSessionPreviews,
  readFileView,
  requestThumbnail,
  type PreviewPlace,
} from "./previewReads";
import { useSessionStore } from "./sessionStore";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

const caps = capsOf(DEFAULT_FILE_PREVIEW);
const LISTED_AT = 1_700_000_000;
const never = new AbortController().signal;

function entry(name: string, contents: Uint8Array): FileEntry {
  return {
    name,
    path: `/vault/${name}`,
    kind: "file",
    size: contents.length,
    modified: LISTED_AT,
    permissions: null,
    owner: null,
    group: null,
    hidden: false,
  };
}

function place(sessionId: string): PreviewPlace {
  return {
    connection: `remote:${sessionId}`,
    sessionId,
    storage: "server",
    exactTimes: true,
    inVault: true,
    sizes: "listed",
  };
}

/** The backend's answer to a read: the file's size and time, then its bytes. */
function frame(contents: Uint8Array): ArrayBuffer {
  const bytes = new Uint8Array(16 + contents.length);
  const header = new DataView(bytes.buffer);
  header.setBigUint64(0, BigInt(contents.length), true);
  header.setBigInt64(8, BigInt(LISTED_AT * 1000), true);
  bytes.set(contents, 16);
  return bytes.buffer;
}

/** A picture of `width` by `height` pixels, as far as its header says. */
function png(width: number, height: number): Uint8Array {
  const bytes = new Uint8Array(64);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  const header = new DataView(bytes.buffer);
  header.setUint32(16, width);
  header.setUint32(20, height);
  return bytes;
}

const diary = new TextEncoder().encode("Dear diary, the password is under the mat.");

/** Answers every read with `contents`. */
function serve(contents: Uint8Array) {
  invoke.mockImplementation(async (command: string) =>
    command === "file_preview_read" ? frame(contents) : undefined,
  );
}

function know(info: SessionInfo) {
  useSessionStore.setState({
    sessions: { [info.id]: { info, profile: null, status: "connected" } },
  });
}

const vault: SessionInfo = {
  id: "vault",
  label: "Private on deploy@web.example.com",
  host: "web.example.com",
  port: 22,
  username: "deploy",
  home: "/vault",
  initialPath: "/vault",
  protocol: "cryptomator",
  vault: { parentSessionId: "server", root: "/vault", name: "Private" },
};

beforeEach(() => {
  invoke.mockReset();
  useSessionStore.setState({ sessions: {} });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  forgetSessionPreviews("vault");
  forgetSessionPreviews("vault2");
});

describe("what the previews keep of a session", () => {
  it("is forgotten as the session closes, and only that session's", async () => {
    serve(diary);
    const file = entry("diary.txt", diary);
    await readFileView(place("vault"), file, caps, false, never);
    // A session whose name starts with the other's.
    await readFileView(place("vault2"), file, caps, false, never);
    expect(cachedFileView(place("vault"), file, caps, false)).toMatchObject({ view: "text" });

    forgetSessionPreviews("vault");
    expect(cachedFileView(place("vault"), file, caps, false)).toBeUndefined();
    expect(cachedFileView(place("vault2"), file, caps, false)).toMatchObject({ view: "text" });
  });

  it("shows nothing kept from before for a remembered listing, which a read must confirm", async () => {
    serve(diary);
    const file = entry("diary.txt", diary);
    await readFileView(place("vault"), file, caps, false, never);
    const remembered: PreviewPlace = { ...place("vault"), sizes: "remembered" };
    expect(cachedFileView(remembered, file, caps, false)).toBeUndefined();
    expect(cachedFileView(place("vault"), file, caps, false)).toMatchObject({ view: "text" });
  });

  it("keeps nothing of a read that ends after the session closed", async () => {
    let answer: (reply: ArrayBuffer) => void = () => undefined;
    invoke.mockImplementation(
      (command: string) =>
        new Promise((resolve) => {
          if (command === "file_preview_read") answer = resolve;
          else resolve(undefined);
        }),
    );
    const file = entry("diary.txt", diary);
    const reading = readFileView(place("vault"), file, caps, false, never);
    await vi.waitFor(() => expect(invoke).toHaveBeenCalled());
    forgetSessionPreviews("vault");
    answer(frame(diary));
    await reading;
    expect(cachedFileView(place("vault"), file, caps, false)).toBeUndefined();
  });

  it("lets the thumbnails' pictures go", async () => {
    const picture = png(4, 4);
    serve(picture);
    vi.stubGlobal("window", { devicePixelRatio: 1 });
    vi.stubGlobal("createImageBitmap", async () => ({ width: 4, height: 4, close: () => {} }));
    vi.stubGlobal(
      "OffscreenCanvas",
      class {
        getContext() {
          return { drawImage: () => {} };
        }
        async convertToBlob() {
          return new Blob(["small"]);
        }
      },
    );
    const revoked = vi.spyOn(URL, "revokeObjectURL");
    const file = entry("face.png", picture);
    const thumbnail = await requestThumbnail(place("vault"), file, 96, caps);
    expect(thumbnail.url).not.toBeNull();
    expect(cachedThumbnail(place("vault"), file, 96)).toBe(thumbnail);

    forgetSessionPreviews("vault");
    expect(revoked).toHaveBeenCalledWith(thumbnail.url);
    expect(cachedThumbnail(place("vault"), file, 96)).toBeUndefined();
  });

  it("is forgotten when the vault is locked or the connection is lost", async () => {
    serve(diary);
    const file = entry("diary.txt", diary);
    know(vault);
    await readFileView(place("vault"), file, caps, false, never);
    useSessionStore.getState().markLost("vault", "The server closed the connection");
    expect(cachedFileView(place("vault"), file, caps, false)).toBeUndefined();

    know(vault);
    await readFileView(place("vault"), file, caps, false, never);
    await useSessionStore.getState().disconnect("vault");
    expect(cachedFileView(place("vault"), file, caps, false)).toBeUndefined();
    expect(invoke).toHaveBeenCalledWith("disconnect", { sessionId: "vault" });
  });
});
