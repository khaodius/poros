// How much of a window's workspace the transfer queue and log take below the panes. Every window
// opens with the share chosen last.

const STORAGE_KEY = "poros.bottomPanelShare";
export const DEFAULT_PANEL_SHARE = 0.3;
const SHARE_LIMITS = { min: 0.12, max: 0.75 };

export function clampPanelShare(share: number): number {
  return Math.min(SHARE_LIMITS.max, Math.max(SHARE_LIMITS.min, share));
}

export function storedPanelShare(): number {
  try {
    const stored = Number(localStorage.getItem(STORAGE_KEY));
    return stored > 0 ? clampPanelShare(stored) : DEFAULT_PANEL_SHARE;
  } catch {
    return DEFAULT_PANEL_SHARE;
  }
}

export function storePanelShare(share: number): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(share));
  } catch {
    // The panel then opens at its default height next time.
  }
}
