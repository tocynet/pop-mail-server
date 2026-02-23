//! Timeout management

use std::time::Duration;

/// Timeout manager for session timeouts
#[derive(Debug, Clone)]
pub struct TimeoutManager {
    /// Timeout for individual commands
    pub command_timeout: Duration,
    /// Timeout for idle connections
    pub idle_timeout: Duration,
}

impl TimeoutManager {
    /// Create a new timeout manager
    pub fn new(command_timeout_secs: u64, idle_timeout_secs: u64) -> Self {
        Self {
            command_timeout: Duration::from_secs(command_timeout_secs),
            idle_timeout: Duration::from_secs(idle_timeout_secs),
        }
    }
}

impl Default for TimeoutManager {
    fn default() -> Self {
        Self::new(30, 300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_manager() {
        let tm = TimeoutManager::new(30, 300);
        assert_eq!(tm.command_timeout, Duration::from_secs(30));
        assert_eq!(tm.idle_timeout, Duration::from_secs(300));
    }

    #[test]
    fn test_default() {
        let tm = TimeoutManager::default();
        assert_eq!(tm.command_timeout, Duration::from_secs(30));
        assert_eq!(tm.idle_timeout, Duration::from_secs(300));
    }
}
