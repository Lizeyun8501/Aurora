//! FSRS 间隔重复调度 — V20 Phase 3（「FSRS 负责 Distill 环节的精准召回」）
//!
//! 基于 FSRS-4.5（开源参数集 w=[0.4072, 1.1829, 3.1262, 15.6926,
//! 7.2101, 0.5316, 1.0651, 0.0234, 1.6162, 0.1544, 1.0824, 2.6561,
//! 0.0068, 0.5431]）实现记忆三参数状态机:
//!
//! - **S 稳定性**（天）: 记忆保持 90% 概率的间隔
//! - **D 难度**（1-10）: 越高越难记
//! - **R 可提取性**（0-1）: 当前时刻的回忆概率（遗忘曲线指数衰减）
//!
//! # 调度循环
//!
//! ```text
//! 用户评分（1 Again / 2 Hard / 3 Good / 4 Easy）
//!   → R（当前可提取性）
//!   → S'/D' 更新（评分越差 S 增长越慢 / D 越高）
//!   → 下一间隔 = S' 对应 R=90% 的时间
//!   → 复习队列（R 跌破阈值 0.9 的卡片入队）
//! ```
//!
//! Phase 3 范围: 调度器核心 + 复习队列（TodayView「复习」分区数据源）。
//! Anki 导入/卡片管理 UI 是 Phase 5「FSRS 深化」。
//!
//! # 数值断言策略
//!
//! 算法常数固定 → 输出确定性。测试断言**语义性质**（单调性/边界）
//! 而非精确数值（与参考实现的对拍留 Phase 5 深化接 fsrs-rs 库时做）。

use chrono::{DateTime, Duration, Utc};

/// 评分（1-4 — FSRS 标准 Again/Hard/Good/Easy）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    fn as_u32(self) -> u32 {
        self as u32
    }
}

/// 卡片记忆状态（三参数 + 调度元数据）。
#[derive(Debug, Clone, PartialEq)]
pub struct CardState {
    /// 稳定性（天, > 0）。
    pub stability: f64,
    /// 难度（1-10）。
    pub difficulty: f64,
    /// 上次复习时刻。
    pub last_review: DateTime<Utc>,
    /// 复习次数。
    pub reps: u32,
    /// 连续失败（Again）次数。
    pub lapses: u32,
}

/// 调度输出。
#[derive(Debug, Clone, PartialEq)]
pub struct Scheduled {
    pub state: CardState,
    /// 下次到期时刻。
    pub due: DateTime<Utc>,
    /// 复习时刻的可提取性（评分依据）。
    pub retrievability: f64,
}

/// FSRS-4.5 调度器（开源参数集 — 训练自 Anki 亿级复习记录）。
pub struct FsrsScheduler {
    w: [f64; 14],
    /// 目标保持率（默认 0.9 — 「90% 概率记得」）。
    pub desired_retention: f64,
    /// 请求间隔天数上限（防爆炸 — Anki 默认 36500）。
    pub maximum_interval: f64,
}

impl Default for FsrsScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl FsrsScheduler {
    /// FSRS-4.5 默认参数集 + 目标保持率 0.9。
    pub fn new() -> Self {
        Self {
            w: [
                0.4072, 1.1829, 3.1262, 15.6926, 7.2101, 0.5316, 1.0651, 0.0234, 1.6162, 0.1544,
                1.0824, 2.6561, 0.0068, 0.5431,
            ],
            desired_retention: 0.9,
            maximum_interval: 36500.0,
        }
    }

    /// 新卡初始状态（首次评分前的虚状态 — 建立调度起点）。
    pub fn new_card(&self, now: DateTime<Utc>) -> CardState {
        CardState {
            stability: 0.0,
            difficulty: 5.0,
            last_review: now,
            reps: 0,
            lapses: 0,
        }
    }

    /// 当前可提取性 — 指数遗忘曲线 R = (1 + FACTOR * t/S)^DECAY。
    ///
    /// FACTOR = 19/81, DECAY = -0.5（FSRS-4.5 遗忘曲线）。
    pub fn retrievability(&self, state: &CardState, now: DateTime<Utc>) -> f64 {
        let elapsed_days = (now - state.last_review).num_minutes() as f64 / 1440.0;
        if state.stability <= 0.0 {
            return 0.9; // 新卡: 首评前按目标保持率
        }
        let factor = 19.0 / 81.0;
        let decay = -0.5;
        let r = (1.0 + factor * elapsed_days / state.stability).powf(decay);
        r.clamp(0.0, 1.0)
    }

