import { beforeEach, describe, expect, it } from "vitest";
import { EMPTY_DRAFT } from "../lib/connectDraft";
import { useUiStore, type AppDialog } from "./uiStore";

const shown = () => useUiStore.getState().dialog;

describe("closeIfShown", () => {
  const connect: AppDialog = { kind: "connect", draft: EMPTY_DRAFT };
  const settings: AppDialog = { kind: "settings", section: "transfers" };

  beforeEach(() => useUiStore.setState({ dialog: null }));

  it("closes the dialog on show", () => {
    useUiStore.getState().open(connect);
    useUiStore.getState().closeIfShown(connect);
    expect(shown()).toBeNull();
  });

  it("leaves alone a dialog opened after the one asking was dismissed", () => {
    const { open, close, closeIfShown } = useUiStore.getState();
    open(connect);
    close();
    open(settings);
    closeIfShown(connect);
    expect(shown()).toBe(settings);
  });

  it("leaves alone a dialog that took the place of the one asking", () => {
    const { open, closeIfShown } = useUiStore.getState();
    open(connect);
    open(settings);
    closeIfShown(connect);
    expect(shown()).toBe(settings);
  });

  it("leaves alone a dialog of the same kind opened since", () => {
    const { open, close, closeIfShown } = useUiStore.getState();
    const another: AppDialog = { kind: "connect", draft: EMPTY_DRAFT };
    open(connect);
    close();
    open(another);
    closeIfShown(connect);
    expect(shown()).toBe(another);
  });

  it("goes back to the dialog it was opened from, only while it is on show", () => {
    const { open, close, closeIfShown } = useUiStore.getState();
    const importing: AppDialog = { kind: "importConnections", returnTo: connect };
    open(importing);
    closeIfShown(importing, connect);
    expect(shown()).toBe(connect);

    open(importing);
    close();
    open(settings);
    closeIfShown(importing, connect);
    expect(shown()).toBe(settings);
  });

  it("does nothing once every dialog is closed", () => {
    const { open, close, closeIfShown } = useUiStore.getState();
    const importing: AppDialog = { kind: "importConnections", returnTo: connect };
    open(importing);
    close();
    closeIfShown(importing, connect);
    expect(shown()).toBeNull();
  });
});
