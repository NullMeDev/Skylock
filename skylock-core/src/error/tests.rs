#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_error_creation() {
        let error = Error::new(
            ErrorCategory::System,
            ErrorSeverity::High,
            "Test error".to_string(),
            "test_module".to_string(),
        );

        assert_eq!(error.category, ErrorCategory::System);
        assert_eq!(error.severity, ErrorSeverity::High);
        assert_eq!(error.message, "Test error");
        assert_eq!(error.source, Some("test_module".to_string()));
        assert_eq!(error.retry_count, 0);
        assert_eq!(error.retry_state, ErrorRetryState::NotRetried);
        assert_eq!(error.status, ErrorStatus::New);
    }

    #[test]
    fn test_error_helper_methods() {
        let error = Error::system(SystemErrorType::ProcessFailed, "Process failed".to_string());
        assert_eq!(error.category, ErrorCategory::System);
        assert!(error.message.contains("ProcessFailed"));
        
        let error = Error::storage(StorageErrorType::FileNotFound, "File missing".to_string());
        assert_eq!(error.category, ErrorCategory::Storage);
        assert!(error.message.contains("FileNotFound"));
        
        let error = Error::network(NetworkErrorType::ConnectionFailed, "No connection".to_string());
        assert_eq!(error.category, ErrorCategory::Network);
        assert!(error.message.contains("ConnectionFailed"));
    }

    #[test]
    fn test_error_retry_logic() {
        let mut error = Error::system(SystemErrorType::ProcessFailed, "Test".to_string());
        
        // Initially retryable
        assert!(error.is_retryable());
        
        // After incrementing retry count
        error.increment_retry_count();
        assert_eq!(error.retry_count, 1);
        assert_eq!(error.retry_state, ErrorRetryState::Retrying(1));
        assert!(error.is_retryable());
        
        // After marking as non-retryable
        error.mark_non_retryable();
        assert!(!error.is_retryable());
        assert_eq!(error.retry_state, ErrorRetryState::PermanentFailure);
    }

    #[test]
    fn test_error_with_details() {
        let error = Error::system(SystemErrorType::ProcessFailed, "Test".to_string())
            .with_details("Additional details".to_string());
        
        assert_eq!(error.details, Some("Additional details".to_string()));
    }

    #[test]
    fn test_error_display() {
        let error = Error::system(SystemErrorType::ProcessFailed, "Test error".to_string());
        let display_string = format!("{}", error);
        
        assert!(display_string.contains("System"));
        assert!(display_string.contains("ProcessFailed: Test error"));
    }

    #[test]
    fn test_storage_error_types() {
        // Test various storage error types
        let _connection_error = StorageErrorType::ConnectionFailed("timeout".to_string());
        let _simple_error = StorageErrorType::FileNotFound;
        let _replication_error = StorageErrorType::ReplicationError("sync failed".to_string());
        
        // Test display implementations
        let error_type = StorageErrorType::AccessDenied;
        let display_string = format!("{}", error_type);
        assert!(!display_string.is_empty());
    }

    #[test]
    fn test_error_serialization() {
        let error = Error::system(SystemErrorType::ProcessFailed, "Test".to_string());
        
        // Test JSON serialization
        let json = serde_json::to_string(&error).expect("Failed to serialize error");
        let deserialized: Error = serde_json::from_str(&json).expect("Failed to deserialize error");
        
        assert_eq!(error.category, deserialized.category);
        assert_eq!(error.message, deserialized.message);
        assert_eq!(error.severity, deserialized.severity);
    }

    #[tokio::test]
    async fn test_retry_handler() {
        let retry_handler = RetryHandler::new(3, Duration::from_millis(10));
        
        let error = Error::network(NetworkErrorType::ConnectionFailed, "Failed to connect".to_string());
        
        assert!(retry_handler.can_handle(&error).await);
        
        let result = retry_handler.handle_error(&error).await;
        assert!(result.is_err());
        
        if let Err(final_error) = result {
            assert!(!final_error.is_retryable());
            assert!(final_error.retry_count > 0);
        }
    }

    #[tokio::test]
    async fn test_error_handler_registry() {
        let mut registry = ErrorHandlerRegistry::new();
        let retry_handler = RetryHandler::new(2, Duration::from_millis(5));
        registry.register(retry_handler);
        
        let error = Error::network(NetworkErrorType::Timeout, "Network timeout".to_string());
        
        let result = registry.handle_error(&error).await;
        assert!(result.is_err());
    }
}
