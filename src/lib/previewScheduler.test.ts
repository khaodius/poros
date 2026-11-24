import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  PreviewCancelled,
  RequestQueue,
  RestingLoad,
  isCancelled,
  loadRetrying,
} from "./previewScheduler";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("RestingLoad", () => {
  it("loads only once the cursor has rested for the delay", () => {
    const resting = new RestingLoad(250);
    const load = vi.fn();
    resting.schedule(load);
    vi.advanceTimersByTime(249);
    expect(load).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(load).toHaveBeenCalledTimes(1);
    expect((load.mock.calls[0][0] as AbortSignal).aborted).toBe(false);
  });

  it("never loads what the cursor only passed over", () => {
    const resting = new RestingLoad(250);
    const loads = [vi.fn(), vi.fn(), vi.fn()];
    for (const load of loads) {
      resting.schedule(load);
      vi.advanceTimersByTime(100);
    }
    vi.advanceTimersByTime(250);
    expect(loads[0]).not.toHaveBeenCalled();
    expect(loads[1]).not.toHaveBeenCalled();
    expect(loads[2]).toHaveBeenCalledTimes(1);
  });

  it("tells a load under way to stop when the cursor moves on", () => {
    const resting = new RestingLoad(250);
    let first: AbortSignal | undefined;
    resting.schedule((signal) => {
      first = signal;
    });
    vi.advanceTimersByTime(250);
    expect(first?.aborted).toBe(false);
    const second = vi.fn();
    resting.schedule(second);
    expect(first?.aborted).toBe(true);
    expect(second).not.toHaveBeenCalled();
    vi.advanceTimersByTime(250);
    expect((second.mock.calls[0][0] as AbortSignal).aborted).toBe(false);
  });

  it("loads at once when asked to, and nothing after it is cancelled", () => {
    const resting = new RestingLoad(250);
    let signal: AbortSignal | undefined;
    resting.schedule((given) => {
      signal = given;
    }, true);
    expect(signal?.aborted).toBe(false);
    resting.cancel();
    expect(signal?.aborted).toBe(true);

    const never = vi.fn();
    resting.schedule(never);
    resting.cancel();
    vi.advanceTimersByTime(1000);
    expect(never).not.toHaveBeenCalled();
  });
});

/** A request that finishes when the test says so. */
function controlled() {
  let finish!: (value: string) => void;
  let fail!: (error: unknown) => void;
  let signal: AbortSignal | undefined;
  const started = vi.fn();
  const run = (given: AbortSignal) => {
    signal = given;
    started();
    return new Promise<string>((resolve, reject) => {
      finish = resolve;
      fail = reject;
    });
  };
  return {
    run,
    started,
    finish: (value: string) => finish(value),
    fail: (error: unknown) => fail(error),
    signal: () => signal,
  };
}

async function settle() {
  await vi.advanceTimersByTimeAsync(0);
}

