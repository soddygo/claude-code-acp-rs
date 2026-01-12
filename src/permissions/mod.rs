//! Permission handling for can_use_tool callback
//!
//! This module implements the SDK's can_use_tool callback for checking
//! tool permissions before execution.

pub mod can_use_tool;

pub use can_use_tool::create_can_use_tool_callback;
