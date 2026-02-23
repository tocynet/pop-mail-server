//! POP3 session handler

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::server::TlsStream;
use tracing::{debug, info, warn};

use crate::auth::{AuthStore, User};
use crate::config::Config;
use crate::protocol::{Command, CommandParser, Response, SessionState, StateMachine};
use crate::storage::adapter::StorageFactory;
use crate::storage::StorageAdapter;
use crate::vhost::VirtualHostRouter;
use crate::webhook::{WebhookDispatcher, WebhookEvent};

/// A POP3 session for a single client connection
pub struct Session {
    stream: BufReader<TlsStream<TcpStream>>,
    peer_addr: SocketAddr,
    config: Arc<Config>,
    auth_store: Arc<AuthStore>,
    #[allow(dead_code)]
    vhost_router: Arc<VirtualHostRouter>,
    storage_factory: Arc<StorageFactory>,
    webhook_dispatcher: Arc<WebhookDispatcher>,
    state_machine: StateMachine,
    current_user: Option<User>,
    pending_username: Option<String>,
    storage: Option<Box<dyn StorageAdapter>>,
    messages_to_delete: Vec<String>,
    auth_attempts: u32,
}

impl Session {
    /// Create a new session
    pub fn new(
        stream: TlsStream<TcpStream>,
        peer_addr: SocketAddr,
        config: Arc<Config>,
        auth_store: Arc<AuthStore>,
        vhost_router: Arc<VirtualHostRouter>,
        storage_factory: Arc<StorageFactory>,
        webhook_dispatcher: Arc<WebhookDispatcher>,
    ) -> Self {
        Self {
            stream: BufReader::new(stream),
            peer_addr,
            config,
            auth_store,
            vhost_router,
            storage_factory,
            webhook_dispatcher,
            state_machine: StateMachine::new(),
            current_user: None,
            pending_username: None,
            storage: None,
            messages_to_delete: Vec::new(),
            auth_attempts: 0,
        }
    }

    /// Run the session until completion
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Send greeting
        self.send_response(&Response::ok("POP3 server ready")).await?;

        let idle_timeout = Duration::from_secs(self.config.security.idle_timeout_secs);
        let command_timeout = Duration::from_secs(self.config.security.command_timeout_secs);

        loop {
            // Read command with timeout
            let line = match timeout(idle_timeout, self.read_line()).await {
                Ok(Ok(Some(line))) => line,
                Ok(Ok(None)) => {
                    // Client disconnected
                    break;
                }
                Ok(Err(e)) => {
                    warn!("Read error: {}", e);
                    break;
                }
                Err(_) => {
                    // Idle timeout
                    self.send_response(&Response::err("Idle timeout")).await?;
                    break;
                }
            };

            // Check command length
            if line.len() > self.config.security.max_command_length {
                self.send_response(&Response::err("Command too long")).await?;
                continue;
            }

            // Parse command
            let command = match CommandParser::parse(&line) {
                Ok(cmd) => cmd,
                Err(e) => {
                    self.send_response(&Response::err(&format!("Invalid command: {}", e)))
                        .await?;
                    continue;
                }
            };

            debug!("Received command: {:?}", command);

            // Process command with timeout
            let should_quit = match timeout(command_timeout, self.process_command(command)).await {
                Ok(Ok(quit)) => quit,
                Ok(Err(e)) => {
                    self.send_response(&Response::err(&format!("Error: {}", e)))
                        .await?;
                    false
                }
                Err(_) => {
                    self.send_response(&Response::err("Command timeout")).await?;
                    false
                }
            };

            if should_quit {
                break;
            }
        }

        // Commit deletions if in UPDATE state
        if self.state_machine.state() == SessionState::Update {
            self.commit_deletions().await?;
        }

