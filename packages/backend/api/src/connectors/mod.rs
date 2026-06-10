//! External-system connectors for Celerex flows.
//!
//! Each submodule implements the orchestrator `Handler` trait against an
//! `ExecutionType`, so a flow node can drive it:
//!   - [`memory`] — galvo-memory (Neo4j agent memory) read/write
//!   - [`galvo`]  — Galvo agent federation (control-plane invoke)
//!   - [`m365`]   — Microsoft 365 / Graph (Copilot connector + Graph read)
//!
//! They live in the API crate (not the generic orchestrator) because they
//! encode Celerex-specific integration details (base URLs, auth, payloads).

pub mod galvo;
pub mod galvo_hub;
pub mod m365;
pub mod memory;
