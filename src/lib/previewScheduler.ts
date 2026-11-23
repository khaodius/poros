// When the preview pane asks for a file: only once the cursor has rested on it, a few requests
// at a time, and never for something the user has already moved away from.

/** How a request that was cancelled ends, told apart from one that failed. */
export class PreviewCancelled extends Error {
  constructor() {
    super("The preview was cancelled");
    this.name = "PreviewCancelled";
  }
}

export function isCancelled(error: unknown): boolean {
  return error instanceof PreviewCancelled;
}

/**
 * Starts a load once nothing else has been asked for during a delay, as when the cursor stops
 * on a file. Whatever was waiting or under way before is cancelled by the next call.
 */
export class RestingLoad {
  private timer: ReturnType<typeof setTimeout> | null = null;
  private current: AbortController | null = null;

  constructor(private readonly delayMillis: number) {}

  /** Calls `load` after the delay, or at once with `immediately`, with a signal that tells it
   * when to stop. */
  schedule(load: (signal: AbortSignal) => void, immediately = false): void {
    this.cancel();
    const controller = new AbortController();
    this.current = controller;
    if (immediately) {
      load(controller.signal);
      return;
    }
    this.timer = setTimeout(() => {
      this.timer = null;
      load(controller.signal);
    }, this.delayMillis);
  }

  /** Drops a load that is waiting and stops one under way. */
  cancel(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.current?.abort();
    this.current = null;
  }
}

interface Waiting<Result> {
  run: (signal: AbortSignal) => Promise<Result>;
  controller: AbortController;
  promise: Promise<Result>;
  resolve: (result: Result) => void;
  reject: (error: unknown) => void;
  started: boolean;
}

/**
 * Runs requests a few at a time, in the order asked for. Each has a key: asking again for one
 * that waits or runs joins it, and cancelling it by its key drops it from the line or tells it
 * to stop.
 */
export class RequestQueue {
  private readonly requests = new Map<string, Waiting<unknown>>();
  private active = 0;

  constructor(private readonly limit: number) {}

  get running(): number {
    return this.active;
  }

  get waiting(): number {
    let waiting = 0;
    for (const request of this.requests.values()) {
      if (!request.started) waiting++;
    }
    return waiting;
  }

  request<Result>(key: string, run: (signal: AbortSignal) => Promise<Result>): Promise<Result> {
    const existing = this.requests.get(key);
    // One told to stop is on its way out: asking again starts afresh.
    if (existing && !existing.controller.signal.aborted) {
      return existing.promise as Promise<Result>;
    }
    let resolve!: (result: Result) => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<Result>((resolvePromise, rejectPromise) => {
      resolve = resolvePromise;
      reject = rejectPromise;
    });
    const waiting: Waiting<Result> = {
      run,
      controller: new AbortController(),
      promise,
      resolve,
      reject,
      started: false,
    };
    this.requests.set(key, waiting as Waiting<unknown>);
    this.startNext();
    return promise;
  }

  /** Drops the request from the line, or tells it to stop if it runs. */
  cancel(key: string): void {
    const request = this.requests.get(key);
    if (!request) return;
    request.controller.abort();
    if (request.started) return;
    this.requests.delete(key);
    request.reject(new PreviewCancelled());
  }

  /** Cancels every request but those named. */
  keepOnly(keys: ReadonlySet<string>): void {
    for (const key of [...this.requests.keys()]) {
      if (!keys.has(key)) this.cancel(key);
    }
  }

  cancelAll(): void {
    this.keepOnly(new Set());
  }

  private startNext(): void {
    for (const [key, request] of this.requests) {
      if (this.active >= this.limit) return;
      if (request.started) continue;
      request.started = true;
      this.active++;
      void this.finish(key, request);
    }
  }

  private async finish(key: string, request: Waiting<unknown>): Promise<void> {
    try {
      const result = await request.run(request.controller.signal);
      if (request.controller.signal.aborted) request.reject(new PreviewCancelled());
      else request.resolve(result);
    } catch (error) {
      request.reject(request.controller.signal.aborted ? new PreviewCancelled() : error);
    } finally {
      this.active--;
      if (this.requests.get(key) === request) this.requests.delete(key);
      this.startNext();
    }
  }
}
