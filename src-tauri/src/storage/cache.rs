// 数据库缓存层 - 使用 LRU 缓存加速频繁访问

use super::models::*;
use super::repository::DatabaseRepository;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 简单的 LRU 缓存实现
struct LruCache<K: Eq + std::hash::Hash + Clone, V: Clone> {
    cache: std::collections::HashMap<K, (V, usize)>,
    access_order: Vec<K>,
    max_size: usize,
    current_tick: usize,
}

impl<K: Eq + std::hash::Hash + Clone, V: Clone> LruCache<K, V> {
    fn new(max_size: usize) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            access_order: Vec::new(),
            max_size,
            current_tick: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        if let Some((value, tick)) = self.cache.get_mut(key) {
            self.current_tick += 1;
            *tick = self.current_tick;
            Some(value.clone())
        } else {
            None
        }
    }

    fn put(&mut self, key: K, value: V) {
        self.current_tick += 1;

        // 如果缓存已满，移除最旧的项
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&key) {
            if let Some(oldest_key) = self.find_oldest_key() {
                self.cache.remove(&oldest_key);
            }
        }

        self.cache.insert(key, (value, self.current_tick));
    }

    fn invalidate(&mut self, key: &K) {
        self.cache.remove(key);
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
        self.current_tick = 0;
    }

    fn find_oldest_key(&self) -> Option<K> {
        self.cache
            .iter()
            .min_by_key(|(_, (_, tick))| tick)
            .map(|(key, _)| key.clone())
    }
}

/// 带缓存的数据库仓库包装器
pub struct CachedRepository {
    /// 底层数据库仓库
    inner: Arc<dyn DatabaseRepository>,
    /// 会话缓存
    session_cache: Arc<RwLock<LruCache<i64, Session>>>,
    /// 会话详情缓存
    session_detail_cache: Arc<RwLock<LruCache<i64, SessionDetail>>>,
    /// 帧缓存（按会话ID）
    frames_cache: Arc<RwLock<LruCache<i64, Vec<Frame>>>>,
}

impl CachedRepository {
    /// 创建新的缓存仓库
    pub fn new(inner: Arc<dyn DatabaseRepository>) -> Self {
        Self {
            inner,
            session_cache: Arc::new(RwLock::new(LruCache::new(100))),
            session_detail_cache: Arc::new(RwLock::new(LruCache::new(50))),
            frames_cache: Arc::new(RwLock::new(LruCache::new(50))),
        }
    }

    /// 使缓存失效
    pub async fn invalidate_session(&self, session_id: i64) {
        let mut session_cache = self.session_cache.write().await;
        session_cache.invalidate(&session_id);

        let mut detail_cache = self.session_detail_cache.write().await;
        detail_cache.invalidate(&session_id);

        let mut frames_cache = self.frames_cache.write().await;
        frames_cache.invalidate(&session_id);
    }

    /// 清空所有缓存
    pub async fn clear_cache(&self) {
        let mut session_cache = self.session_cache.write().await;
        session_cache.clear();

        let mut detail_cache = self.session_detail_cache.write().await;
        detail_cache.clear();

        let mut frames_cache = self.frames_cache.write().await;
        frames_cache.clear();
    }
}

#[async_trait]
impl DatabaseRepository for CachedRepository {
    // ========== 会话操作 ==========

    async fn insert_session(&self, session: &Session) -> Result<i64> {
        let id = self.inner.insert_session(session).await?;
        // 插入后清空相关缓存（因为列表查询会失效）
        self.clear_cache().await;
        Ok(id)
    }

    async fn insert_sessions(&self, sessions: &[Session]) -> Result<Vec<i64>> {
        let ids = self.inner.insert_sessions(sessions).await?;
        self.clear_cache().await;
        Ok(ids)
    }

