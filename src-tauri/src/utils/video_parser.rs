//! 视频文件名解析工具
//!
//! 用于从视频文件名中解析时间窗口信息

use chrono::{DateTime, Local, NaiveDateTime, TimeZone, Utc};

/// 从视频文件名中解析时间窗口
///
/// 支持两种格式：
/// 1. segment_YYYYMMDDHHMMSS_YYYYMMDDHHMMSS
/// 2. YYYYMMDDHHMMSS-YYYYMMDDHHMMSS
///
/// # 参数
/// - `stem`: 文件名（不含扩展名）
///
/// # 返回
/// - `Some((start, end))`: 解析成功，返回开始和结束时间（UTC时区）
/// - `None`: 解析失败
pub fn parse_video_window_from_stem(
    stem: &str,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    // 处理 segment_YYYYMMDDHHMMSS_YYYYMMDDHHMMSS 格式
    if stem.starts_with("segment_") {
        let parts: Vec<&str> = stem
            .strip_prefix("segment_")?
            .split('_')
            .filter(|p| !p.is_empty())
            .collect();

        if parts.len() != 2 {
            return None;
        }

        let start = parts[0];
        let end = parts[1];

        if start.len() != 12 || end.len() != 12 {
            return None;
        }

        let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
        let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

        // 视频文件名中的时间是本地时间，需要先转为本地时区，再转为 UTC
        return Some((
            Local
                .from_local_datetime(&start_naive)
                .single()?
                .with_timezone(&Utc),
            Local
                .from_local_datetime(&end_naive)
                .single()?
                .with_timezone(&Utc),
        ));
    }

    // 处理带连字符的旧格式 YYYYMMDDHHMMSS-YYYYMMDDHHMMSS
    let cleaned: String = stem
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .collect();

    let mut parts = cleaned.split('-').filter(|p| !p.is_empty());
    let start = parts.next()?;
    let end = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    if start.len() != 12 || end.len() != 12 {
        return None;
    }

    let start_naive = NaiveDateTime::parse_from_str(start, "%Y%m%d%H%M").ok()?;
    let end_naive = NaiveDateTime::parse_from_str(end, "%Y%m%d%H%M").ok()?;

    // 视频文件名中的时间是本地时间，需要先转为本地时区，再转为 UTC
    Some((
        Local
            .from_local_datetime(&start_naive)
            .single()?
            .with_timezone(&Utc),
        Local
            .from_local_datetime(&end_naive)
            .single()?
            .with_timezone(&Utc),
    ))
}
