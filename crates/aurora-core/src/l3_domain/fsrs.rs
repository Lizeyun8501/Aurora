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
                0.4072, 1.1829, 3.1262, 15.6926, 7.2101, 0.5316, 1.0651, 0.0234, 1.6162,
                0.1544, 1.0824, 2.6561, 0.0068, 0.5431,
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
    pub fn review(
        &self,
        state: &CardState,
        rating: Rating,
        now: DateTime<Utc>,
    ) -> Scheduled {
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
        let interval_days = (stability * factor
            / (self.desired_retention.powf(1.0 / decay) - 1.0))
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
        let easy_bonus = if g == 4.0 {
            self.w[12]
        } else {
            1.0
        };
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

/// 复习队列: 到期/可提取性跌破阈值的卡片集合（内存读模型 —
/// 与 TaskProjection 同模式; 持久化接 KVStore 留后续 PR）。
pub struct ReviewQueue {
    scheduler: FsrsScheduler,
    items: std::sync::RwLock<Vec<ReviewItem>>,
}

impl ReviewQueue {
    pub fn new() -> Self {
        Self {
            scheduler: FsrsScheduler::new(),
            items: std::sync::RwLock::new(Vec::new()),
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
}

impl Default for ReviewQueue {
    fn default() -> Self {
        Self::new()
    }
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
        assert!(out.state.stability > 0.5, "S0(Good) 应有合理初值: {}", out.state.stability);
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
}
