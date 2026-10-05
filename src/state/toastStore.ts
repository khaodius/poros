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
}

const VISIBLE_MILLIS = 6000;
let nextId = 1;

export const useToastStore = create<ToastState>((set, get) => ({
  toasts: [],
  show: (tone, message) => {
    const id = nextId++;
    set((state) => ({ toasts: [...state.toasts.slice(-3), { id, tone, message }] }));
    window.setTimeout(() => get().dismiss(id), VISIBLE_MILLIS);
  },
  dismiss: (id) => set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),
}));