        Ok(())
    }

    /// Read a line from the client
    async fn read_line(&mut self) -> anyhow::Result<Option<String>> {
        let mut line = String::new();
        let bytes_read = self.stream.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Ok(None);
        }
        Ok(Some(line.trim_end().to_string()))
    }

    /// Send a response to the client
    async fn send_response(&mut self, response: &Response) -> anyhow::Result<()> {
        let data = response.to_string();
        self.stream.get_mut().write_all(data.as_bytes()).await?;
        self.stream.get_mut().flush().await?;
        Ok(())
    }

    /// Process a POP3 command
    async fn process_command(&mut self, command: Command) -> anyhow::Result<bool> {
        // Check if command is valid for current state
        if !self.state_machine.is_valid_command(&command) {
            self.send_response(&Response::err("Command not valid in current state"))
                .await?;
            return Ok(false);
        }

        match command {
            Command::User(username) => self.handle_user(username).await,
            Command::Pass(password) => self.handle_pass(password).await,
            Command::Apop(_, _) => {
                // APOP not supported
                self.send_response(&Response::err("APOP not supported"))
                    .await?;
                Ok(false)
            }
            Command::Stat => self.handle_stat().await,
            Command::List(msg_num) => self.handle_list(msg_num).await,
            Command::Retr(msg_num) => self.handle_retr(msg_num).await,
            Command::Dele(msg_num) => self.handle_dele(msg_num).await,
            Command::Noop => self.handle_noop().await,
            Command::Rset => self.handle_rset().await,
            Command::Uidl(msg_num) => self.handle_uidl(msg_num).await,
            Command::Top(msg_num, lines) => self.handle_top(msg_num, lines).await,
            Command::Quit => self.handle_quit().await,
            Command::Capa => self.handle_capa().await,
        }
    }

    async fn handle_user(&mut self, username: String) -> anyhow::Result<bool> {
        self.pending_username = Some(username);
        self.send_response(&Response::ok("User accepted")).await?;
        Ok(false)
    }

    async fn handle_pass(&mut self, password: String) -> anyhow::Result<bool> {
        let username = match &self.pending_username {
            Some(u) => u.clone(),
            None => {
                self.send_response(&Response::err("No username provided"))
                    .await?;
                return Ok(false);
            }
        };

        // Check auth attempts
        self.auth_attempts += 1;
        if self.auth_attempts > self.config.security.max_auth_attempts {
            self.send_response(&Response::err("Too many authentication attempts"))
                .await?;
            return Ok(true); // Force quit
        }

        // Authenticate user
        match self.auth_store.authenticate(&username, &password).await {
            Ok(user) => {
                // Initialize storage for user
                let storage = self.storage_factory.create_for_user(&user).await?;
                storage.process_new_messages(&user.username).await?;

                self.current_user = Some(user.clone());
                self.storage = Some(storage);
                self.state_machine.transition_to(SessionState::Transaction);

                // Send webhook
                self.webhook_dispatcher
                    .dispatch(WebhookEvent::SessionStart {
                        username: username.clone(),
                        peer_addr: self.peer_addr,
                    })
                    .await;

                info!("User {} authenticated from {}", username, self.peer_addr);
                self.send_response(&Response::ok("Authentication successful"))
                    .await?;
            }
            Err(e) => {
                warn!(
                    "Authentication failed for {} from {}: {}",
                    username, self.peer_addr, e
                );
                self.send_response(&Response::err("Authentication failed"))
                    .await?;
            }
        }

        self.pending_username = None;
        Ok(false)
    }

    async fn handle_stat(&mut self) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;
        let count = messages.len();
        let total_size: usize = messages.iter().map(|m| m.size).sum();

        self.send_response(&Response::ok(&format!("{} {}", count, total_size)))
            .await?;

        self.dispatch_command_webhook("stat").await;
        Ok(false)
    }

    async fn handle_list(&mut self, msg_num: Option<u32>) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;

        match msg_num {
            Some(num) => {
                let idx = num as usize - 1;
                if idx < messages.len() && !self.messages_to_delete.contains(&messages[idx].id) {
                    self.send_response(&Response::ok(&format!("{} {}", num, messages[idx].size)))
                        .await?;
                } else {
                    self.send_response(&Response::err("No such message"))
                        .await?;
                }
            }
            None => {
                let mut lines = Vec::new();
                for (i, msg) in messages.iter().enumerate() {
                    if !self.messages_to_delete.contains(&msg.id) {
                        lines.push(format!("{} {}", i + 1, msg.size));
                    }
                }
                self.send_response(&Response::multiline("Message list follows", &lines))
                    .await?;
            }
        }

        self.dispatch_command_webhook("list").await;
        Ok(false)
    }

    async fn handle_retr(&mut self, msg_num: u32) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;
        let idx = msg_num as usize - 1;

        if idx >= messages.len() || self.messages_to_delete.contains(&messages[idx].id) {
            self.send_response(&Response::err("No such message")).await?;
            return Ok(false);
        }

        let msg = &messages[idx];
        let content = storage.get_message(&user.username, &msg.id).await?;
        let content_str = String::from_utf8_lossy(&content);
        let lines: Vec<String> = content_str.lines().map(|s| s.to_string()).collect();

        self.send_response(&Response::multiline(&format!("{} octets", msg.size), &lines))
            .await?;

        self.dispatch_command_webhook("retr").await;
        Ok(false)
    }

    async fn handle_dele(&mut self, msg_num: u32) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;
        let idx = msg_num as usize - 1;

        if idx >= messages.len() {
            self.send_response(&Response::err("No such message")).await?;
            return Ok(false);
        }

        let msg_id = messages[idx].id.clone();
        if self.messages_to_delete.contains(&msg_id) {
            self.send_response(&Response::err("Message already marked for deletion"))
                .await?;
            return Ok(false);
        }

        self.messages_to_delete.push(msg_id);
        self.send_response(&Response::ok("Message marked for deletion"))
            .await?;

        self.dispatch_command_webhook("dele").await;
        Ok(false)
    }

    async fn handle_noop(&mut self) -> anyhow::Result<bool> {
        self.send_response(&Response::ok("")).await?;
        self.dispatch_command_webhook("noop").await;
        Ok(false)
    }

    async fn handle_rset(&mut self) -> anyhow::Result<bool> {
        self.messages_to_delete.clear();
        self.send_response(&Response::ok("Deletions reset")).await?;
        self.dispatch_command_webhook("rset").await;
        Ok(false)
    }

    async fn handle_uidl(&mut self, msg_num: Option<u32>) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;

        match msg_num {
            Some(num) => {
                let idx = num as usize - 1;
                if idx < messages.len() && !self.messages_to_delete.contains(&messages[idx].id) {
                    self.send_response(&Response::ok(&format!("{} {}", num, messages[idx].uidl)))
                        .await?;
                } else {
                    self.send_response(&Response::err("No such message"))
                        .await?;
                }
            }
            None => {
                let mut lines = Vec::new();
                for (i, msg) in messages.iter().enumerate() {
                    if !self.messages_to_delete.contains(&msg.id) {
                        lines.push(format!("{} {}", i + 1, msg.uidl));
                    }
                }
                self.send_response(&Response::multiline("Unique-ID listing follows", &lines))
                    .await?;
            }
        }

        self.dispatch_command_webhook("uidl").await;
        Ok(false)
    }

    async fn handle_top(&mut self, msg_num: u32, lines: u32) -> anyhow::Result<bool> {
        let storage = self.storage.as_ref().unwrap();
        let user = self.current_user.as_ref().unwrap();

        let messages = storage.list_messages(&user.username).await?;
        let idx = msg_num as usize - 1;

        if idx >= messages.len() || self.messages_to_delete.contains(&messages[idx].id) {
            self.send_response(&Response::err("No such message")).await?;
            return Ok(false);
        }

        let msg = &messages[idx];
        let content = storage.get_message(&user.username, &msg.id).await?;
        let content_str = String::from_utf8_lossy(&content);

        // Split into headers and body
        let mut result_lines = Vec::new();
        let mut in_body = false;
        let mut body_lines = 0;

        for line in content_str.lines() {
            if in_body {
                if body_lines >= lines {
                    break;
                }
                result_lines.push(line.to_string());
                body_lines += 1;
            } else {
                result_lines.push(line.to_string());
                if line.is_empty() {
                    in_body = true;
                }
            }
        }

        self.send_response(&Response::multiline("Top of message follows", &result_lines))
            .await?;

        self.dispatch_command_webhook("top").await;
        Ok(false)
    }

    async fn handle_quit(&mut self) -> anyhow::Result<bool> {
        // Transition to UPDATE state to commit deletions
        self.state_machine.transition_to(SessionState::Update);

        if let Some(user) = &self.current_user {
            self.webhook_dispatcher
                .dispatch(WebhookEvent::SessionEnd {
                    username: user.username.clone(),
                    peer_addr: self.peer_addr,
                })
                .await;
        }

        self.send_response(&Response::ok("Bye")).await?;
        Ok(true)
    }

    async fn handle_capa(&mut self) -> anyhow::Result<bool> {
        let capabilities = vec![
            "USER".to_string(),
            "UIDL".to_string(),
            "TOP".to_string(),
            "IMPLEMENTATION pop-mail-server".to_string(),
        ];
        self.send_response(&Response::multiline("Capability list follows", &capabilities))
            .await?;
        Ok(false)
    }

    async fn commit_deletions(&mut self) -> anyhow::Result<()> {
        if let (Some(storage), Some(user)) = (&self.storage, &self.current_user) {
            for msg_id in &self.messages_to_delete {
                if let Err(e) = storage.delete_message(&user.username, msg_id).await {
                    warn!("Failed to delete message {}: {}", msg_id, e);
                }
            }
        }
        self.messages_to_delete.clear();
        Ok(())
    }

    async fn dispatch_command_webhook(&self, command: &str) {
        if let Some(user) = &self.current_user {
            self.webhook_dispatcher
                .dispatch(WebhookEvent::Command {
                    username: user.username.clone(),
                    command: command.to_string(),
                    peer_addr: self.peer_addr,
                })
                .await;
        }
    }
}
