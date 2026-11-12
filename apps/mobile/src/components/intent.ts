/** B-02 切片三: SharedIntent/ShareTarget 域随迁（纯搬移）。 */
export interface SharedIntent {
  seq: number;
  kind: 'text' | 'image' | 'action';
  content: string;
  mime: string;
  subject: string;
  at: number;
}

/** A-06: 落位目标（浮层可改——默认今日日记,工单「用户可改目标位置」）。 */
export type ShareTarget = 'daily' | 'newNote';
