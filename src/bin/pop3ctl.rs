//! POP3 Server CLI Management Tool

use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Parser)]
#[command(name = "pop3ctl")]
#[command(about = "POP3 Server Management CLI", long_about = None)]
struct Cli {
    /// API endpoint URL
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    endpoint: String,

    /// API key for authentication
    #[arg(short = 'k', long, env = "POP3_API_KEY")]
    api_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Domain management
    Domain {
        #[command(subcommand)]
        command: DomainCommands,
    },
    /// User management
    User {
        #[command(subcommand)]
        command: UserCommands,
    },
    /// Generate password hash
    HashPassword,
    /// Check server health
    Health,
}

#[derive(Subcommand)]
enum DomainCommands {
    /// List all domains
    List,
    /// Add a new domain
    Add {
        /// Domain name
        name: String,
        /// Path to TLS certificate
        #[arg(long)]
        cert: Option<String>,
        /// Path to TLS private key
        #[arg(long)]
        key: Option<String>,
        /// Default storage type (maildir or s3)
        #[arg(long, default_value = "maildir")]
        storage: String,
        /// Base path for maildir storage
        #[arg(long)]
        maildir_base: Option<String>,
    },
    /// Remove a domain
    Remove {
        /// Domain name
        name: String,
    },
    /// Show domain details
    Show {
        /// Domain name
        name: String,
    },
}

#[derive(Subcommand)]
enum UserCommands {
    /// List users for a domain
    List {
        /// Domain name
        domain: String,
    },
    /// Add a new user
    Add {
        /// Username (user@domain format)
        username: String,
        /// Read password from stdin
        #[arg(long)]
        password_stdin: bool,
    },
    /// Remove a user
    Remove {
        /// Username (user@domain format)
        username: String,
    },
    /// Show user details
    Show {
        /// Username (user@domain format)
        username: String,
    },
    /// Set webhook URL for a user
    SetWebhook {
        /// Username (user@domain format)
        username: String,
        /// Webhook URL
        url: String,
    },
    /// Update user password
    SetPassword {
        /// Username (user@domain format)
        username: String,
        /// Read password from stdin
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Serialize)]
struct CreateDomainRequest {
    name: String,
    enabled: bool,
    default_storage: String,
    maildir_base: String,
    cert_path: String,
    key_path: String,
}

#[derive(Serialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    storage: String,
    enabled: bool,
}

#[derive(Serialize)]
struct UpdateUserRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    webhook_url: Option<String>,
}

#[derive(Deserialize)]
struct DomainResponse {
    name: String,
    enabled: bool,
    default_storage: String,
    maildir_base: String,
}

