//! Protocol-specific tests

use pop_mail_server::protocol::{Command, SessionState, StateMachine};

#[test]
fn test_state_machine_initial() {
    let sm = StateMachine::new();
    assert_eq!(sm.state(), SessionState::Authorization);
}

#[test]
fn test_state_machine_authorization_commands() {
    let sm = StateMachine::new();

    // Valid in AUTHORIZATION state
    assert!(sm.is_valid_command(&Command::User("alice".to_string())));
    assert!(sm.is_valid_command(&Command::Pass("secret".to_string())));
    assert!(sm.is_valid_command(&Command::Quit));
    assert!(sm.is_valid_command(&Command::Capa));

    // Invalid in AUTHORIZATION state
    assert!(!sm.is_valid_command(&Command::Stat));
    assert!(!sm.is_valid_command(&Command::List(None)));
    assert!(!sm.is_valid_command(&Command::Retr(1)));
    assert!(!sm.is_valid_command(&Command::Dele(1)));
    assert!(!sm.is_valid_command(&Command::Noop));
    assert!(!sm.is_valid_command(&Command::Rset));
    assert!(!sm.is_valid_command(&Command::Uidl(None)));
    assert!(!sm.is_valid_command(&Command::Top(1, 10)));
}

#[test]
fn test_state_machine_transaction_commands() {
    let mut sm = StateMachine::new();
    sm.transition_to(SessionState::Transaction);

    // Valid in TRANSACTION state
    assert!(sm.is_valid_command(&Command::Stat));
    assert!(sm.is_valid_command(&Command::List(None)));
    assert!(sm.is_valid_command(&Command::List(Some(1))));
    assert!(sm.is_valid_command(&Command::Retr(1)));
    assert!(sm.is_valid_command(&Command::Dele(1)));
    assert!(sm.is_valid_command(&Command::Noop));
    assert!(sm.is_valid_command(&Command::Rset));
    assert!(sm.is_valid_command(&Command::Uidl(None)));
    assert!(sm.is_valid_command(&Command::Uidl(Some(1))));
    assert!(sm.is_valid_command(&Command::Top(1, 10)));
    assert!(sm.is_valid_command(&Command::Quit));
    assert!(sm.is_valid_command(&Command::Capa));

    // Invalid in TRANSACTION state
    assert!(!sm.is_valid_command(&Command::User("alice".to_string())));
    assert!(!sm.is_valid_command(&Command::Pass("secret".to_string())));
}

#[test]
fn test_state_machine_update_commands() {
    let mut sm = StateMachine::new();
    sm.transition_to(SessionState::Update);

    // No commands valid in UPDATE state
    assert!(!sm.is_valid_command(&Command::User("alice".to_string())));
    assert!(!sm.is_valid_command(&Command::Pass("secret".to_string())));
    assert!(!sm.is_valid_command(&Command::Stat));
    assert!(!sm.is_valid_command(&Command::Quit));
}

#[test]
fn test_state_transitions() {
    let mut sm = StateMachine::new();

    // Start in AUTHORIZATION
    assert_eq!(sm.state(), SessionState::Authorization);

    // Transition to TRANSACTION
    sm.transition_to(SessionState::Transaction);
    assert_eq!(sm.state(), SessionState::Transaction);

    // Transition to UPDATE
    sm.transition_to(SessionState::Update);
    assert_eq!(sm.state(), SessionState::Update);
}

#[test]
fn test_command_equality() {
    assert_eq!(
        Command::User("alice".to_string()),
        Command::User("alice".to_string())
    );
    assert_ne!(
        Command::User("alice".to_string()),
        Command::User("bob".to_string())
    );
    assert_eq!(Command::Stat, Command::Stat);
    assert_eq!(Command::List(Some(1)), Command::List(Some(1)));
    assert_ne!(Command::List(Some(1)), Command::List(Some(2)));
    assert_ne!(Command::List(Some(1)), Command::List(None));
}
