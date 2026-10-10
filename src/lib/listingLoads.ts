// Orders the folder listings a pane loads. The newest load shows, except that a refresh never
// starts while the user's navigation is on its way: a refresh of the folder being left would
// otherwise cancel the navigation and keep the pane where it was.

export type ListingLoadKind = "navigation" | "refresh";

export class ListingLoads {
  private latest = 0;
  private pendingNavigation: number | null = null;

  /** Starts a load and returns its number, or null for a refresh while a navigation is on its way. */
  start(kind: ListingLoadKind): number | null {
    if (kind === "refresh" && this.pendingNavigation !== null) return null;
    this.latest += 1;
    if (kind === "navigation") this.pendingNavigation = this.latest;
    return this.latest;
  }

  /** Whether no load started after this one, so its listing is the one to show. */
  isCurrent(load: number): boolean {
    return load === this.latest;
  }

  /** Marks a load done, whether or not it succeeded. */
  finish(load: number): void {
    if (load === this.pendingNavigation) this.pendingNavigation = null;
  }
}