    async fn get_session(&self, session_id: i64) -> Result<Session> {
        // 先检查缓存
        {
            let mut cache = self.session_cache.write().await;
            if let Some(session) = cache.get(&session_id) {
                return Ok(session);
            }
        }

        // 缓存未命中，从数据库读取
        let session = self.inner.get_session(session_id).await?;

        // 放入缓存
        {
            let mut cache = self.session_cache.write().await;
            cache.put(session_id, session.clone());
        }

        Ok(session)
    }

    async fn get_session_detail(&self, session_id: i64) -> Result<SessionDetail> {
        // 先检查缓存
        {
            let mut cache = self.session_detail_cache.write().await;
            if let Some(detail) = cache.get(&session_id) {
                return Ok(detail);
            }
        }

        // 缓存未命中，从数据库读取
        let detail = self.inner.get_session_detail(session_id).await?;

        // 放入缓存
        {
            let mut cache = self.session_detail_cache.write().await;
            cache.put(session_id, detail.clone());
        }

        Ok(detail)
    }

    async fn get_sessions_by_date(&self, date: &str) -> Result<Vec<Session>> {
        // 日期查询不缓存，直接查询数据库
        self.inner.get_sessions_by_date(date).await
    }

    async fn get_all_sessions(&self) -> Result<Vec<Session>> {
        // 全量查询不缓存
        self.inner.get_all_sessions().await
    }

    async fn update_session(
        &self,
        session_id: i64,
        title: &str,
        summary: &str,
        video_path: Option<&str>,
        tags: &str,
    ) -> Result<()> {
        self.inner
            .update_session(session_id, title, summary, video_path, tags)
            .await?;
        // 更新后使缓存失效
        self.invalidate_session(session_id).await;
        Ok(())
    }

    async fn update_session_tags(&self, session_id: i64, tags: &str) -> Result<()> {
        self.inner.update_session_tags(session_id, tags).await?;
        self.invalidate_session(session_id).await;
        Ok(())
    }

    async fn update_session_video_path(&self, session_id: i64, video_path: &str) -> Result<()> {
        self.inner
            .update_session_video_path(session_id, video_path)
            .await?;
        self.invalidate_session(session_id).await;
        Ok(())
    }

    async fn update_device_info_for_all_sessions(&self) -> Result<u64> {
        let count = self.inner.update_device_info_for_all_sessions().await?;
        self.clear_cache().await;
        Ok(count)
    }

    async fn delete_session(&self, session_id: i64) -> Result<()> {
        self.inner.delete_session(session_id).await?;
        self.invalidate_session(session_id).await;
        Ok(())
    }

    async fn get_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<Vec<Session>> {
        // 不缓存，直接查询数据库（用于清理操作）
        self.inner.get_old_sessions(cutoff_date).await
    }

    async fn delete_old_sessions(&self, cutoff_date: DateTime<Utc>) -> Result<u64> {
        let count = self.inner.delete_old_sessions(cutoff_date).await?;
        self.clear_cache().await;
        Ok(count)
    }

    // ========== 帧操作 ==========

    async fn insert_frame(&self, frame: &Frame) -> Result<i64> {
        let id = self.inner.insert_frame(frame).await?;
        // 使对应会话的帧缓存失效
        let mut cache = self.frames_cache.write().await;
        cache.invalidate(&frame.session_id);
        Ok(id)
    }

    async fn insert_frames(&self, frames: &[Frame]) -> Result<()> {
        self.inner.insert_frames(frames).await?;
        // 使所有相关会话的帧缓存失效
        let mut cache = self.frames_cache.write().await;
        for frame in frames {
            cache.invalidate(&frame.session_id);
        }
        Ok(())
    }

    async fn get_frames_by_session(&self, session_id: i64) -> Result<Vec<Frame>> {
        // 先检查缓存
        {
            let mut cache = self.frames_cache.write().await;
            if let Some(frames) = cache.get(&session_id) {
                return Ok(frames);
            }
        }

        // 缓存未命中，从数据库读取
        let frames = self.inner.get_frames_by_session(session_id).await?;

        // 放入缓存
        {
            let mut cache = self.frames_cache.write().await;
            cache.put(session_id, frames.clone());
        }

        Ok(frames)
    }