    /// 应用评分 → 新状态 + 下次到期。
    pub fn review(&self, state: &CardState, rating: Rating, now: DateTime<Utc>) -> Scheduled {
        let g = rating.as_u32() as f64;
        let retrievability = self.retrievability(state, now);

        // 难度更新: D' = D - w6*(G-3) 并夹在 [1,10]
        let mut difficulty = state.difficulty - self.w[6] * (g - 3.0);
        // 初始卡（reps=0）: D0 = w4 - e^(w5*(G-1)) + 1
        if state.reps == 0 {
            difficulty = self.w[3] - (self.w[4] * (g - 1.0)).exp() + 1.0;
        }
        difficulty = difficulty.clamp(1.0, 10.0);

        // 稳定性更新
        let stability = if state.reps == 0 {
            // 初始: S0(G) = w2(G) [again: w0? — 4.5 用 w(G-1) 分档]
            self.initial_stability(g)
        } else {
            let s_success = self.next_stability_success(state, difficulty, retrievability, g);
            let s_fail = self.next_stability_fail(state, retrievability);
            if rating == Rating::Again {
                s_fail
            } else {
                s_success
            }
        };

        let reps = state.reps + 1;
        let lapses = if rating == Rating::Again {
            state.lapses + 1
        } else {
            state.lapses
        };

        // 下一间隔: R 跌到目标保持率的时间 = S * FACTOR / (R^(1/DECAY) - 1)
        let factor = 19.0 / 81.0;
        let decay = -0.5;
        let interval_days = (stability * factor / (self.desired_retention.powf(1.0 / decay) - 1.0))
            .round()
            .clamp(1.0, self.maximum_interval);

        let new_state = CardState {
            stability,
            difficulty,
            last_review: now,
            reps,
            lapses,
        };
        let due = now + Duration::days(interval_days as i64);
        Scheduled {
            state: new_state,
            due,
            retrievability,
        }
    }

    /// 初始稳定性分档: S0 = w[G-1]（w0..w3 对应 Again/Hard/Good/Easy）。
    fn initial_stability(&self, g: f64) -> f64 {
        let idx = (g as usize).clamp(1, 4) - 1;
        self.w[idx].max(0.1)
    }

    /// 成功复习后的稳定性（FSRS-4.5 S_recall 公式）。
    fn next_stability_success(
        &self,
        state: &CardState,
        difficulty: f64,
        retrievability: f64,
        g: f64,
    ) -> f64 {
        // S' = S * (1 + e^(w8) * (11 - D) * S^(-w9) * (e^(w10*R) - 1) * hard_penalty)
        let hard_penalty = if g == 2.0 {
            self.w[11] // hard
        } else {
            1.0
        };
        let easy_bonus = if g == 4.0 { self.w[12] } else { 1.0 };
        let s_dot = (self.w[7]
            * (11.0 - difficulty)
            * state.stability.powf(-self.w[8])
            * ((self.w[9] * retrievability).exp() - 1.0)
            * hard_penalty)
            .exp();
        (state.stability * (1.0 + s_dot * easy_bonus)).max(0.1)
    }

    /// 失败（Again）后的稳定性 — 遗忘后重学: S_f = w11 * D^(-w12) * ((S+1)^w13 - 1) * e^(w10*R)
    fn next_stability_fail(&self, state: &CardState, retrievability: f64) -> f64 {
        let s_after = self.w[10]
            * state.difficulty.powf(-self.w[11])
            * ((state.stability + 1.0).powf(self.w[12]) - 1.0)
            * (self.w[9] * retrievability).exp();
        s_after.max(0.1)
    }

    /// 复习队列判定: 可提取性跌破目标 → 需复习（TodayView「复习」分区）。
    pub fn needs_review(&self, state: &CardState, now: DateTime<Utc>) -> bool {
        let r = self.retrievability(state, now);
        r < self.desired_retention
    }
}

// ===========================================================================
// 复习队列（TodayView「复习」分区数据源 — V20 Phase 3）
// ===========================================================================

/// 复习条目（卡片状态 + 归属笔记锚定）。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    /// 卡片 ID（笔记块级锚定 — Distill 的知识单元）。
    pub card_id: String,
    /// 关联笔记。
    pub note_id: String,
    pub state: CardState,
    pub due: DateTime<Utc>,
}

/// 评分日志条目（revlog — 留存率/今日复习量数据源；环形上限内保留）。
/// 序列化走快照手写（rating 数字化），core 类型零 serde 侵入。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewLogEntry {
    /// 卡片 ID。
    pub card_id: String,
    /// 本次评分。
    pub rating: Rating,
    /// 评分时刻。
    pub ts: DateTime<Utc>,
}

/// revlog 环形上限（无界增长防爆破——超限丢最旧）。
const REVIEW_LOG_CAP: usize = 10_000;

/// R17：FSRS 统计面聚合结果（review_stats 返回值）。
pub struct ReviewStats {
    /// 总卡数。
    pub total_cards: usize,
    /// 当前到期数（与 stats() 口径一致）。
    pub due_now: usize,
    /// 今日已评分次数（Utc 日界 00:00 起——与 due_items 同用 Utc 会计口径，
    /// 本地时区偏差可接受，诚实边界）。
    pub today_reviews: usize,
    /// 今日 Again 占比（今日 reviews==0 → None）。
    pub today_again_rate: Option<f64>,
    /// 未来 7 天到期分布（[0]=今日 24h 窗口含已到期；[n]=(now+24n, now+24(n+1)]；
    /// 第 6 天尾=now+168h 截止）。
    pub due_7d: [usize; 7],
}

/// R18：留存率曲线日统计（review_daily_series 返回元素——按日时间序列）。
pub struct ReviewDayStat {
    /// 日期（"YYYY-MM-DD"，Utc 会计口径——与 review_stats 同偏差边界）。
    pub date: String,
    /// 当日评分次数。
    pub reviews: usize,
    /// 当日 Again 占比（当日 0 复习 → None）。
    pub again_rate: Option<f64>,
}

