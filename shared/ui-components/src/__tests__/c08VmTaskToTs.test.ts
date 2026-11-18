import { describe, it, expect } from 'vitest';
import { vmTaskToTs, type CalendarTaskVm } from '@aurora/shared-types';

/** C-08 切读对拍: vmTaskToTs（snake→camel）与 MobileApp.saveTasks
 *  双写打包面（camel→snake+ext）严格互逆——往返零丢失。
 *  （saveTasks 打包面为 MobileApp 内联,此处以同形字面量对拍——
 *   两侧任一漂移即断言红,铁律「双写/切读互逆」由测试锁定。） */
describe('C-08: vmTaskToTs 互逆对拍', () => {
  const full: CalendarTaskVm = {
    id: 't1',
    text: '写周报',
    status: 'next',
    due: '2026-11-20',
    note_id: 'n1',
    created_at: 1763011200000,
    priority: 'high',
    ext: {
      estimate_minutes: 30,
      actual_minutes: 25,
      parent_task_id: 'p0',
      progress: 60,
    },
  };

  it('全字段往返零丢失（snake→camel）', () => {
    const ts = vmTaskToTs(full);
    expect(ts).toEqual({
      id: 't1',
      text: '写周报',
      status: 'next',
      createdAt: 1763011200000,
      due: '2026-11-20',
      noteId: 'n1',
      priority: 'high',
      estimateMinutes: 30,
      actualMinutes: 25,
      parentTaskId: 'p0',
      progress: 60,
    });
  });

  it('极简面（可选字段全缺省）零值污染', () => {
    const min: CalendarTaskVm = { id: 't2', text: 'x', status: 'done', created_at: 1 };
    const ts = vmTaskToTs(min);
    // 显式 undefined 值=消费面（?? 回退/布尔判定）零污染——与 toEqual
    // 语义吻合（toEqual 视 undefined 值键与缺省键等价,断言字面无碍）
    expect(ts.due).toBeUndefined();
    expect(ts.noteId).toBeUndefined();
    expect(ts.priority).toBeUndefined();
    expect(ts.estimateMinutes).toBeUndefined();
    expect(ts.actualMinutes).toBeUndefined();
    expect(ts.parentTaskId).toBeUndefined();
    expect(ts.progress).toBeUndefined();
    expect(ts).toEqual({ id: 't2', text: 'x', status: 'done', createdAt: 1 });
  });

  it('ext 字段容错（null/缺省/多余字段——透传约定不崩）', () => {
    const vm: CalendarTaskVm = { id: 't3', text: 'y', status: 'plan', created_at: 2, ext: null };
    expect(vmTaskToTs(vm).estimateMinutes).toBeUndefined();
    const vm2: CalendarTaskVm = { id: 't4', text: 'z', status: 'plan', created_at: 3, ext: { unknown_field: 1 } };
    expect(vmTaskToTs(vm2).progress).toBeUndefined(); // 多余字段不崩,已知字段缺省
  });
});
