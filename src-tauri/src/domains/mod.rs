// 领域模块 - 用于组织应用的业务逻辑
//
// 将原本混乱的 AppState 按业务领域分组,实现单一职责原则
// 包含4个领域:捕获、分析、存储、系统

pub mod capture;
pub mod analysis;
pub mod storage;
pub mod system;

pub use capture::CaptureDomain;
pub use analysis::AnalysisDomain;
pub use storage::StorageDomain;
pub use system::SystemDomain;
