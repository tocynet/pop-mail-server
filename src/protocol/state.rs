//! POP3 session state machine

use crate::protocol::Command;

/// POP3 session states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state, awaiting authentication
    Authorization,
    /// Authenticated, can access messages
    Transaction,
    /// Session ending, committing changes
    Update,
}

/// State machine for POP3 session
pub struct StateMachine {
    state: SessionState,
}

impl StateMachine {
    /// Create a new state machine in AUTHORIZATION state
    pub fn new() -> Self {
        Self {
            state: SessionState::Authorization,
        }
    }

    /// Get the current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Transition to a new state
    pub fn transition_to(&mut self, new_state: SessionState) {
        self.state = new_state;
    }

    /// Check if a command is valid in the current state
    pub fn is_valid_command(&self, command: &Command) -> bool {
        match self.state {
            SessionState::Authorization => matches!(
                command,
                Command::User(_)
                    | Command::Pass(_)
                    | Command::Apop(_, _)
                    | Command::Quit
                    | Command::Capa
            ),
            SessionState::Transaction => matches!(
                command,
                Command::Stat
                    | Command::List(_)
                    | Command::Retr(_)
                    | Command::Dele(_)
                    | Command::Noop
                    | Command::Rset
                    | Command::Uidl(_)
                    | Command::Top(_, _)
                    | Command::Quit
                    | Command::Capa
            ),
            SessionState::Update => {
                // No commands valid in UPDATE state
                // This state is entered on QUIT and immediately exits
                false
            }
        }
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = StateMachine::new();
        assert_eq!(sm.state(), SessionState::Authorization);
    }

    #[test]
    fn test_authorization_commands() {
        let sm = StateMachine::new();
        assert!(sm.is_valid_command(&Command::User("alice".to_string())));
        assert!(sm.is_valid_command(&Command::Pass("secret".to_string())));
        assert!(sm.is_valid_command(&Command::Quit));
        assert!(!sm.is_valid_command(&Command::Stat));
        assert!(!sm.is_valid_command(&Command::Retr(1)));
    }

    #[test]
    fn test_transaction_commands() {
        let mut sm = StateMachine::new();
        sm.transition_to(SessionState::Transaction);
        assert!(sm.is_valid_command(&Command::Stat));
        assert!(sm.is_valid_command(&Command::List(None)));
        assert!(sm.is_valid_command(&Command::Retr(1)));
        assert!(sm.is_valid_command(&Command::Dele(1)));
        assert!(sm.is_valid_command(&Command::Quit));
        assert!(!sm.is_valid_command(&Command::User("alice".to_string())));
        assert!(!sm.is_valid_command(&Command::Pass("secret".to_string())));
    }

    #[test]
    fn test_state_transition() {
        let mut sm = StateMachine::new();
        assert_eq!(sm.state(), SessionState::Authorization);

        sm.transition_to(SessionState::Transaction);
        assert_eq!(sm.state(), SessionState::Transaction);

        sm.transition_to(SessionState::Update);
        assert_eq!(sm.state(), SessionState::Update);
    }
}
