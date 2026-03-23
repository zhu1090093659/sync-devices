/// Embedded Worker JS bundle, built from `worker/src/` via `npm run build`.
/// Run `cd worker && npm run build` before `cargo build` to regenerate.
pub const WORKER_JS: &str = include_str!("../worker/dist/worker.js");
