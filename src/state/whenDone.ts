// What the main window does when the transfer queue finishes: notify, play a sound, and carry
// out the action picked under "When done".

import { create } from "zustand";
import { getAllWindows, getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import {
  COUNTDOWN_SECONDS,
  describeQueueFinished,
  queueEnvironment,
  type CountdownAction,
} from "../lib/automation";
import { system, toAppError } from "../lib/ipc";
import type { QueueFinished } from "../lib/types";
import { askToSave, unsavedEditorTabs } from "./editorActions";
import { useLogStore } from "./logStore";
import { saveSettingsSection, useSettingsStore } from "./settingsStore";
import { useToastStore } from "./toastStore";

interface Countdown {
  action: CountdownAction;
  /** Milliseconds since the Unix epoch. */
  deadline: number;
}

export const useCountdownStore = create<{ countdown: Countdown | null }>(() => ({
  countdown: null,
}));

function report(context: string, caught: unknown): void {
  const message = `${context}: ${toAppError(caught).message}`;
  useLogStore.getState().write("error", message);
  useToastStore.getState().show("error", message);
}

export function cancelCountdown(): void {
  useCountdownStore.setState({ countdown: null });
}

export async function performNow(action: CountdownAction): Promise<void> {
  cancelCountdown();
  try {
    if (action === "close") {
      // Files with unsaved changes are asked about first; the window closes once they are.
      const unsaved = unsavedEditorTabs();
      if (unsaved.length > 0)
        askToSave(
          unsaved.map((tab) => tab.id),
          true,
        );
      else await system.exit();
    } else {
      await system.powerAction(action);
    }
  } catch (caught) {
    report("When the queue finished", caught);
  }
}

async function anyWindowFocused(): Promise<boolean> {
  if (document.hasFocus()) return true;
  const windows = await getAllWindows().catch(() => []);
  const focused = await Promise.all(windows.map((window) => window.isFocused().catch(() => false)));
  return focused.some(Boolean);
}

async function notify(title: string, body: string, onlyInBackground: boolean): Promise<void> {
  if (onlyInBackground && (await anyWindowFocused())) return;
  let granted = await isPermissionGranted();
  if (!granted) granted = (await requestPermission()) === "granted";
  if (granted) sendNotification({ title, body });
}

/** Two short rising tones. */
function playChime(): void {
  try {
    const context = new AudioContext();
    void context.resume();
    const start = context.currentTime;
    [880, 1320].forEach((frequency, index) => {
      const oscillator = context.createOscillator();
      const gain = context.createGain();
      const at = start + index * 0.15;
      oscillator.frequency.value = frequency;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.2, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.6);
      oscillator.connect(gain).connect(context.destination);
      oscillator.start(at);
      oscillator.stop(at + 0.65);
    });
    window.setTimeout(() => void context.close(), 1500);
  } catch {
    // No audio output; the notification still says the queue finished.
  }
}

export async function handleQueueFinished(finished: QueueFinished): Promise<void> {
  // Paused jobs are still waiting, so the queue is only resting.
  if (finished.paused > 0) return;
  const { automation } = useSettingsStore.getState().settings;
  if (automation.notify) {
    const { title, body } = describeQueueFinished(finished);
    void notify(title, body, automation.notifyOnlyInBackground).catch((caught) =>
      useLogStore
        .getState()
        .write("warn", `Could not show a notification: ${toAppError(caught).message}`),
    );
  }
  if (automation.sound) playChime();

  const action = automation.whenDone;
  if (action === "nothing") return;
  if (automation.whenDoneOnce) void saveSettingsSection("automation", { whenDone: "nothing" });
  if (action === "runCommand") {
    if (!automation.whenDoneCommand.trim()) return;
    try {
      await system.runCommand(automation.whenDoneCommand, queueEnvironment(finished));
    } catch (caught) {
      report("The command after the queue finished", caught);
    }
    return;
  }
  useCountdownStore.setState({
    countdown: { action, deadline: Date.now() + COUNTDOWN_SECONDS * 1000 },
  });
  void getCurrentWindow()
    .setFocus()
    .catch(() => undefined);
}
