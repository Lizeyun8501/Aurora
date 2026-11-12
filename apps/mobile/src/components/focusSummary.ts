/** B-02 切片三: focusSummary 随迁（纯搬移零逻辑改动）。 */
import { platform } from '../adapters/androidPlatform';
import type { Task } from './primitives';

export function focusSummary(tasks: Task[]): { estimate: number; actual: number; deviation: number | null } {
  let estimate = 0, actual = 0;
  for (const t of tasks) {
    actual += t.actualMinutes ?? 0;
    estimate += t.estimateMinutes ?? 0;
  }
  const remote = (platform as { todayFocusSummary?: () => { estimate: number; actual: number; deviation: number | null } | null }).todayFocusSummary?.();
  if (remote) return remote;
  const deviation = estimate > 0 ? (actual - estimate) / estimate : null;
  return { estimate, actual, deviation };
}
