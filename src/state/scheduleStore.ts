import { create } from "zustand";
import { schedules } from "../lib/ipc";
import type { ScheduledTask, TaskView } from "../lib/types";
import { forgetWhatRunsChanged } from "./scheduleActions";

interface ScheduleState {
  tasks: TaskView[];
  /** Read at least once, so changes from the backend are worth reloading. */
  loaded: boolean;
  load: () => Promise<void>;
  save: (task: ScheduledTask) => Promise<TaskView>;
  remove: (id: string) => Promise<void>;
  runNow: (id: string) => Promise<void>;
}

export const useScheduleStore = create<ScheduleState>((set, get) => ({
  tasks: [],
  loaded: false,
  load: async () => {
    const tasks = await schedules.list();
    const { loaded, tasks: before } = get();
    set({ tasks, loaded: true });
    // Runs that finished since the last look, here or in poros-cli, changed folders on servers.
    if (loaded) forgetWhatRunsChanged(before, tasks);
  },
  save: async (task) => {
    const saved = await schedules.save(task);
    await get().load();
    return saved;
  },
  remove: async (id) => {
    await schedules.remove(id);
    await get().load();
  },
  runNow: async (id) => {
    await schedules.runNow(id);
    await get().load();
  },
}));

/** Keeps an open list current when tasks start, finish or change in another window. */
export function reloadSchedules(): Promise<void> {
  const { loaded, load } = useScheduleStore.getState();
  return loaded ? load() : Promise.resolve();
}
