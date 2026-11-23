// Reads a file for the preview pane, step by step as the plan says, and turns what came into
// what the pane shows. The reading itself is handed in, so this runs the same over any storage.

import {
  HEX_BYTES,
  MAX_READS,
  changedSinceListed,
  firstStep,
  nextStep,
  type Caps,
} from "./previewPlan";
import { PreviewCancelled } from "./previewScheduler";
import type { ImageFormat } from "./previewSniff";
import { decodeText, withoutCutLine } from "./previewText";
import type { AppError, FileEntry, FileStamp, PreviewRead, TextEncoding } from "./types";

/** What the pane shows of a file once it has been read. */
export type FileView =
  | { view: "empty" }
  | { view: "image"; format: ImageFormat; bytes: Uint8Array; changed: boolean }
  | {
      view: "text";
      text: string;
      encoding: TextEncoding;
      /** Only the beginning of the file is in `text`. */
      partial: boolean;
      /** The size of the whole file. */
      size: number;
      changed: boolean;
    }
  | { view: "binary"; description: string | null; head: Uint8Array; changed: boolean }
  | { view: "tooLarge"; canLoad: boolean; size: number };

export interface LoadOptions {
  /** Reads up to `length` bytes from the start of the file. */
  read: (length: number, signal: AbortSignal) => Promise<PreviewRead>;
  caps: Caps;
  /** The user asked for a picture over the cap. */
  loadAnyway: boolean;
  /** The listing's times can be compared with the file's. */
  exactTimes: boolean;
}

/** What a view keeps in memory, for the cache's bounds. */
export function viewWeight(view: FileView): number {
  switch (view.view) {
    case "image":
      return view.bytes.length;
    case "text":
      return view.text.length * 2;
    case "binary":
      return view.head.length;
    default:
      return 1;
  }
}

export async function loadFileView(
  entry: FileEntry,
  { read, caps, loadAnyway, exactTimes }: LoadOptions,
  signal: AbortSignal,
): Promise<FileView> {
  let step = firstStep(entry, caps, loadAnyway);
  let bytes: Uint8Array = new Uint8Array(0);
  let stamp: FileStamp | null = null;
  for (let reads = 0; step.step === "read"; reads++) {
    if (reads >= MAX_READS) {
      const error: AppError = { kind: "io", message: "The file changes as it is read" };
      throw error;
    }
    const reply = await read(step.length, signal);
    if (signal.aborted) throw new PreviewCancelled();
    ({ bytes, stamp } = reply);
    step = nextStep(entry.name, bytes, stamp.size, caps, loadAnyway);
  }
  const changed = stamp !== null && changedSinceListed(entry, stamp, exactTimes);
  const size = stamp?.size ?? entry.size;
  switch (step.view) {
    case "empty":
      return { view: "empty" };
    case "tooLarge":
      return { view: "tooLarge", canLoad: step.canLoad, size };
    case "image":
      return { view: "image", format: step.format, bytes, changed };
    case "binary":
      // A copy, so the rest of what was read can be let go.
      return {
        view: "binary",
        description: step.description,
        head: bytes.slice(0, HEX_BYTES),
        changed,
      };
    case "text": {
      const { encoding, partial } = step;
      const decoded = decodeText(bytes.subarray(0, step.length), encoding, !partial);
      const text = partial ? withoutCutLine(decoded) : decoded;
      return { view: "text", text, encoding, partial, size, changed };
    }
  }
}
