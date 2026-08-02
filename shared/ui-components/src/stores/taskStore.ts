import { create } from 'zustand';
import type { Project, Task, TaskStatus } from '@aurora/shared-types';

export interface TaskState {
  tasks: Task[];
  projects: Project[];
  inboxCount: number;
  addTask: (task: Task) => void;
  updateTaskStatus: (taskId: string, status: TaskStatus) => void;
  clarifyInbox: (taskId: string, status: TaskStatus) => void;
}

function countInbox(tasks: Task[]): number {
  return tasks.filter((t) => t.status === 'inbox').length;
}

const now = (): string => new Date().toISOString();

export const useTaskStore = create<TaskState>()((set) => ({
  tasks: [],
  projects: [],
  inboxCount: 0,
  addTask: (task) =>
    set((state) => {
      const tasks = [...state.tasks, task];
      return { tasks, inboxCount: countInbox(tasks) };
    }),
  updateTaskStatus: (taskId, status) =>
    set((state) => {
      const tasks = state.tasks.map((t) =>
        t.id === taskId ? { ...t, status, updated_at: now() } : t,
      );
      return { tasks, inboxCount: countInbox(tasks) };
    }),
  clarifyInbox: (taskId, status) =>
    set((state) => {
      const tasks = state.tasks.map((t) =>
        t.id === taskId && t.status === 'inbox'
          ? { ...t, status, updated_at: now() }
          : t,
      );
      return { tasks, inboxCount: countInbox(tasks) };
    }),
}));
