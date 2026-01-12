//! can_use_tool callback implementation for SDK
//!
//! This module implements the permission checking callback that the SDK calls
//! before executing tools.

use claude_code_agent_sdk::types::permissions::{
    CanUseToolCallback, PermissionResult, PermissionResultAllow, PermissionResultDeny,
    ToolPermissionContext,
};
use std::sync::{Arc, OnceLock};
use tracing::{debug, error, info, warn};

use crate::session::{PermissionOutcome, PermissionRequestBuilder, Session, ToolPermissionResult};

/// Create a can_use_tool callback that receives Session via OnceLock
///
/// The callback tries to get the Session from OnceLock when called.
/// This allows creating the callback before the Session exists.
pub fn create_can_use_tool_callback(
    session_lock: Arc<OnceLock<Arc<Session>>>,
) -> CanUseToolCallback {
    Arc::new(
        move |tool_name: String, tool_input: serde_json::Value, _context: ToolPermissionContext| {
            let session_lock = Arc::clone(&session_lock);

            Box::pin(async move {
                debug!(
                    tool_name = %tool_name,
                    "can_use_tool callback called"
                );

                // Try to get session from OnceLock
                let Some(session) = session_lock.get() else {
                    warn!(
                        tool_name = %tool_name,
                        "Session not ready in callback - denying"
                    );
                    return PermissionResult::Deny(PermissionResultDeny {
                        message: "Session not initialized yet".to_string(),
                        interrupt: false,
                    });
                };

                // Get permission handler and check permission
                let handler_guard = session.permission().await;
                let result = handler_guard
                    .check_permission(&tool_name, &tool_input)
                    .await;

                match result {
                    ToolPermissionResult::Allowed => {
                        PermissionResult::Allow(PermissionResultAllow::default())
                    }
                    ToolPermissionResult::Blocked { reason } => {
                        PermissionResult::Deny(PermissionResultDeny {
                            message: reason,
                            interrupt: false,
                        })
                    }
                    ToolPermissionResult::NeedsPermission => {
                        // Send permission request
                        handle_needs_permission(session, &tool_name, &tool_input).await
                    }
                }
            })
        },
    )
}

/// Handle permission requests by using PermissionRequestBuilder
async fn handle_needs_permission(
    session: &Arc<Session>,
    tool_name: &str,
    tool_input: &serde_json::Value,
) -> PermissionResult {
    // Get connection_cx
    let Some(connection_cx) = session.get_connection_cx() else {
        warn!(
            tool_name = %tool_name,
            "Connection not available for permission request"
        );
        return PermissionResult::Deny(PermissionResultDeny {
            message: "Connection not available. Please try again after the prompt starts."
                .to_string(),
            interrupt: false,
        });
    };

    // Generate tool_call_id (SDK context doesn't provide it)
    let tool_call_id = uuid::Uuid::new_v4().to_string();

    // Build and send permission request
    let outcome = match PermissionRequestBuilder::new(
        &session.session_id,
        tool_call_id,
        tool_name,
        tool_input.clone(),
    )
    .request(connection_cx)
    .await
    {
        Ok(o) => o,
        Err(e) => {
            error!(
                tool_name = %tool_name,
                error = %e,
                "Permission request failed"
            );
            return PermissionResult::Deny(PermissionResultDeny {
                message: format!("Permission request failed: {}", e),
                interrupt: false,
            });
        }
    };

    // Handle user response
    match outcome {
        PermissionOutcome::AllowOnce => {
            info!(tool_name = %tool_name, "Permission allowed once");
            PermissionResult::Allow(PermissionResultAllow::default())
        }
        PermissionOutcome::AllowAlways => {
            info!(tool_name = %tool_name, "Permission allowed always");
            // Add to permission rules
            session.add_permission_allow_rule(tool_name).await;
            PermissionResult::Allow(PermissionResultAllow::default())
        }
        PermissionOutcome::Rejected => {
            info!(tool_name = %tool_name, "Permission rejected by user");
            PermissionResult::Deny(PermissionResultDeny {
                message: "Permission denied by user".to_string(),
                interrupt: false,
            })
        }
        PermissionOutcome::Cancelled => {
            info!(tool_name = %tool_name, "Permission request cancelled");
            PermissionResult::Deny(PermissionResultDeny {
                message: "Permission request was cancelled".to_string(),
                interrupt: false,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Note: The callback now requires Arc<OnceLock<Arc<Session>>>
    // which requires a full Session setup to test.
    // The basic test below verifies the callback compiles correctly.
    // Functional tests require integration testing with a real session.

    #[test]
    fn test_callback_function_compiles() {
        // This test verifies the callback function signature is correct
        let session_lock: Arc<OnceLock<Arc<Session>>> = Arc::new(OnceLock::new());
        let _callback = create_can_use_tool_callback(session_lock);
        // If this compiles, the signature is correct
    }
}