    async fn delete_frames_by_session(&self, session_id: i64) -> Result<()> {
        self.inner.delete_frames_by_session(session_id).await?;
        let mut cache = self.frames_cache.write().await;
        cache.invalidate(&session_id);
        Ok(())
    }

    // ========== 其他操作（不缓存，直接透传） ==========

    async fn get_activities(&self, start_date: &str, end_date: &str) -> Result<Vec<Activity>> {
        self.inner.get_activities(start_date, end_date).await
    }

    async fn insert_llm_call(&self, record: &LLMCallRecord) -> Result<i64> {
        self.inner.insert_llm_call(record).await
    }

    async fn get_llm_calls_by_session(&self, session_id: i64) -> Result<Vec<LLMCallRecord>> {
        self.inner.get_llm_calls_by_session(session_id).await
    }

    async fn get_recent_llm_errors(&self, limit: i64) -> Result<Vec<LLMCallRecord>> {
        self.inner.get_recent_llm_errors(limit).await
    }

    async fn delete_llm_calls_by_session(&self, session_id: i64) -> Result<()> {
        self.inner.delete_llm_calls_by_session(session_id).await
    }

    async fn insert_video_segment(&self, segment: &VideoSegmentRecord) -> Result<i64> {
        self.inner.insert_video_segment(segment).await
    }

    async fn insert_video_segments(&self, segments: &[VideoSegmentRecord]) -> Result<()> {
        self.inner.insert_video_segments(segments).await
    }

    async fn get_video_segments_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<VideoSegmentRecord>> {
        self.inner.get_video_segments_by_session(session_id).await
    }

    async fn delete_video_segments_by_session(&self, session_id: i64) -> Result<()> {
        self.inner
            .delete_video_segments_by_session(session_id)
            .await
    }

    async fn insert_timeline_card(&self, card: &TimelineCardRecord) -> Result<i64> {
        self.inner.insert_timeline_card(card).await
    }

    async fn insert_timeline_cards(&self, cards: &[TimelineCardRecord]) -> Result<()> {
        self.inner.insert_timeline_cards(cards).await
    }

    async fn get_timeline_cards_by_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<TimelineCardRecord>> {
        self.inner.get_timeline_cards_by_session(session_id).await
    }

    async fn get_recent_timeline_cards(&self, limit: i64) -> Result<Vec<TimelineCardRecord>> {
        self.inner.get_recent_timeline_cards(limit).await
    }

    async fn delete_timeline_cards_by_session(&self, session_id: i64) -> Result<()> {
        self.inner
            .delete_timeline_cards_by_session(session_id)
            .await
    }

    async fn get_stats(&self) -> Result<(i64, i64, i64)> {
        self.inner.get_stats().await
    }

    async fn get_analyzed_video_paths(&self) -> Result<Vec<String>> {
        self.inner.get_analyzed_video_paths().await
    }

    async fn save_day_summary(&self, date: &str, summary: &DaySummaryRecord) -> Result<()> {
        self.inner.save_day_summary(date, summary).await
    }

    async fn get_day_summary(&self, date: &str) -> Result<Option<DaySummaryRecord>> {
        self.inner.get_day_summary(date).await
    }

    async fn delete_day_summary(&self, date: &str) -> Result<()> {
        self.inner.delete_day_summary(date).await
    }

    async fn initialize_tables(&self) -> Result<()> {
        self.inner.initialize_tables().await
    }

    fn db_type(&self) -> &str {
        self.inner.db_type()
    }

    async fn migrate_timezone_to_local(&self) -> Result<(u64, u64, u64, u64, u64, u64)> {
        // 清空所有缓存，因为时间数据已改变
        self.clear_cache().await;
        // 委托给内部实现
        self.inner.migrate_timezone_to_local().await
    }
}
