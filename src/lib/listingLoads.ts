// Orders the folder listings a pane loads. The newest load shows, except that a refresh never
// starts while the user's navigation is on its way: a refresh of the folder being left would
// otherwise cancel the navigation and keep the pane where it was. A navigation is on its way
// only until its folder is on show, which a remembered listing makes it at once.

export type ListingLoadKind = "navigation" | "refresh";

export class ListingLoads {
  private latest = 0;
  private pendingNavigation: number | null = null;
  private readonly unfinished = new Set<number>();

  /** Starts a load and returns its number, or null for a refresh while a navigation is on its way. */
  start(kind: ListingLoadKind): number | null {
    if (kind === "refresh" && this.pendingNavigation !== null) return null;
    this.latest += 1;
    if (kind === "navigation") this.pendingNavigation = this.latest;
    this.unfinished.add(this.latest);
    return this.latest;
  }

  /** Whether no load started after this one, so its listing is the one to show. */
  isCurrent(load: number): boolean {
    return load === this.latest;
  }

  /**
   * The navigation's folder is on show already, from a remembered listing. What is still on its
   * way only refreshes that folder, so a refresh asked for meanwhile starts, and replaces it.
   */
  landed(load: number): void {
    if (load === this.pendingNavigation) this.pendingNavigation = null;
  }

  /** Marks a load done, whether or not it succeeded. */
  finish(load: number): void {
    if (load === this.pendingNavigation) this.pendingNavigation = null;
    this.unfinished.delete(load);
  }

  /** Whether a load is still on its way. */
  get busy(): boolean {
    return this.unfinished.size > 0;
  }
}
