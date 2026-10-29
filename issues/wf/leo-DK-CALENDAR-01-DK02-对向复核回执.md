# Leo 主会话 · DK-CALENDAR-01 对向复核+断链补全回执

**角色**: Leo（主会话/复核方） ｜ **对象**: Alpha 3e361f4 + 7d29abf/f9c078b（Bravo WF 交付） ｜ **时间**: 2026-10-29 23:0x

## 复核结论

### ① CALENDAR-01（日历升级 P0）——功能验收通过+两处真 bug 修复（8f82772）

- 7d29abf 月⇄日双视图+时间轴聚合：功能面 6/6 + 我的 UTC 桩复测全绿
- **真 bug 1（环境探针挖出）**: `dayData` 用 `updatedAt.startsWith(key)` 匹配公历——
  updatedAt 为 RFC3339 UTC，**本地 00:00-07:59 笔记（UTC=前一日）错落昨日格子**。
  修: `Date` 本地日界比对（NaN 兜底回退）。
- **真 bug 2**: 兜底区多小时各自渲染一行「更早」——合并单「更早」/单「更晚」。
- 验证: 本地 01:30/06:00（UTC 前日）→「更早」唯一行点亮今日格;23:40→「更晚」;
  15 主时段+2 兜底;零 pageerror。
- **我的 373bcba 版被弃**: 同病 startsWith+`slice(11,13)` 直取 UTC 时（真机偏移 8h
  落错格）——Alpha 的 `new Date(iso).getHours()` 本地解析是对的。**双向复核价值实证**。

### ② DK-02（3e361f4 回收站 S2）——半接入实锤，补链（本回执 commit）

- Alpha 自白「cargo check 三跑被 504 闪断（uniffi_bindgen 编译窗>5min 超回收阈值）」
- 实锤: MainActivity.java 调 `core.listTrashed()/restoreNote/purgeNote/purgeExpired`
  但 **UniffiAppCore.java 无四方法/类**（javac cannot find symbol）+ **Rust JNI export
  全缺**（只有域方法 1123-1204（无 `Java_..._nativeListTrashed` 系导出（→ gradle
  assembleDebug FAILED
- 补链: Rust 四 export（JSON 串惯例（dk49_json_to_jstring+jstring_to_rust（restore/purge
  jboolean 直传（purgeExpired 返 len（+ UniffiAppCore.java TrashedNoteItem 类+四包装
  （org.json 解析（错误降级空表
- 待终证: cargo check + arm64 交叉编译 + gradle 全绿出包 v26a（含 DK02 桥+CALENDAR-01）

## 教训沉淀

1. **Alpha 环境 504 闪断是系统性约束**（编译窗>5min 的交付必然半接入——对向复核
   必须跑 gradle/javac 级验证（rg 代码核对不够（Alpha 的「字段面已 rg 核对」承诺
   在这里失效
2. **RFC3339 UTC 时区是移动端聚合的高频雷**（两 agent 各踩一次（startsWith/slice
   都错——日历/聚合类功能验收必须造 UTC 桩数据
