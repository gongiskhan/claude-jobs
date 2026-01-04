//! Agent Runtime Bridge
//!
//! This crate provides the Rust-side bridge for communicating with the Python
//! Agent SDK sidecar process. It handles JSON-RPC over Unix sockets.

pub mod bridge;
pub mod events;
pub mod sidecar;
pub mod types;

pub use bridge::AgentRuntimeBridge;
pub use events::{AgentEvent, AgentEventHandler};
pub use sidecar::SidecarManager;
pub use types::{AgentCommand, AgentConfig, AgentState, AgentType, SessionContext};
