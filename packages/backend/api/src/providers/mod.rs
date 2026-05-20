//! Per-engagement encrypted LLM provider API keys.
//!
//! Phase 1: storage + retrieval only. Wiring overrides into the LlmHandler's
//! actual provider call is a follow-up.
//!
//! Plaintext keys never leave this module's `set`/`get` boundary; the rest of
//! the system sees only `ProviderKeyMeta` (provider id + masked preview +
//! timestamps).

pub mod routes;
pub mod store;

pub use store::{PgProviderKeysStore, ProviderKeyMeta, ProviderKeysStore, SUPPORTED_PROVIDERS};
