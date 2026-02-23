//! POP3 command parser

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Empty command")]
    Empty,
    #[error("Unknown command: {0}")]
    Unknown(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Missing argument")]
    MissingArgument,
}

/// POP3 commands
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    // AUTHORIZATION state commands
    User(String),
    Pass(String),
    Apop(String, String), // username, digest

    // TRANSACTION state commands
    Stat,
    List(Option<u32>),
    Retr(u32),
    Dele(u32),
    Noop,
    Rset,
    Uidl(Option<u32>),
    Top(u32, u32), // message number, lines

    // Any state commands
    Quit,
    Capa,
}

/// POP3 command parser
pub struct CommandParser;

impl CommandParser {
    /// Parse a POP3 command from a string
    pub fn parse(line: &str) -> Result<Command, ParseError> {
        let line = line.trim();
        if line.is_empty() {
            return Err(ParseError::Empty);
        }

        let mut parts = line.splitn(2, ' ');
        let cmd = parts.next().unwrap().to_uppercase();
        let args = parts.next().unwrap_or("").trim();

        match cmd.as_str() {
            "USER" => {
                if args.is_empty() {
                    Err(ParseError::MissingArgument)
                } else {
                    Ok(Command::User(args.to_string()))
                }
            }
            "PASS" => {
                if args.is_empty() {
                    Err(ParseError::MissingArgument)
                } else {
                    Ok(Command::Pass(args.to_string()))
                }
            }
            "APOP" => {
                let mut parts = args.splitn(2, ' ');
                let username = parts
                    .next()
                    .filter(|s| !s.is_empty())
                    .ok_or(ParseError::MissingArgument)?;
                let digest = parts
                    .next()
                    .filter(|s| !s.is_empty())
                    .ok_or(ParseError::MissingArgument)?;
                Ok(Command::Apop(username.to_string(), digest.to_string()))
            }
            "STAT" => Ok(Command::Stat),
            "LIST" => {
                if args.is_empty() {
                    Ok(Command::List(None))
                } else {
                    let num: u32 = args
                        .parse()
                        .map_err(|_| ParseError::InvalidArgument(args.to_string()))?;
                    if num == 0 {
                        Err(ParseError::InvalidArgument(
                            "Message number must be positive".to_string(),
                        ))
                    } else {
                        Ok(Command::List(Some(num)))
                    }
                }
            }
            "RETR" => {
                if args.is_empty() {
                    Err(ParseError::MissingArgument)
                } else {
                    let num: u32 = args
                        .parse()
                        .map_err(|_| ParseError::InvalidArgument(args.to_string()))?;
                    if num == 0 {
                        Err(ParseError::InvalidArgument(
                            "Message number must be positive".to_string(),
                        ))
                    } else {
                        Ok(Command::Retr(num))
                    }
                }
            }
            "DELE" => {
                if args.is_empty() {
                    Err(ParseError::MissingArgument)
                } else {
                    let num: u32 = args
                        .parse()
                        .map_err(|_| ParseError::InvalidArgument(args.to_string()))?;
                    if num == 0 {
                        Err(ParseError::InvalidArgument(
                            "Message number must be positive".to_string(),
                        ))
                    } else {
                        Ok(Command::Dele(num))
                    }
                }
            }
            "NOOP" => Ok(Command::Noop),
            "RSET" => Ok(Command::Rset),
            "UIDL" => {
                if args.is_empty() {
                    Ok(Command::Uidl(None))
                } else {
                    let num: u32 = args
                        .parse()
                        .map_err(|_| ParseError::InvalidArgument(args.to_string()))?;
                    if num == 0 {
                        Err(ParseError::InvalidArgument(
                            "Message number must be positive".to_string(),
                        ))
                    } else {
                        Ok(Command::Uidl(Some(num)))
                    }
                }
            }
            "TOP" => {
                let mut parts = args.split_whitespace();
                let msg_num: u32 = parts
                    .next()
                    .ok_or(ParseError::MissingArgument)?
                    .parse()
                    .map_err(|_| ParseError::InvalidArgument("Invalid message number".to_string()))?;
                let lines: u32 = parts
                    .next()
                    .ok_or(ParseError::MissingArgument)?
                    .parse()
                    .map_err(|_| ParseError::InvalidArgument("Invalid line count".to_string()))?;
                if msg_num == 0 {
                    Err(ParseError::InvalidArgument(
                        "Message number must be positive".to_string(),
                    ))
                } else {
                    Ok(Command::Top(msg_num, lines))
                }
            }
            "QUIT" => Ok(Command::Quit),
            "CAPA" => Ok(Command::Capa),
            _ => Err(ParseError::Unknown(cmd)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user() {
        assert_eq!(
            CommandParser::parse("USER alice").unwrap(),
            Command::User("alice".to_string())
        );
    }

    #[test]
    fn test_parse_pass() {
        assert_eq!(
            CommandParser::parse("PASS secret").unwrap(),
            Command::Pass("secret".to_string())
        );
    }

    #[test]
    fn test_parse_stat() {
        assert_eq!(CommandParser::parse("STAT").unwrap(), Command::Stat);
    }

    #[test]
    fn test_parse_list() {
        assert_eq!(CommandParser::parse("LIST").unwrap(), Command::List(None));
        assert_eq!(
            CommandParser::parse("LIST 1").unwrap(),
            Command::List(Some(1))
        );
    }

    #[test]
    fn test_parse_retr() {
        assert_eq!(CommandParser::parse("RETR 1").unwrap(), Command::Retr(1));
    }

    #[test]
    fn test_parse_dele() {
        assert_eq!(CommandParser::parse("DELE 1").unwrap(), Command::Dele(1));
    }

    #[test]
    fn test_parse_quit() {
        assert_eq!(CommandParser::parse("QUIT").unwrap(), Command::Quit);
    }

    #[test]
    fn test_parse_top() {
        assert_eq!(
            CommandParser::parse("TOP 1 10").unwrap(),
            Command::Top(1, 10)
        );
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(CommandParser::parse("quit").unwrap(), Command::Quit);
        assert_eq!(CommandParser::parse("Quit").unwrap(), Command::Quit);
    }

    #[test]
    fn test_parse_invalid() {
        assert!(CommandParser::parse("").is_err());
        assert!(CommandParser::parse("INVALID").is_err());
        assert!(CommandParser::parse("RETR").is_err());
        assert!(CommandParser::parse("RETR abc").is_err());
    }
}
