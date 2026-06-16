//! Automations: connector-event-triggered project_members. A poll loop (sibling
//! to the scheduler) detects new connector items and fires the member's flow.
//! See docs/superpowers/specs/2026-06-15-automations-triggered-design.md
pub mod source;
pub mod http_api;
pub mod store;
pub mod poll;
