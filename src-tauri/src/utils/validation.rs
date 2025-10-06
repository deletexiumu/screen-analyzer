//! 输入验证工具函数
//!
//! 提供各种输入参数的验证功能，防止SQL注入和无效输入

/// 验证会话ID是否有效（防止SQL注入和无效输入）
///
/// # 参数
/// - `id`: 会话ID
///
/// # 返回
/// - `Ok(())`: 验证通过
/// - `Err(String)`: 错误信息
pub fn validate_session_id(id: i64) -> Result<(), String> {
    if id < 0 {
        return Err(format!("无效的会话 ID: {}", id));
    }
    Ok(())
}
