import { createContext, useContext, useEffect, type KeyboardEvent } from "react";
import { useLatest } from "./useLatest";

/** Answers a key pressed while the preview has the keyboard; true when it took the key. */
export type PreviewKeyHandler = (event: KeyboardEvent<HTMLElement>) => boolean;

/**
 * The preview itself holds the keyboard, so its parts never see a key go by. Those with keys of
 * their own, as a picture that zooms, are asked here first.
 */
export const PreviewKeys = createContext<Set<PreviewKeyHandler> | null>(null);

export function usePreviewKeys(handler: PreviewKeyHandler): void {
  const handlers = useContext(PreviewKeys);
  const latest = useLatest(handler);
  useEffect(() => {
    if (!handlers) return;
    const answer: PreviewKeyHandler = (event) => latest.current(event);
    handlers.add(answer);
    return () => {
      handlers.delete(answer);
    };
  }, [handlers, latest]);
}
