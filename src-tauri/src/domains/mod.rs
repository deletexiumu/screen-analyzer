// 领域模块 - 用于组织应用的业务逻辑
//
// 将原本混乱的 AppState 按业务领域分组,实现单一职责原则
// 包含5个领域:捕获、分析、存储、系统、总结

pub mod analysis;
pub mod capture;
pub mod storage;
pub mod summary;
pub mod system;

pub use analysis::AnalysisDomain;
pub use capture::CaptureDomain;
pub use storage::StorageDomain;
pub use summary::{DaySummary, DeviceStat, ParallelWork, SummaryGenerator, UsagePattern};
pub use system::SystemDomain;
