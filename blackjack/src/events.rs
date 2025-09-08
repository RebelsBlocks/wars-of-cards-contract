use near_sdk::{env, serde::Serialize};

/// Emit event for logging - generic function for any serializable event
pub fn emit_event<T: Serialize>(event: T) {
    env::log_str(&format!("EVENT_JSON:{}", serde_json::to_string(&event).unwrap()));
}

/// Log a simple message event (for quick debugging/tracking)
pub fn log_event(event_type: &str, message: &str) {
    let simple_event = SimpleEvent {
        event_type: event_type.to_string(),
        message: message.to_string(),
        timestamp: env::block_timestamp(),
    };
    
    emit_event(simple_event);
}

/// Log an error event with context
pub fn log_error(error: &str, context: &str, account_id: Option<near_sdk::AccountId>) {
    let error_event = ErrorEvent {
        error: error.to_string(),
        context: context.to_string(),
        account_id,
        timestamp: env::block_timestamp(),
    };
    
    emit_event(error_event);
}

/// Simple event structure for basic logging
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct SimpleEvent {
    event_type: String,
    message: String,
    timestamp: u64,
}

/// Error event structure for tracking failures
#[derive(Serialize)]
#[serde(crate = "near_sdk::serde")]
struct ErrorEvent {
    error: String,
    context: String,
    account_id: Option<near_sdk::AccountId>,
    timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_event() {
        // This test just verifies the function doesn't panic
        log_event("test", "test message");
    }

    #[test]
    fn test_error_event() {
        // This test just verifies the function doesn't panic
        log_error("test error", "test context", None);
    }
}