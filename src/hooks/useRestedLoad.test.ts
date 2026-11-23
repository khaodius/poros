// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppError } from "../lib/types";
import {
  BUSY_RETRIES,
  BUSY_RETRY_MILLIS,
  REST_MILLIS,
  useRestedLoad,
  type Rested,
} from "./useRestedLoad";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const busy: AppError = { kind: "busy", message: "Another request is using this connection" };
const lent: AppError = { kind: "lent", message: "A transfer is using this connection" };

let root: Root;
let shown: Rested<string>[] = [];

function Probe({ load }: { load: (signal: AbortSignal) => Promise<string> }) {
  const { state } = useRestedLoad({
    key: "/srv/logo.png",
    cached: undefined,
    mode: "auto",
    storage: "server",
    inVault: false,
    load,
  });
  shown.push(state);
  return null;
}

async function render(load: (signal: AbortSignal) => Promise<string>) {
  await act(async () => {
    root.render(createElement(Probe, { load }));
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(REST_MILLIS.server);
  });
}

function last(): Rested<string> {
  return shown[shown.length - 1];
}

beforeEach(() => {
  vi.useFakeTimers();
  shown = [];
  root = createRoot(document.createElement("div"));
});

afterEach(() => {
  act(() => root.unmount());
  vi.useRealTimers();
});

describe("useRestedLoad", () => {
  it("keeps a busy connection's notice on show and loads again by itself", async () => {
    const load = vi.fn<(signal: AbortSignal) => Promise<string>>();
    load.mockRejectedValueOnce(busy).mockResolvedValueOnce("picture");
    await render(load);
    expect(last()).toEqual({
      status: "failed",
      problem: "busy",
      message: busy.message,
      retrying: true,
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(BUSY_RETRY_MILLIS);
    });
    expect(load).toHaveBeenCalledTimes(2);
    expect(last()).toEqual({ status: "ready", result: "picture" });
    // Nothing in between went back to loading.
    const afterFailure = shown.slice(shown.findIndex((state) => state.status === "failed"));
    expect(afterFailure.some((state) => state.status === "loading")).toBe(false);
  });

  it("stops trying by itself after a while", async () => {
    const load = vi.fn<(signal: AbortSignal) => Promise<string>>().mockRejectedValue(busy);
    await render(load);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(BUSY_RETRY_MILLIS * (BUSY_RETRIES + 2));
    });
    expect(load).toHaveBeenCalledTimes(BUSY_RETRIES + 1);
    expect(last()).toMatchObject({ status: "failed", problem: "busy", retrying: false });
  });

  it("waits for the user while transfers have the connection", async () => {
    const load = vi.fn<(signal: AbortSignal) => Promise<string>>().mockRejectedValue(lent);
    await render(load);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(BUSY_RETRY_MILLIS * 3);
    });
    expect(load).toHaveBeenCalledTimes(1);
    expect(last()).toEqual({
      status: "failed",
      problem: "lent",
      message: lent.message,
      retrying: false,
    });
  });
});
