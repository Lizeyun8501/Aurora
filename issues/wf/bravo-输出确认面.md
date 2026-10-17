# bravo-输出确认面 注记（2026-10-17 领单）

前置: S1 前缀闸验收通过（f7238f0）。任务: ①cmd_pull_plugin_output（core pull_output 包装）②插件详情页输出区（手动刷新+15s 轮询；不可信内容原样文本渲染 fail-closed；超长行截断+展开；空态占位）。

拍板: pull 拉取即消费透传（core 语义不动）；UserOutput=产品向内容（log=诊断流分界 15b 文档化，面板定位注释在位）；不做确认 gate/推送/导出（停手条款）。

## 拍板披露（卡面"15c 徽章页复用"偏差）

MarketplaceView 调用点硬编码 demo（pluginName="demo-plugin" granted=[]，无 invoke/真实 id）——输出区挂它=永远空面板。**实挂 PluginsView**（真实 cmd_plugin_list id 通路+invoke 在手；输出按钮→底部面板，与审计查看交互同形）——"复用已有路由"语义满足（不新建路由）。诚实注记。

## 验证矩阵

| 门 | 结果 |
|---|---|
| TS（tsc --noEmit） | 0 |
| fakepc DESK check | 0 |
| fmt 实跑+check | 0 |
| CI | 盯至全绿 |

## CI 终态（2fbe1ee）8/8 全绿 ✓ 2026-10-17