/// 复习队列: 到期/可提取性跌破阈值的卡片集合（内存读模型 —
/// 与 TaskProjection 同模式; 持久化接 KVStore 留后续 PR）。
pub struct ReviewQueue {
    scheduler: FsrsScheduler,
    items: std::sync::RwLock<Vec<ReviewItem>>,
    /// 评分日志（revlog）——add_card 首评 + review_card 成功评分追加。
    review_log: std::sync::RwLock<Vec<ReviewLogEntry>>,
}

impl ReviewQueue {
    pub fn new() -> Self {
        Self {
            scheduler: FsrsScheduler::new(),
            items: std::sync::RwLock::new(Vec::new()),
            review_log: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// 追加评分日志（环形上限——超限丢最旧）。
    fn push_log(&self, card_id: &str, rating: Rating, ts: DateTime<Utc>) {
        let mut log = self.review_log.write().unwrap();
        log.push(ReviewLogEntry {
            card_id: card_id.to_string(),
            rating,
            ts,
        });
        if log.len() > REVIEW_LOG_CAP {
            let overflow = log.len() - REVIEW_LOG_CAP;
            log.drain(0..overflow);
        }
    }

    /// 注册卡片（首次评分 — 笔记 Distill 时调用）。
    pub fn add_card(&self, card_id: &str, note_id: &str, rating: Rating) -> Scheduled {
        let now = Utc::now();
        let fresh = self.scheduler.new_card(now);
        let scheduled = self.scheduler.review(&fresh, rating, now);
        let mut items = self.items.write().unwrap();
        // 幂等: 同 card_id 覆盖
        items.retain(|i| i.card_id != card_id);
        items.push(ReviewItem {
            card_id: card_id.to_string(),
            note_id: note_id.to_string(),
            state: scheduled.state.clone(),
            due: scheduled.due,
        });
        // 首评入日志（建卡即评分）。
        self.push_log(card_id, rating, now);
        scheduled
    }

    /// 复习一次（评分 → 更新状态）。
    pub fn review_card(&self, card_id: &str, rating: Rating) -> Option<Scheduled> {
        let now = Utc::now();
        let mut items = self.items.write().unwrap();
        let item = items.iter_mut().find(|i| i.card_id == card_id)?;
        let scheduled = self.scheduler.review(&item.state, rating, now);
        item.state = scheduled.state.clone();
        item.due = scheduled.due;
        // 评分成功才记（None 早返不入日志）。
        self.push_log(card_id, rating, now);
        Some(scheduled)
    }

    /// 到期卡片（TodayView「复习」分区 — due ≤ now 或 R 跌破阈值）。
    pub fn due_items(&self, now: DateTime<Utc>) -> Vec<ReviewItem> {
        let items = self.items.read().unwrap();
        items
            .iter()
            .filter(|i| i.due <= now || self.scheduler.needs_review(&i.state, now))
            .cloned()
            .collect()
    }

    /// 队列统计（今日页头部: 待复习数/总卡数）。
    pub fn stats(&self) -> (usize, usize) {
        let now = Utc::now();
        let items = self.items.read().unwrap();
        let due = items
            .iter()
            .filter(|i| i.due <= now || self.scheduler.needs_review(&i.state, now))
            .count();
        (due, items.len())
    }

    /// R17：FSRS 统计面聚合（统计面板数据源——与 stats() 并列，旧签名零破坏）。
    pub fn review_stats(&self, now: DateTime<Utc>) -> ReviewStats {
        // 锁序纪律：items 先、log 后（add_card/review_card 同序——防死锁）。
        let items = self.items.read().unwrap();
        let log = self.review_log.read().unwrap();
        // 「今日」=Utc 日界 00:00 起（与 due_items 同用 Utc 会计口径，
        // 本地时区偏差可接受——诚实边界）。
        let day_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always a valid naive time")
            .and_utc();
        let today_reviews = log.iter().filter(|e| e.ts >= day_start).count();
        let today_again = log
            .iter()
            .filter(|e| e.ts >= day_start && e.rating == Rating::Again)
            .count();
        let due_now = items
            .iter()
            .filter(|i| i.due <= now || self.scheduler.needs_review(&i.state, now))
            .count();
        // 7 天到期分布：[0]=今日 24h 窗口（无下界，含已到期）；
        // [n]=(now+24n, now+24(n+1)]；第 6 天尾=now+168h 截止。
        let mut due_7d = [0usize; 7];
        for i in items.iter() {
            for (day, slot) in due_7d.iter_mut().enumerate() {
                let lo = now + Duration::hours(24 * day as i64);
                let hi = lo + Duration::hours(24);
                if i.due <= hi && (day == 0 || i.due > lo) {
                    *slot += 1;
                    break;
                }
            }
        }
        ReviewStats {
            total_cards: items.len(),
            due_now,
            today_reviews,
            today_again_rate: if today_reviews == 0 {
                None
            } else {
                Some(today_again as f64 / today_reviews as f64)
            },
            due_7d,
        }
    }

    /// R18：留存率曲线聚合——最近 days 天按日评分量+Again 率时间序列。
    /// 窗口=[今日回推 days-1 天起，今日 24h 收尾]（含今日，末元素未完——
    /// 诚实边界）；Utc 日界与 review_stats 同口径。只读 log 单锁（无 items
    /// 依赖，无锁序交叉面）。单遍历分桶 O(log)；days=0 → 空序列。
    pub fn review_daily_series(&self, now: DateTime<Utc>, days: usize) -> Vec<ReviewDayStat> {
        let log = self.review_log.read().unwrap();
        let today_start = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always a valid naive time")
            .and_utc();
        let window_start = today_start - Duration::hours(24 * (days as i64 - 1).max(0));
        let mut counts = vec![0usize; days];
        let mut agains = vec![0usize; days];
        for e in log.iter() {
            if e.ts < window_start || e.ts >= today_start + Duration::hours(24) {
                continue;
            }
            // Duration 非负（window_start 已滤）→ num_hours 向零取整=floor。
            let idx = (e.ts - window_start).num_hours() as usize / 24;
            if idx < days {
                counts[idx] += 1;
                if e.rating == Rating::Again {
                    agains[idx] += 1;
                }
            }
        }
        (0..days)
            .map(|d| ReviewDayStat {
                date: (window_start + Duration::hours(24 * d as i64))
                    .date_naive()
                    .format("%Y-%m-%d")
                    .to_string(),
                reviews: counts[d],
                again_rate: if counts[d] == 0 {
                    None
                } else {
                    Some(agains[d] as f64 / counts[d] as f64)
                },
            })
            .collect()
    }

    /// R12：块卡同步（按块建卡的幂等语义升级）——注册 keep_ids 中缺失的卡
    /// （新卡首评 Good），移除同笔记内不在 keep 集的旧块卡（heading 改/删致
    /// hash 锚定失效）。笔记级卡与其他笔记不动；已存在卡 FSRS 进度保留。
    /// 返回 (added, removed)。
    pub fn sync_block_cards(&self, note_id: &str, keep_ids: &[String]) -> (usize, usize) {
        let now = Utc::now();
        let mut items = self.items.write().unwrap();
        let keep: std::collections::HashSet<&str> = keep_ids.iter().map(|s| s.as_str()).collect();
        let before = items.len();
        items.retain(|i| {
            if i.note_id != note_id {
                return true; // 他笔记不动
            }
            !is_generated_card_id(&i.card_id) || keep.contains(i.card_id.as_str())
        });
        let removed = before - items.len();
        let mut added = 0;
        for id in keep_ids {
            if items.iter().any(|i| i.card_id == *id) {
                continue; // 已存在——FSRS 进度保留
            }
            let fresh = self.scheduler.new_card(now);
            let scheduled = self.scheduler.review(&fresh, Rating::Good, now);
            items.push(ReviewItem {
                card_id: id.clone(),
                note_id: note_id.to_string(),
                state: scheduled.state.clone(),
                due: scheduled.due,
            });
            added += 1;
        }
        (added, removed)
    }

    /// R13（切片三-1）：全量快照——desktop 壳 KVStore 落盘用（重启恢复）。
    /// 手写序列化（core 类型零侵入）；version 迁移位。
    pub fn snapshot_json(&self) -> serde_json::Value {
        // 锁序纪律：items 先、log 后。
        let items = self.items.read().unwrap();
        let log = self.review_log.read().unwrap();
        serde_json::json!({
            "version": 1,
            "items": items
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "card_id": i.card_id,
                        "note_id": i.note_id,
                        "due": i.due.to_rfc3339(),
                        "stability": i.state.stability,
                        "difficulty": i.state.difficulty,
                        "last_review": i.state.last_review.to_rfc3339(),
                        "reps": i.state.reps,
                        "lapses": i.state.lapses,
                    })
                })
                .collect::<Vec<_>>(),
            "review_log": log
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "card_id": e.card_id,
                        "rating": e.rating.as_u32(),
                        "ts": e.ts.to_rfc3339(),
                    })
                })
                .collect::<Vec<_>>(),
        })
    }

    /// R13（切片三-1）：从快照恢复——逐条覆盖同 card_id（恢复语义）；
    /// version 不符或条目字段缺失跳过该条（坏数据不毒化队列），返回恢复数。
    pub fn restore_json(&self, snap: &serde_json::Value) -> usize {
        if snap.get("version").and_then(|v| v.as_u64()) != Some(1) {
            return 0;
        }
        let arr = match snap.get("items").and_then(|v| v.as_array()) {
            Some(a) => a.clone(),
            None => return 0,
        };
        let mut restored = 0;
        let mut items = self.items.write().unwrap();
        for it in arr {
            let card_id = it.get("card_id").and_then(|v| v.as_str()).map(String::from);
            let note_id = it.get("note_id").and_then(|v| v.as_str()).map(String::from);
            let due = it
                .get("due")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc));
            let stability = it.get("stability").and_then(|v| v.as_f64());
            let difficulty = it.get("difficulty").and_then(|v| v.as_f64());
            let last_review = it
                .get("last_review")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc));
            let reps = it.get("reps").and_then(|v| v.as_u64()).map(|v| v as u32);
            let lapses = it.get("lapses").and_then(|v| v.as_u64()).map(|v| v as u32);
            let (
                Some(card_id),
                Some(note_id),
                Some(due),
                Some(stability),
                Some(difficulty),
                Some(last_review),
                Some(reps),
                Some(lapses),
            ) = (
                card_id,
                note_id,
                due,
                stability,
                difficulty,
                last_review,
                reps,
                lapses,
            )
            else {
                continue; // 字段缺失跳过（诚实边界：坏条目不入队）
            };
            items.retain(|x| x.card_id != card_id);
            items.push(ReviewItem {
                card_id,
                note_id,
                state: CardState {
                    stability,
                    difficulty,
                    last_review,
                    reps,
                    lapses,
                },
                due,
            });
            restored += 1;
        }
        // R17：评分日志恢复——replace 语义（快照为全量事实源；旧快照无
        // review_log 字段 → 默认空，version 不 bump，字段缺省即兼容）。
        // 坏条目跳过不毒化（同 items 语义）；超限环形截断。
        {
            let mut log = self.review_log.write().unwrap();
            log.clear();
            if let Some(log_arr) = snap.get("review_log").and_then(|v| v.as_array()) {
                for e in log_arr {
                    let Some(card_id) = e.get("card_id").and_then(|v| v.as_str()).map(String::from)
                    else {
                        continue;
                    };
                    let rating = match e.get("rating").and_then(|v| v.as_u64()) {
                        Some(1) => Some(Rating::Again),
                        Some(2) => Some(Rating::Hard),
                        Some(3) => Some(Rating::Good),
                        Some(4) => Some(Rating::Easy),
                        _ => None,
                    };
                    let Some(rating) = rating else {
                        continue;
                    };
                    let Some(ts) = e
                        .get("ts")
                        .and_then(|v| v.as_str())
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.with_timezone(&Utc))
                    else {
                        continue;
                    };
                    log.push(ReviewLogEntry {
                        card_id,
                        rating,
                        ts,
                    });
                }
                if log.len() > REVIEW_LOG_CAP {
                    let overflow = log.len() - REVIEW_LOG_CAP;
                    log.drain(0..overflow);
                }
            }
        }
        restored
    }
}

