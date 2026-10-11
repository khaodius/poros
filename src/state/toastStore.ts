import { create } from "zustand";

export type ToastTone = "info" | "error";

export interface Toast {
  id: number;
  tone: ToastTone;
  message: string;
}

interface ToastState {
  toasts: Toast[];
  show: (tone: ToastTone, message: string) => void;
  dismiss: (id: number) => void;
  /** Keeps a toast up while the pointer is on it, to be read or copied. */
  hold: (id: number) => void;
  release: (id: number) => void;
}

const VISIBLE_MILLIS = 6000;
let nextId = 1;
const timers = new Map<number, number>();
const held = new Set<number>();

function forget(id: number): void {
  window.clearTimeout(timers.get(id));
  timers.delete(id);
  held.delete(id);
}

export const useToastStore = create<ToastState>((set, get) => {
  const startTimer = (id: number) => {
    window.clearTimeout(timers.get(id));
    if (held.has(id)) return;
    const timer = window.setTimeout(() => get().dismiss(id), VISIBLE_MILLIS);
    timers.set(id, timer);
  };
  return {
    toasts: [],
    show: (tone, message) => {
      // The same notice again keeps the one on screen up longer rather than stacking a copy.
      const shown = get().toasts.find((toast) => toast.tone === tone && toast.message === message);
      if (shown) {
        startTimer(shown.id);
        return;
      }
      const id = nextId++;
      const toasts = get().toasts;
      for (const pushedOut of toasts.slice(0, -3)) forget(pushedOut.id);
      set({ toasts: [...toasts.slice(-3), { id, tone, message }] });
      startTimer(id);
    },
    dismiss: (id) => {
      forget(id);
      set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) }));
    },
    hold: (id) => {
      held.add(id);
      window.clearTimeout(timers.get(id));
    },
    release: (id) => {
      held.delete(id);
      if (get().toasts.some((toast) => toast.id === id)) startTimer(id);
    },
  };
});
