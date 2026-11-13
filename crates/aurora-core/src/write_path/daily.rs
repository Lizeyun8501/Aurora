//! B-04: 每日笔记域（write_path 拆分——纯搬移）。
use super::*;

/// 渲染每日笔记模板：{{date}} → YYYY-MM-DD；{{weekday}} → 星期X（中文）。
pub fn render_daily_template(template: &str, date: &str) -> String {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .map(|d| chrono::Datelike::weekday(&d));
    let weekday = match d {
        Some(chrono::Weekday::Mon) => "星期一",
        Some(chrono::Weekday::Tue) => "星期二",
        Some(chrono::Weekday::Wed) => "星期三",
        Some(chrono::Weekday::Thu) => "星期四",
        Some(chrono::Weekday::Fri) => "星期五",
        Some(chrono::Weekday::Sat) => "星期六",
        _ => "星期日",
    };
    template
        .replace("{{date}}", date)
        .replace("{{weekday}}", weekday)
}

/// 读取每日笔记模板（未设置用默认）。
pub async fn get_daily_template(core: &AppCore) -> Result<String, Error> {
    Ok(core
        .kv_store
        .get(DAILY_TEMPLATE_KEY)
        .await?
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_else(|| DEFAULT_DAILY_TEMPLATE.to_string()))
}

/// 设置每日笔记模板。
pub async fn set_daily_template(core: &AppCore, template: &str) -> Result<(), Error> {
    core.kv_store
        .set(DAILY_TEMPLATE_KEY, template.as_bytes())
        .await
}

/// 读取每日笔记模式（未设置默认 Auto）。
pub async fn get_daily_mode(core: &AppCore) -> Result<DailyNoteMode, Error> {
    Ok(core
        .kv_store
        .get(DAILY_MODE_KEY)
        .await?
        .map(|b| DailyNoteMode::from_str(&String::from_utf8_lossy(&b)))
        .unwrap_or_default())
}

/// 设置每日笔记模式。
pub async fn set_daily_mode(core: &AppCore, mode: DailyNoteMode) -> Result<(), Error> {
    let s = match mode {
        DailyNoteMode::Auto => "auto",
        DailyNoteMode::Manual => "manual",
        DailyNoteMode::Off => "off",
    };
    core.kv_store.set(DAILY_MODE_KEY, s.as_bytes()).await
}

/// 定位当日每日笔记（索引主路 + scan 兜底，不含创建）。
async fn locate_daily_note(
    core: &AppCore,
    unseal: Option<&SealPair>,
    date: &str,
) -> Result<Option<String>, Error> {
    if let Some(id) = core
        .kv_store
        .get(&daily_key(date))
        .await?
        .map(|b| String::from_utf8_lossy(&b).to_string())
    {
        if let Some(rec) = load_note_meta(core, &id, unseal).await? {
            if rec.kind == NoteKind::DailyNote
                && rec.daily_date.as_deref() == Some(date)
                && core.kv_store.get(&format!("trash:{id}")).await?.is_none()
            {
                return Ok(Some(id));
            }
        }
    }
    // 兜底：索引缺失/损坏时扫描 daily_date（自愈数据源）
    for (k, bytes) in core.kv_store.scan_prefix("note:").await? {
        let _ = k;
        if let Some(rec) = decode_record(&bytes, unseal) {
            if rec.kind == NoteKind::DailyNote
                && rec.daily_date.as_deref() == Some(date)
                && core
                    .kv_store
                    .get(&format!("trash:{}", rec.id))
                    .await?
                    .is_none()
            {
                // 自愈索引
                core.kv_store
                    .set(&daily_key(date), rec.id.as_bytes())
                    .await?;
                return Ok(Some(rec.id));
            }
        }
    }
    Ok(None)
}

/// 幂等确保当日每日笔记存在（DK-19 核心）：
/// - Auto：定位→无则按模板创建（进程内互斥防并发双建）；
/// - Manual：仅定位，不存在返回 None（不创建）；
/// - Off：拒绝（Error::InvalidInput）。
///
/// 成功创建后发布 `DailyNoteOpened`（投影消费做索引自愈）；定位命中也发布
/// （打开即事件——投影水位线推进与索引校验依赖消费面）。
pub async fn ensure_daily_note(
    ctx: &WriteContext,
    date: &str,
) -> Result<Option<DailyNoteRef>, Error> {
    let core = ctx.core.as_ref();
    let unseal = ctx.seal.as_ref();
    if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
        return Err(Error::InvalidInput(format!(
            "日期格式非法: '{date}'（需 YYYY-MM-DD）"
        )));
    }
    let mode = get_daily_mode(core).await?;
    if mode == DailyNoteMode::Off {
        return Err(Error::InvalidInput(
            "每日笔记功能已关闭（daily_note_mode=off）".into(),
        ));
    }
    let _guard = core.daily_note_lock.lock().await;
    if let Some(id) = locate_daily_note(core, unseal, date).await? {
        core.event_bus
            .publish(crate::event_bus::layered::AppEvent::DailyNoteOpened {
                date: date.to_string(),
                note_id: id.clone(),
            });
        return Ok(Some(DailyNoteRef {
            note_id: id,
            created: false,
        }));
    }
    if mode == DailyNoteMode::Manual {
        return Ok(None); // 「手动才创建」模式：仅定位
    }
    // Auto：创建
    let template = get_daily_template(core).await?;
    let title = render_daily_template(&template, date)
        .lines()
        .next()
        .unwrap_or(date)
        .trim_start_matches('#')
        .trim()
        .to_string();
    let title = if title.is_empty() {
        date.to_string()
    } else {
        title
    };
    let note_id = create_note(ctx, &title).await?;
    let mut rec = load_note_meta(core, &note_id, unseal)
        .await?
        .ok_or(Error::NoteNotFound {
            id: note_id.clone(),
        })?;
    rec.kind = NoteKind::DailyNote;
    rec.daily_date = Some(date.to_string());
    put_note_meta(core, &note_id, &rec, unseal).await?;
    core.kv_store
        .set(&daily_key(date), note_id.as_bytes())
        .await?;
    core.event_bus
        .publish(crate::event_bus::layered::AppEvent::DailyNoteOpened {
            date: date.to_string(),
            note_id: note_id.clone(),
        });
    info!(note_id = %note_id, date, "daily note created via WritePath (DK-19)");
    Ok(Some(DailyNoteRef {
        note_id,
        created: true,
    }))
}
