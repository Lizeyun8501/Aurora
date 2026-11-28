/**
 * navigation.ts — U-05: 导航注册表（单源事实,IA 图同源派生）。
 *
 * 背景: V26 前导航知识散在 MobileApp 五处（ViewId 域/VIEW_TITLES/TAB_OF_VIEW/
 * TabBar items/Drawer2 分组 item()）——加视图需五处同步,漏改即 IA 漂移。
 * U-05 收敛: 视图元数据唯一注册于此,TabBar/Drawer2/标题栏从注册表渲染。
 *
 * ADR-007 语义保持:
 * - 主 Tab 五项: 今日/笔记/收集箱/任务/日历（搜索=AppBar 全局动作,无 Tab 语境）
 * - 次级视图挂 Tab 高亮语境（favorites/recent/diary/trash→notes 等）
 * - Agent=抽屉「更多」;设置=笔记页齿轮二级
 */

/** 视图 id 域（全应用可导航终点——注册表即域,新增视图在此登记）。 */
export type ViewId =
  | 'today' | 'notes' | 'search' | 'agent' | 'settings' | 'calendar'
  | 'favorites' | 'recent' | 'tasks' | 'diary' | 'trash' | 'inbox'
  | 'marketplace';

/** 单视图注册项。 */
export interface ViewEntry {
  id: ViewId;
  /** AppBar 标题（VIEW_TITLES 语义——长标题）。 */
  title: string;
  /** TabBar/抽屉短标签（缺省=title——TabBar「笔记」vs AppBar「知识库」两语义显式分流）。 */
  short?: string;
  /** 所属主 Tab id（''=无 Tab 语境,如 search;ADR-007）。 */
  tab: string;
  /** 入口层级——IA 图层级: tab=底部主 Tab / drawer=抽屉次级 / nested=页内二级。 */
  level: 'tab' | 'drawer' | 'nested';
  /** TabBar/抽屉渲染图标键（MobileApp I 集注入——注册表不持 ReactNode,壳层装配）。 */
  iconKey: string;
}

/** 导航注册表（唯一事实源——TabBar/Drawer2/标题全由此派生）。 */
export const NAV_REGISTRY: ViewEntry[] = [
  // 主 Tab 层（IA L0）
  { id: 'today',    title: '今日',     tab: 'today',    level: 'tab',    iconKey: 'today' },
  { id: 'notes',    title: '知识库',   tab: 'notes',    level: 'tab',    iconKey: 'notes', short: '笔记' },
  { id: 'inbox',    title: '收集箱',   tab: 'inbox',    level: 'tab',    iconKey: 'inbox' },
  { id: 'tasks',    title: '任务中心', tab: 'tasks',    level: 'tab',    iconKey: 'taskSquare', short: '任务' },
  { id: 'calendar', title: '日历',     tab: 'calendar', level: 'tab',    iconKey: 'calendar' },
  // 抽屉次级层（IA L1——Tab 语境保留）
  { id: 'favorites', title: '我的收藏', tab: 'notes',   level: 'drawer', iconKey: 'star' },
  { id: 'recent',    title: '最近浏览', tab: 'notes',   level: 'drawer', iconKey: 'clock' },
  { id: 'diary',     title: '日记本',   tab: 'notes',   level: 'drawer', iconKey: 'book' },
  { id: 'trash',     title: '回收站',   tab: 'notes',   level: 'drawer', iconKey: 'trash' },
  { id: 'agent',     title: 'Agent',    tab: 'agent',   level: 'drawer', iconKey: 'sparkles' },
  { id: 'marketplace', title: '插件市场', tab: 'notes',   level: 'drawer', iconKey: 'agent' },
  // 页内二级（IA L2——无直入口,语境挂主 Tab）
  { id: 'search',   title: '搜索',   tab: '',          level: 'nested', iconKey: 'search' },
  { id: 'settings', title: '设置',   tab: 'settings',  level: 'nested', iconKey: 'gear' },
];

/** 视图→注册项索引（O(1) 查询——TabBar 高亮/标题渲染热路径）。 */
export const VIEW_INDEX: Record<ViewId, ViewEntry> = Object.fromEntries(
  NAV_REGISTRY.map((e) => [e.id, e]),
) as Record<ViewId, ViewEntry>;

/** 视图标题（VIEW_TITLES 单源替代）。 */
export function viewTitle(v: ViewId): string {
  return VIEW_INDEX[v]?.title ?? v;
}

/** TabBar/抽屉短标签（short 优先,缺省 title——「笔记/知识库」双语义分流点）。 */
export function viewShortLabel(v: ViewId): string {
  return VIEW_INDEX[v]?.short ?? VIEW_INDEX[v]?.title ?? v;
}

/** 视图所属主 Tab（TAB_OF_VIEW 单源替代;无语境=''——TabBar 语义:无高亮）。 */
export function tabOfView(v: ViewId): string {
  return VIEW_INDEX[v]?.tab ?? '';
}

/** 主 Tab 列表（TabBar 渲染源——IA L0 顺序即注册表序）。 */
export const TABS: ViewEntry[] = NAV_REGISTRY.filter((e) => e.level === 'tab');

/** 抽屉次级列表（Drawer2 渲染源——分组逻辑在壳层,项源自此）。 */
export const DRAWER_VIEWS: ViewEntry[] = NAV_REGISTRY.filter((e) => e.level === 'drawer');

/** IA 图（Markdown 形——文档/交付物由单源派生,不手写漂移）。 */
export function navigationIaMarkdown(): string {
  const lines = ['# Aurora Note 移动端 IA 图（单源派生——navigation.ts）', ''];
  for (const lvl of ['tab', 'drawer', 'nested'] as const) {
    const label = lvl === 'tab' ? 'L0 底部主 Tab' : lvl === 'drawer' ? 'L1 抽屉次级' : 'L2 页内二级';
    lines.push(`## ${label}`);
    for (const e of NAV_REGISTRY.filter((x) => x.level === lvl)) {
      const ctx = e.tab ? `（Tab 语境: ${e.tab}）` : '（无 Tab 语境）';
      lines.push(`- **${e.title}** \`${e.id}\` ${ctx}`);
    }
    lines.push('');
  }
  return lines.join('\n');
}
