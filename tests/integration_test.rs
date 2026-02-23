//! Integration tests for POP3 server

use pop_mail_server::auth::PasswordHasher;
use pop_mail_server::protocol::{Command, CommandParser, Response};
use pop_mail_server::storage::maildir::MaildirAdapter;
use pop_mail_server::storage::StorageAdapter;

use std::fs;
use tempfile::TempDir;

#[test]
fn test_password_hashing() {
    let hasher = PasswordHasher::new();
    let password = "test_password_123";

    let hash = hasher.hash(password).unwrap();
    assert!(hash.starts_with("$argon2"));

    // Verify correct password
    assert!(hasher.verify(password, &hash).unwrap());

    // Verify incorrect password
    assert!(!hasher.verify("wrong_password", &hash).unwrap());
}

#[test]
fn test_command_parsing() {
    // Test USER command
    let cmd = CommandParser::parse("USER alice").unwrap();
    assert!(matches!(cmd, Command::User(u) if u == "alice"));

    // Test PASS command
    let cmd = CommandParser::parse("PASS secret123").unwrap();
    assert!(matches!(cmd, Command::Pass(p) if p == "secret123"));

    // Test STAT command
    let cmd = CommandParser::parse("STAT").unwrap();
    assert!(matches!(cmd, Command::Stat));

    // Test LIST command
    let cmd = CommandParser::parse("LIST").unwrap();
    assert!(matches!(cmd, Command::List(None)));

    let cmd = CommandParser::parse("LIST 1").unwrap();
    assert!(matches!(cmd, Command::List(Some(1))));

    // Test RETR command
    let cmd = CommandParser::parse("RETR 1").unwrap();
    assert!(matches!(cmd, Command::Retr(1)));

    // Test DELE command
    let cmd = CommandParser::parse("DELE 2").unwrap();
    assert!(matches!(cmd, Command::Dele(2)));

    // Test QUIT command
    let cmd = CommandParser::parse("QUIT").unwrap();
    assert!(matches!(cmd, Command::Quit));

    // Test case insensitivity
    let cmd = CommandParser::parse("quit").unwrap();
    assert!(matches!(cmd, Command::Quit));

    let cmd = CommandParser::parse("Quit").unwrap();
    assert!(matches!(cmd, Command::Quit));
}

#[test]
fn test_response_formatting() {
    // Test OK response
    let resp = Response::ok("Success");
    assert_eq!(resp.to_string(), "+OK Success\r\n");

    // Test empty OK response
    let resp = Response::ok("");
    assert_eq!(resp.to_string(), "+OK\r\n");

    // Test ERR response
    let resp = Response::err("Failed");
    assert_eq!(resp.to_string(), "-ERR Failed\r\n");

    // Test multi-line response
    let lines = vec!["1 1234".to_string(), "2 5678".to_string()];
    let resp = Response::multiline("List follows", &lines);
    let expected = "+OK List follows\r\n1 1234\r\n2 5678\r\n.\r\n";
    assert_eq!(resp.to_string(), expected);

    // Test byte-stuffing
    let lines = vec![".hidden".to_string(), "normal".to_string()];
    let resp = Response::multiline("", &lines);
    let expected = "+OK\r\n..hidden\r\nnormal\r\n.\r\n";
    assert_eq!(resp.to_string(), expected);
}

#[tokio::test]
async fn test_maildir_adapter() {
    let temp_dir = TempDir::new().unwrap();
    let adapter = MaildirAdapter::new(temp_dir.path()).unwrap();

    // Initially empty
    let messages = adapter.list_messages("test").await.unwrap();
    assert!(messages.is_empty());

    // Create a test message in cur/
    let cur_dir = temp_dir.path().join("cur");
    let msg_path = cur_dir.join("1234567890.12345.hostname");
    fs::write(&msg_path, "Subject: Test\r\n\r\nHello, World!").unwrap();

    // List messages
    let messages = adapter.list_messages("test").await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "1234567890.12345.hostname");

    // Get message content
    let content = adapter
        .get_message("test", "1234567890.12345.hostname")
        .await
        .unwrap();
    assert_eq!(content, b"Subject: Test\r\n\r\nHello, World!");

    // Delete message
    adapter
        .delete_message("test", "1234567890.12345.hostname")
        .await
        .unwrap();

    // Verify deletion
    let messages = adapter.list_messages("test").await.unwrap();
    assert!(messages.is_empty());
}

#[tokio::test]
async fn test_maildir_process_new() {
    let temp_dir = TempDir::new().unwrap();
    let adapter = MaildirAdapter::new(temp_dir.path()).unwrap();

    // Create a message in new/
    let new_dir = temp_dir.path().join("new");
    let msg_path = new_dir.join("new_message.msg");
    fs::write(&msg_path, "Subject: New\r\n\r\nNew message").unwrap();

    // Process new messages
    adapter.process_new_messages("test").await.unwrap();

    // Verify message moved to cur/
    let cur_dir = temp_dir.path().join("cur");
    assert!(!msg_path.exists());
    assert!(cur_dir.join("new_message.msg").exists());
}

#[test]
fn test_invalid_commands() {
    // Empty command
    assert!(CommandParser::parse("").is_err());

    // Unknown command
    assert!(CommandParser::parse("INVALID").is_err());

    // Missing argument
    assert!(CommandParser::parse("USER").is_err());
    assert!(CommandParser::parse("RETR").is_err());

    // Invalid argument
    assert!(CommandParser::parse("RETR abc").is_err());
    assert!(CommandParser::parse("RETR 0").is_err());
}
