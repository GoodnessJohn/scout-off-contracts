/// Hard ceiling on the ring-buffer depth to bound storage costs.
/// No admin call may set `HistoryMaxDepth` above this value.
pub const MAX_HISTORY_DEPTH: u32 = 50;

/// Default depth used when no admin configuration exists.
/// Matches the previous compile-time constant so existing deployments
/// see no behaviour change until `set_history_max_depth` is first called.
pub const DEFAULT_HISTORY_MAX_DEPTH: u32 = 10;
