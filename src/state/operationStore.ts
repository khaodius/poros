// Moves, copies and other work on files where they are, while it runs: the tray shows each
// one with its progress, and asks what to do when names are already taken.

import { create } from "zustand";
import type { NameClash, OperationProgress, PlaceMode } from "../lib/types";

/** Quick operations finish before the tray would show them, so it waits this long. */
const SHOW_AFTER_MILLIS = 400;

export interface RunningOperation {
  id: string;
  title: string;
  shown: boolean;
  files: number;
  bytes: number;
  current?: string;
}

export interface NameClashQuestion {
  mode: PlaceMode;
  /** The names already taken in the folder. */
  names: string[];
  total: number;
  folder: string;
  answer: (choice: NameClash | null) => void;
}

interface OperationState {
  running: RunningOperation[];
  question: NameClashQuestion | null;
  update: (progress: OperationProgress) => void;
}

export const useOperationStore = create<OperationState>((set) => ({
  running: [],
  question: null,
  update: ({ operationId, files, bytes, current }) =>
    set((state) => ({
      running: state.running.map((operation) =>
        operation.id === operationId ? { ...operation, files, bytes, current } : operation,
      ),
    })),
}));

function changeOperation(id: string, change: Partial<RunningOperation>): void {
  useOperationStore.setState((state) => ({
    running: state.running.map((operation) =>
      operation.id === id ? { ...operation, ...change } : operation,
    ),
  }));
}

/** Runs `work` under a fresh operation id, showing it in the tray while it lasts. */
export async function track<T>(title: string, work: (operationId: string) => Promise<T>) {
  const id = crypto.randomUUID();
  useOperationStore.setState((state) => ({
    running: [...state.running, { id, title, shown: false, files: 0, bytes: 0 }],
  }));
  const reveal = window.setTimeout(() => changeOperation(id, { shown: true }), SHOW_AFTER_MILLIS);
  try {
    return await work(id);
  } finally {
    window.clearTimeout(reveal);
    useOperationStore.setState((state) => ({
      running: state.running.filter((operation) => operation.id !== id),
    }));
  }
}

/** Asks what to do about names already taken; null when the user cancels. */
export function askNameClash(
  question: Omit<NameClashQuestion, "answer">,
): Promise<NameClash | null> {
  useOperationStore.getState().question?.answer(null);
  return new Promise((resolve) => {
    useOperationStore.setState({
      question: {
        ...question,
        answer: (choice) => {
          useOperationStore.setState({ question: null });
          resolve(choice);
        },
      },
    });
  });
}