describe("RequestQueue", () => {
  it("runs a few requests at a time, in the order asked for", async () => {
    const queue = new RequestQueue(2);
    const requests = [controlled(), controlled(), controlled(), controlled()];
    const results = requests.map((request, index) => queue.request(`key-${index}`, request.run));
    expect(requests.map((request) => request.started.mock.calls.length)).toEqual([1, 1, 0, 0]);
    expect(queue.running).toBe(2);
    expect(queue.waiting).toBe(2);

    requests[1].finish("second");
    await expect(results[1]).resolves.toBe("second");
    expect(requests[2].started).toHaveBeenCalledTimes(1);
    expect(requests[3].started).not.toHaveBeenCalled();

    requests[0].finish("first");
    requests[2].finish("third");
    await settle();
    expect(requests[3].started).toHaveBeenCalledTimes(1);
    requests[3].finish("fourth");
    await expect(Promise.all(results)).resolves.toEqual(["first", "second", "third", "fourth"]);
    expect(queue.running).toBe(0);
    expect(queue.waiting).toBe(0);
  });

  it("drops a cancelled request from the line without ever starting it", async () => {
    const queue = new RequestQueue(1);
    const first = controlled();
    const second = controlled();
    const third = controlled();
    const running = queue.request("first", first.run);
    const dropped = queue.request("second", second.run);
    const kept = queue.request("third", third.run);
    queue.cancel("second");
    await expect(dropped).rejects.toBeInstanceOf(PreviewCancelled);
    first.finish("done");
    await expect(running).resolves.toBe("done");
    expect(second.started).not.toHaveBeenCalled();
    expect(third.started).toHaveBeenCalledTimes(1);
    third.finish("also done");
    await expect(kept).resolves.toBe("also done");
  });

  it("tells a running request to stop, and frees its place when it has", async () => {
    const queue = new RequestQueue(1);
    const first = controlled();
    const second = controlled();
    const stopped = queue.request("first", first.run);
    const next = queue.request("second", second.run);
    queue.cancel("first");
    expect(first.signal()?.aborted).toBe(true);
    // Its place is taken until the read has actually ended.
    expect(second.started).not.toHaveBeenCalled();
    first.fail(new Error("stopped by the server"));
    await expect(stopped).rejects.toSatisfy(isCancelled);
    expect(second.started).toHaveBeenCalledTimes(1);
    second.finish("done");
    await expect(next).resolves.toBe("done");
  });

  it("counts a result that arrives after the cancellation as cancelled", async () => {
    const queue = new RequestQueue(1);
    const request = controlled();
    const result = queue.request("key", request.run);
    queue.cancel("key");
    request.finish("too late");
    await expect(result).rejects.toBeInstanceOf(PreviewCancelled);
  });

  it("passes a failure on as it is", async () => {
    const queue = new RequestQueue(1);
    const request = controlled();
    const result = queue.request("key", request.run);
    const failure = { kind: "permissionDenied", message: "Permission denied" };
    request.fail(failure);
    await expect(result).rejects.toBe(failure);
    expect(isCancelled(failure)).toBe(false);
    expect(queue.running).toBe(0);
  });

  it("joins a request already waiting or running under the same key", async () => {
    const queue = new RequestQueue(1);
    const first = controlled();
    const again = controlled();
    const one = queue.request("same", first.run);
    const two = queue.request("same", again.run);
    expect(two).toBe(one);
    expect(again.started).not.toHaveBeenCalled();
    first.finish("shared");
    await expect(two).resolves.toBe("shared");
    // Once it has finished, asking again runs again.
    const fresh = queue.request("same", again.run);
    expect(again.started).toHaveBeenCalledTimes(1);
    again.finish("new");
    await expect(fresh).resolves.toBe("new");
  });

  it("starts afresh when asked again for a request that was told to stop", async () => {
    const queue = new RequestQueue(2);
    const first = controlled();
    const again = controlled();
    const stopped = queue.request("same", first.run);
    queue.cancel("same");
    const fresh = queue.request("same", again.run);
    expect(fresh).not.toBe(stopped);
    expect(again.started).toHaveBeenCalledTimes(1);
    first.finish("late");
    await expect(stopped).rejects.toBeInstanceOf(PreviewCancelled);
    again.finish("fresh");
    await expect(fresh).resolves.toBe("fresh");
    expect(queue.running).toBe(0);
  });

  it("keeps only the requests still wanted, as when a grid scrolls", async () => {
    const queue = new RequestQueue(1);
    const requests = [controlled(), controlled(), controlled()];
    const results = requests.map((request, index) => queue.request(`tile-${index}`, request.run));
    const outcomes = results.map((result) => result.catch((error: unknown) => error));
    queue.keepOnly(new Set(["tile-2"]));
    expect(requests[0].signal()?.aborted).toBe(true);
    requests[0].finish("gone");
    await settle();
    expect(requests[1].started).not.toHaveBeenCalled();
    expect(requests[2].started).toHaveBeenCalledTimes(1);
    requests[2].finish("kept");
    const [first, second, third] = await Promise.all(outcomes);
    expect(isCancelled(first)).toBe(true);
    expect(isCancelled(second)).toBe(true);
    expect(third).toBe("kept");

    queue.request("later", controlled().run).catch(() => undefined);
    queue.cancelAll();
    expect(queue.waiting).toBe(0);
  });
});

describe("loadRetrying", () => {
  const busy = { kind: "busy", message: "Another request is using this connection" };
  const passes = (caught: unknown) => caught === busy;

  it("loads again a while after a failure that passes, telling of it meanwhile", async () => {
    const load = vi.fn().mockRejectedValueOnce(busy).mockResolvedValueOnce("read");
    const meanwhile = vi.fn();
    const signal = new AbortController().signal;
    const loaded = loadRetrying(load, signal, { retries: 3, delayMillis: 1000, passes, meanwhile });
    await settle();
    expect(load).toHaveBeenCalledTimes(1);
    expect(meanwhile).toHaveBeenCalledWith(busy);
    await vi.advanceTimersByTimeAsync(999);
    expect(load).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(await loaded).toBe("read");
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("gives up after its retries, and at once on any other failure", async () => {
    const always = vi.fn().mockRejectedValue(busy);
    const signal = new AbortController().signal;
    const meanwhile = vi.fn();
    const outcome = loadRetrying(always, signal, {
      retries: 2,
      delayMillis: 10,
      passes,
      meanwhile,
    }).catch((caught: unknown) => caught);
    await vi.advanceTimersByTimeAsync(100);
    expect(await outcome).toBe(busy);
    expect(always).toHaveBeenCalledTimes(3);
    expect(meanwhile).toHaveBeenCalledTimes(2);

    const denied = { kind: "permissionDenied", message: "No" };
    const once = vi.fn().mockRejectedValue(denied);
    const refused = loadRetrying(once, signal, {
      retries: 2,
      delayMillis: 10,
      passes,
      meanwhile,
    }).catch((caught: unknown) => caught);
    expect(await refused).toBe(denied);
    expect(once).toHaveBeenCalledTimes(1);
  });

  it("stops waiting the moment it is cancelled", async () => {
    const load = vi.fn().mockRejectedValue(busy);
    const controller = new AbortController();
    const outcome = loadRetrying(load, controller.signal, {
      retries: 5,
      delayMillis: 1000,
      passes,
      meanwhile: () => undefined,
    }).catch((caught: unknown) => caught);
    await settle();
    controller.abort();
    expect(isCancelled(await outcome)).toBe(true);
    await vi.advanceTimersByTimeAsync(5000);
    expect(load).toHaveBeenCalledTimes(1);
  });
});