#[derive(Deserialize)]
struct UserResponse {
    username: String,
    domain: String,
    storage: String,
    enabled: bool,
    webhook_url: String,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

fn read_password() -> io::Result<String> {
    eprint!("Password: ");
    io::stderr().flush()?;

    let stdin = io::stdin();
    let mut password = String::new();
    stdin.lock().read_line(&mut password)?;

    Ok(password.trim().to_string())
}

fn parse_username(username: &str) -> Option<(&str, &str)> {
    let parts: Vec<&str> = username.rsplitn(2, '@').collect();
    if parts.len() == 2 {
        Some((parts[1], parts[0]))
    } else {
        None
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    let api_key = cli.api_key.unwrap_or_default();

    match cli.command {
        Commands::Health => {
            let url = format!("{}/api/v1/health", cli.endpoint);
            let response = client.get(&url).send().await?;

            if response.status().is_success() {
                let health: HealthResponse = response.json().await?;
                println!("Status: {}", health.status);
                println!("Version: {}", health.version);
            } else {
                eprintln!("Health check failed: {}", response.status());
            }
        }

        Commands::HashPassword => {
            let password = read_password()?;
            let hasher = pop_mail_server::auth::PasswordHasher::new();
            match hasher.hash(&password) {
                Ok(hash) => println!("{}", hash),
                Err(e) => eprintln!("Error: {}", e),
            }
        }

        Commands::Domain { command } => match command {
            DomainCommands::List => {
                let url = format!("{}/api/v1/domains", cli.endpoint);
                let response = client
                    .get(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let domains: Vec<DomainResponse> = response.json().await?;
                    if domains.is_empty() {
                        println!("No domains configured");
                    } else {
                        println!("{:<30} {:<10} {:<10}", "NAME", "ENABLED", "STORAGE");
                        for domain in domains {
                            println!(
                                "{:<30} {:<10} {:<10}",
                                domain.name, domain.enabled, domain.default_storage
                            );
                        }
                    }
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            DomainCommands::Add {
                name,
                cert,
                key,
                storage,
                maildir_base,
            } => {
                let url = format!("{}/api/v1/domains", cli.endpoint);
                let request = CreateDomainRequest {
                    name: name.clone(),
                    enabled: true,
                    default_storage: storage,
                    maildir_base: maildir_base.unwrap_or_else(|| format!("/var/mail/{}", name)),
                    cert_path: cert.unwrap_or_default(),
                    key_path: key.unwrap_or_default(),
                };

                let response = client
                    .post(&url)
                    .header("x-api-key", &api_key)
                    .json(&request)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("Domain {} created successfully", name);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            DomainCommands::Remove { name } => {
                let url = format!("{}/api/v1/domains/{}", cli.endpoint, name);
                let response = client
                    .delete(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("Domain {} removed successfully", name);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            DomainCommands::Show { name } => {
                let url = format!("{}/api/v1/domains/{}", cli.endpoint, name);
                let response = client
                    .get(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let domain: DomainResponse = response.json().await?;
                    println!("Name: {}", domain.name);
                    println!("Enabled: {}", domain.enabled);
                    println!("Storage: {}", domain.default_storage);
                    println!("Maildir Base: {}", domain.maildir_base);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }
        },

        Commands::User { command } => match command {
            UserCommands::List { domain } => {
                let url = format!("{}/api/v1/domains/{}/users", cli.endpoint, domain);
                let response = client
                    .get(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let users: Vec<UserResponse> = response.json().await?;
                    if users.is_empty() {
                        println!("No users for domain {}", domain);
                    } else {
                        println!("{:<20} {:<10} {:<10}", "USERNAME", "ENABLED", "STORAGE");
                        for user in users {
                            println!(
                                "{:<20} {:<10} {:<10}",
                                user.username, user.enabled, user.storage
                            );
                        }
                    }
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            UserCommands::Add {
                username,
                password_stdin,
            } => {
                let (user, domain) = match parse_username(&username) {
                    Some(parts) => parts,
                    None => {
                        eprintln!("Invalid username format. Use user@domain");
                        return Ok(());
                    }
                };

                let password = if password_stdin {
                    read_password()?
                } else {
                    eprintln!("Use --password-stdin to provide password");
                    return Ok(());
                };

                let url = format!("{}/api/v1/domains/{}/users", cli.endpoint, domain);
                let request = CreateUserRequest {
                    username: user.to_string(),
                    password,
                    storage: "maildir".to_string(),
                    enabled: true,
                };

                let response = client
                    .post(&url)
                    .header("x-api-key", &api_key)
                    .json(&request)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("User {} created successfully", username);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            UserCommands::Remove { username } => {
                let (user, domain) = match parse_username(&username) {
                    Some(parts) => parts,
                    None => {
                        eprintln!("Invalid username format. Use user@domain");
                        return Ok(());
                    }
                };

                let url = format!(
                    "{}/api/v1/domains/{}/users/{}",
                    cli.endpoint, domain, user
                );
                let response = client
                    .delete(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("User {} removed successfully", username);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            UserCommands::Show { username } => {
                let (user, domain) = match parse_username(&username) {
                    Some(parts) => parts,
                    None => {
                        eprintln!("Invalid username format. Use user@domain");
                        return Ok(());
                    }
                };

                let url = format!(
                    "{}/api/v1/domains/{}/users/{}",
                    cli.endpoint, domain, user
                );
                let response = client
                    .get(&url)
                    .header("x-api-key", &api_key)
                    .send()
                    .await?;

                if response.status().is_success() {
                    let user: UserResponse = response.json().await?;
                    println!("Username: {}", user.username);
                    println!("Domain: {}", user.domain);
                    println!("Storage: {}", user.storage);
                    println!("Enabled: {}", user.enabled);
                    println!("Webhook URL: {}", user.webhook_url);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            UserCommands::SetWebhook { username, url } => {
                let (user, domain) = match parse_username(&username) {
                    Some(parts) => parts,
                    None => {
                        eprintln!("Invalid username format. Use user@domain");
                        return Ok(());
                    }
                };

                let api_url = format!(
                    "{}/api/v1/domains/{}/users/{}",
                    cli.endpoint, domain, user
                );
                let request = UpdateUserRequest {
                    password: None,
                    webhook_url: Some(url.clone()),
                };

                let response = client
                    .put(&api_url)
                    .header("x-api-key", &api_key)
                    .json(&request)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("Webhook URL set to {} for {}", url, username);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }

            UserCommands::SetPassword {
                username,
                password_stdin,
            } => {
                let (user, domain) = match parse_username(&username) {
                    Some(parts) => parts,
                    None => {
                        eprintln!("Invalid username format. Use user@domain");
                        return Ok(());
                    }
                };

                let password = if password_stdin {
                    read_password()?
                } else {
                    eprintln!("Use --password-stdin to provide password");
                    return Ok(());
                };

                let api_url = format!(
                    "{}/api/v1/domains/{}/users/{}",
                    cli.endpoint, domain, user
                );
                let request = UpdateUserRequest {
                    password: Some(password),
                    webhook_url: None,
                };

                let response = client
                    .put(&api_url)
                    .header("x-api-key", &api_key)
                    .json(&request)
                    .send()
                    .await?;

                if response.status().is_success() {
                    println!("Password updated for {}", username);
                } else {
                    let error: ErrorResponse = response.json().await?;
                    eprintln!("Error: {}", error.error);
                }
            }
        },
    }

    Ok(())
}
