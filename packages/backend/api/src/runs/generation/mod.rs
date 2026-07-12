//! Server-side design-generation engine (ported from the daemon's
//! apps/daemon/src/api-generation/). Runs an agentic tool loop against the
//! engagement's in-process vaulted provider, writes files server-side, and
//! streams the daemon SSE vocabulary to the unchanged web SPA.

pub mod emitter;
pub mod file_store;
pub mod loop_core;
pub mod provider_turn;
pub mod registry;
pub mod system_prompt;
pub mod template_guard;
pub mod tools;
