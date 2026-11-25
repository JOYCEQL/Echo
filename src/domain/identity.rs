use std::time::{SystemTime, UNIX_EPOCH};
pub fn uid() -> String {
    format!(
        "{:x}-{:x}",
        now_nanos(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
pub fn now() -> i64 {
    (now_nanos() / 1_000_000) as i64
}