impl Default for ReviewQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// R12：块卡形态判定（core 单一事实源——desktop due 重拼/同步清理共用；
/// TS 端正则 /:b[0-9a-f]{8}$/ 互指同语义）。
pub fn is_block_card_id(card_id: &str) -> bool {
    card_id
        .rfind(":b")
        .map(|i| {
            let tail = &card_id[i + 2..];
            tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .unwrap_or(false)
}

/// R14：生成卡形态判定（:b 块卡 / :l 列表卡——同 hash8 尾族）。
/// 同步清理语义用（笔记级卡不动）；块卡重拼仍用 is_block_card_id 分流。
pub fn is_generated_card_id(card_id: &str) -> bool {
    card_id
        .rfind(":b")
        .or_else(|| card_id.rfind(":l"))
        .map(|i| {
            let tail = &card_id[i + 2..];
            tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 新卡 Good → 间隔正增长 + 状态建立。
    #[test]
    fn new_card_good_review() {
        let s = FsrsScheduler::new();
        let now = Utc::now();
        let card = s.new_card(now);
        let out = s.review(&card, Rating::Good, now);
        assert!(
            out.state.stability > 0.5,
            "S0(Good) 应有合理初值: {}",
            out.state.stability
        );
        assert!(out.due > now, "下次到期在未来");
        assert_eq!(out.state.reps, 1);
    }

    /// 评分单调性: Easy 间隔 > Good > Hard > Again（同起点）。
    #[test]
    fn interval_monotonic_with_rating() {
        let s = FsrsScheduler::new();
        let now = Utc::now();
        let base = s.new_card(now);
        let mut intervals = Vec::new();
        for r in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
            let out = s.review(&base, r, now);
            intervals.push((out.due - now).num_days());
        }
        assert!(
            intervals.windows(2).all(|w| w[0] <= w[1]),
            "间隔随评分单调不减: {intervals:?}"
        );
    }

    /// 成功复习稳定性增长（Good 二连 → S 增长）。
    #[test]
    fn stability_grows_on_successive_good() {
        let s = FsrsScheduler::new();
        let now = Utc::now();
        let card = s.new_card(now);
        let first = s.review(&card, Rating::Good, now);
        let next = s.review(&first.state, Rating::Good, first.due);
        assert!(
            next.state.stability > first.state.stability,
            "S 应增长: {} → {}",
            first.state.stability,
            next.state.stability
        );
    }

    /// Again → 稳定性回缩（遗忘）+ lapses 计数。
    #[test]
    fn again_resets_stability_and_counts_lapse() {
        let s = FsrsScheduler::new();
        let now = Utc::now();
        let card = s.new_card(now);
        let good = s.review(&card, Rating::Good, now);
        let fail = s.review(&good.state, Rating::Again, good.due);
        assert_eq!(fail.state.lapses, 1);
        assert!(
            fail.state.stability < good.state.stability,
            "遗忘后 S 回缩: {} → {}",
            good.state.stability,
            fail.state.stability
        );
    }

    /// 遗忘曲线: R 随时间衰减（3 天 > 0 天）。
    #[test]
    fn retrievability_decays_over_time() {
        let s = FsrsScheduler::new();
        let now = Utc::now();
        let card = s.new_card(now);
        let out = s.review(&card, Rating::Good, now);
        let r_now = s.retrievability(&out.state, now);
        let r_later = s.retrievability(&out.state, now + Duration::days(3));
        assert!(r_now > r_later, "R 衰减: {r_now} > {r_later}");
        assert!(r_later > 0.0 && r_later < 1.0);
    }

    /// 复习队列闭环: 注册→到期→复习→再调度。
    #[test]
    fn review_queue_lifecycle() {
        let q = ReviewQueue::new();
        q.add_card("c1", "n1", Rating::Good);
        // 新卡 Good → due 在未来（不在到期队列）
        assert!(q.due_items(Utc::now()).is_empty());

        // 时间快进到到期 → 入队
        let (_, total) = q.stats();
        assert_eq!(total, 1);
        let future = Utc::now() + Duration::days(30);
        let due = q.due_items(future);
        assert_eq!(due.len(), 1, "30 天后应到期（R 跌破 0.9）");

        // 复习（Good）→ 重新调度（due > now — S 增长推迟）
        let now2 = Utc::now();
        let out = q.review_card("c1", Rating::Good).unwrap();
        assert!(out.due > now2, "复习后推迟: {:?}", out.due);
        assert_eq!(out.state.reps, 2);
        // 二连 Good 后 S 增长
        assert!(out.state.stability > 1.0, "S 增长: {}", out.state.stability);
    }

    // ── R12 块卡同步 ─────────────────────────────────────────────

    fn block_id(nid: &str, h: &str) -> String {
        format!("card:note:{}:b{}", nid, h) // h 手填 8hex
    }

    #[test]
    fn block_id_shape_judge() {
        assert!(is_block_card_id(&block_id("n1", "277e29c9")));
        assert!(!is_block_card_id("card:note:n1")); // 笔记级
        assert!(!is_block_card_id("card:note:note:bblue")); // 尾非 8hex
        assert!(!is_block_card_id("card:note:n1:b277e29")); // 7 位
                                                            // R14：生成卡族（:b/:l 同判——列表卡进同步清理语义）
        assert!(is_generated_card_id(&block_id("n1", "277e29c9")));
        assert!(is_generated_card_id(&format!(
            "card:note:n1:l{}",
            "277e29c9"
        )));
        assert!(!is_generated_card_id("card:note:n1"));
        assert!(!is_generated_card_id("card:note:n1:lblue")); // 非 8hex
    }

    #[test]
    fn sync_adds_missing_and_keeps_progress() {
        let q = ReviewQueue::new();
        let id = block_id("n1", "277e29c9");
        let s1 = q.add_card(&id, "n1", Rating::Good);
        let reps1 = s1.state.reps;
        // 再评分推进进度
        let s2 = q.review_card(&id, Rating::Good).unwrap();
        let (added, removed) = q.sync_block_cards("n1", std::slice::from_ref(&id));
        assert_eq!((added, removed), (0, 0));
        let binding = q.due_items(Utc::now() + Duration::days(3650));
        let item = binding.iter().find(|i| i.card_id == id).unwrap();
        assert_eq!(item.state.reps, s2.state.reps, "FSRS 进度保留");
        assert!(reps1 >= 1);
    }

    #[test]
    fn sync_removes_stale_block_cards_only() {
        let q = ReviewQueue::new();
        let stale = block_id("n1", "277e29c9");
        let fresh = block_id("n1", "86023237");
        let note_card = "card:note:n1".to_string();
        let other = block_id("n2", "277e29c9");
        q.add_card(&stale, "n1", Rating::Good);
        q.add_card(&note_card, "n1", Rating::Good);
        q.add_card(&other, "n2", Rating::Good);
        // 同步 n1 只保留 fresh
        let (added, removed) = q.sync_block_cards("n1", std::slice::from_ref(&fresh));
        assert_eq!((added, removed), (1, 1), "清 stale 建新");
        let total = q.stats().1;
        assert_eq!(total, 3, "笔记级卡+n2 块卡+fresh 块卡");
        // fresh 已建（added=1 且存在）
        let again = q.sync_block_cards("n1", std::slice::from_ref(&fresh));
        assert_eq!(again, (0, 0), "幂等二跑");
    }

    // ── R13 持久化快照 ───────────────────────────────────────────

    #[test]
    fn snapshot_roundtrip_preserves_progress() {
        let q1 = ReviewQueue::new();
        let id = block_id("n1", "277e29c9");
        q1.add_card(&id, "n1", Rating::Good);
        q1.review_card(&id, Rating::Good).unwrap();
        let snap = q1.snapshot_json();

        let q2 = ReviewQueue::new();
        let restored = q2.restore_json(&snap);
        assert_eq!(restored, 1);
        // 恢复后进度/状态一致（reps=2、due 时刻同）
        let s1 = q1.due_items(Utc::now() + Duration::days(3650));
        let s2 = q2.due_items(Utc::now() + Duration::days(3650));
        assert_eq!(s1.len(), s2.len());
        let a = s1.iter().find(|i| i.card_id == id).unwrap();
        let b = s2.iter().find(|i| i.card_id == id).unwrap();
        assert_eq!(a.state, b.state, "CardState 全等");
        assert_eq!(a.due, b.due);
        // 恢复后可继续调度
        let out = q2.review_card(&id, Rating::Good).unwrap();
        assert!(out.state.reps >= 3);
    }

    #[test]
    fn restore_skips_bad_entries() {
        let q = ReviewQueue::new();
        // version 错 → 0
        assert_eq!(
            q.restore_json(&serde_json::json!({"version": 99, "items": []})),
            0
        );
        // 字段缺失条目跳过、好条目收
        let n = q.restore_json(&serde_json::json!({
            "version": 1,
            "items": [
                {"card_id": "card:note:n1", "note_id": "n1", "due": "2030-01-01T00:00:00Z",
                 "stability": 2.0, "difficulty": 5.0, "last_review": "2026-10-24T00:00:00Z",
                 "reps": 3, "lapses": 0},
                {"card_id": "card:note:n2", "note_id": "n2"}
            ]
        }));
        assert_eq!(n, 1, "坏条目跳过");
        assert_eq!(q.stats().1, 1);
    }

    // ── R17 评分日志（revlog）+ 统计聚合 ─────────────────────────

    #[test]
    fn review_log_append_semantics() {
        let q = ReviewQueue::new();
        // 建卡首评 → 1 条
        q.add_card("c1", "n1", Rating::Good);
        // 成功复习 → +1
        q.review_card("c1", Rating::Easy).unwrap();
        // 卡不存在（None 早返）→ 不记
        assert!(q.review_card("ghost", Rating::Good).is_none());

        let snap = q.snapshot_json();
        let log = snap.get("review_log").and_then(|v| v.as_array()).unwrap();
        assert_eq!(log.len(), 2, "首评+成功复习各一条");
        // 顺序：首评 Good → 复习 Easy
        assert_eq!(log[0]["card_id"], "c1");
        assert_eq!(log[0]["rating"], 3);
        assert_eq!(log[1]["rating"], 4);
    }

    #[test]
    fn review_log_cap_ring_drops_oldest() {
        let q = ReviewQueue::new();
        // 10_001 张卡 → 环形丢最旧 1 条（CAP=10_000）
        for i in 0..=10_000 {
            q.add_card(&format!("c{i}"), "n1", Rating::Good);
        }
        let snap = q.snapshot_json();
        let log = snap.get("review_log").and_then(|v| v.as_array()).unwrap();
        assert_eq!(log.len(), 10_000, "上限截断");
        assert_eq!(log[0]["card_id"], "c1", "最旧 c0 已丢");
        assert_eq!(log[9_999]["card_id"], "c10000", "最新保留");
    }

    #[test]
    fn review_stats_aggregates_and_windows() {
        // 锚定当日 12:00 UTC——now±h 相对量在 UTC 午夜附近跑测试时不跨日
        // （2026-10-26 UTC 00:57 实测踩坑：rfc(-1) 落昨日致今日计数归零）。
        let now = Utc::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon is always a valid naive time")
            .and_utc();
        let rfc = |h: i64| (now + Duration::hours(h)).to_rfc3339();
        let q = ReviewQueue::new();
        // 三卡 due：今日窗(12h) / 明日窗(36h) / 8 天外(200h 不计入)
        // 两条今日日志：1 Again + 1 Good → again_rate 0.5
        let n = q.restore_json(&serde_json::json!({
            "version": 1,
            "items": [
                {"card_id": "a", "note_id": "n1", "due": rfc(12), "stability": 2.0,
                 "difficulty": 5.0, "last_review": rfc(-12), "reps": 1, "lapses": 0},
                {"card_id": "b", "note_id": "n1", "due": rfc(36), "stability": 2.0,
                 "difficulty": 5.0, "last_review": rfc(-12), "reps": 1, "lapses": 0},
                {"card_id": "c", "note_id": "n1", "due": rfc(200), "stability": 9.0,
                 "difficulty": 5.0, "last_review": rfc(-12), "reps": 1, "lapses": 0}
            ],
            "review_log": [
                {"card_id": "a", "rating": 1, "ts": rfc(-1)},
                {"card_id": "b", "rating": 3, "ts": rfc(-2)}
            ]
        }));
        assert_eq!(n, 3);
        let s = q.review_stats(now);
        assert_eq!(s.total_cards, 3);
        assert_eq!(s.today_reviews, 2);
        assert_eq!(s.today_again_rate, Some(0.5));
        assert_eq!(s.due_7d, [1, 1, 0, 0, 0, 0, 0], "今日窗/明日窗/8天外");
        // 窗口边界：恰 now+24h 落今日窗尾（含）；now+48h 落明日窗
        let q2 = ReviewQueue::new();
        let now2 = Utc::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon is always a valid naive time")
            .and_utc();
        let rfc2 = |h: i64| (now2 + Duration::hours(h)).to_rfc3339();
        q2.restore_json(&serde_json::json!({
            "version": 1,
            "items": [
                {"card_id": "edge0", "note_id": "n1", "due": rfc2(24), "stability": 2.0,
                 "difficulty": 5.0, "last_review": rfc2(-12), "reps": 1, "lapses": 0},
                {"card_id": "edge1", "note_id": "n1", "due": rfc2(48), "stability": 2.0,
                 "difficulty": 5.0, "last_review": rfc2(-12), "reps": 1, "lapses": 0}
            ]
        }));
        let s2 = q2.review_stats(now2);
        assert_eq!(s2.due_7d[0], 1, "恰+24h 含于今日窗尾");
        assert_eq!(s2.due_7d[1], 1, "恰+48h 含于明日窗尾");
        assert_eq!(s2.due_7d[2], 0);
    }

    #[test]
    fn review_daily_series_aggregates_and_day_bounds() {
        // 锚定当日 12:00 UTC（同 review_stats 测试——UTC 午夜附近跑不跨日）。
        let now = Utc::now()
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .expect("noon is always a valid naive time")
            .and_utc();
        let rfc = |h: i64| (now + Duration::hours(h)).to_rfc3339();
        let q = ReviewQueue::new();
        // 三天日志：前天 1 条(Again) / 昨天 2 条(1 Again+1 Good) / 今日 3 条(全 Good) /
        // 4 天前 1 条（days=3 窗外，不计）。
        q.restore_json(&serde_json::json!({
            "version": 1,
            "items": [],
            "review_log": [
                {"card_id": "a", "rating": 1, "ts": rfc(-49)},
                {"card_id": "b", "rating": 3, "ts": rfc(-30)},
                {"card_id": "c", "rating": 1, "ts": rfc(-25)},
                {"card_id": "d", "rating": 3, "ts": rfc(-3)},
                {"card_id": "e", "rating": 3, "ts": rfc(-2)},
                {"card_id": "f", "rating": 4, "ts": rfc(-1)},
                {"card_id": "g", "rating": 2, "ts": rfc(-96)}
            ]
        }));
        let series = q.review_daily_series(now, 3);
        assert_eq!(series.len(), 3);
        // [0]=前天：1 条，again 1.0；[1]=昨天：2 条 again 0.5；[2]=今日：3 条 again 0.0
        assert_eq!(series[0].reviews, 1);
        assert_eq!(series[0].again_rate, Some(1.0));
        assert_eq!(series[1].reviews, 2);
        assert_eq!(series[1].again_rate, Some(0.5));
        assert_eq!(series[2].reviews, 3);
        assert_eq!(series[2].again_rate, Some(0.0));
        // 日期字符串=Utc 会计日（末元素=今日）
        let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();
        assert_eq!(series[2].date, today);
        // 日界边界：恰今日 00:00 的评分归今日（idx=2），恰前天 00:00 归前天（idx=0）
        let q2 = ReviewQueue::new();
        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let at = |h: i64| (today_start + Duration::hours(h)).to_rfc3339();
        q2.restore_json(&serde_json::json!({
            "version": 1,
            "items": [],
            "review_log": [
                {"card_id": "edge0", "rating": 3, "ts": at(0)},
                {"card_id": "edge1", "rating": 1, "ts": at(-48)}
            ]
        }));
        let s2 = q2.review_daily_series(today_start + Duration::hours(12), 3);
        assert_eq!(s2[0].reviews, 1, "恰前天 00:00 归窗口首日");
        assert_eq!(s2[0].again_rate, Some(1.0));
        assert_eq!(s2[2].reviews, 1, "恰今日 00:00 归今日");
        assert_eq!(s2[2].again_rate, Some(0.0));
    }

    #[test]
    fn review_daily_series_empty_and_zero_days() {
        let q = ReviewQueue::new();
        // 空日志：全 0 + None；days=0 → 空序列
        let series = q.review_daily_series(Utc::now(), 7);
        assert_eq!(series.len(), 7);
        assert!(series
            .iter()
            .all(|d| d.reviews == 0 && d.again_rate.is_none()));
        assert!(q.review_daily_series(Utc::now(), 0).is_empty());
    }

    #[test]
    fn restore_old_snapshot_without_log_is_compatible() {
        let q = ReviewQueue::new();
        // 旧快照（R13 格式——无 review_log 字段）→ 默认空日志，items 正常恢复
        let n = q.restore_json(&serde_json::json!({
            "version": 1,
            "items": [
                {"card_id": "old", "note_id": "n1", "due": "2030-01-01T00:00:00Z",
                 "stability": 2.0, "difficulty": 5.0, "last_review": "2026-10-24T00:00:00Z",
                 "reps": 3, "lapses": 0}
            ]
        }));
        assert_eq!(n, 1);
        let s = q.review_stats(Utc::now());
        assert_eq!(s.total_cards, 1);
        assert_eq!(s.today_reviews, 0, "旧快照日志默认空");
        assert_eq!(s.today_again_rate, None);
        // 恢复后新评分正常累积（clear 语义不残留）
        q.review_card("old", Rating::Good).unwrap();
        let snap = q.snapshot_json();
        let log = snap.get("review_log").and_then(|v| v.as_array()).unwrap();
        assert_eq!(log.len(), 1, "恢复后评分重新累积");
    }
}
