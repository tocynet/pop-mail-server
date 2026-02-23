//! POP3 response formatter

use std::fmt;

/// POP3 response
#[derive(Debug, Clone)]
pub enum Response {
    /// Single-line positive response
    Ok(String),
    /// Single-line negative response
    Err(String),
    /// Multi-line response
    MultiLine {
        status: String,
        lines: Vec<String>,
    },
}

impl Response {
    /// Create a positive response
    pub fn ok(message: &str) -> Self {
        Response::Ok(message.to_string())
    }

    /// Create a negative response
    pub fn err(message: &str) -> Self {
        Response::Err(message.to_string())
    }

    /// Create a multi-line response
    pub fn multiline(status: &str, lines: &[String]) -> Self {
        Response::MultiLine {
            status: status.to_string(),
            lines: lines.to_vec(),
        }
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Response::Ok(msg) => {
                if msg.is_empty() {
                    write!(f, "+OK\r\n")
                } else {
                    write!(f, "+OK {}\r\n", msg)
                }
            }
            Response::Err(msg) => {
                if msg.is_empty() {
                    write!(f, "-ERR\r\n")
                } else {
                    write!(f, "-ERR {}\r\n", msg)
                }
            }
            Response::MultiLine { status, lines } => {
                // First line is the status
                if status.is_empty() {
                    write!(f, "+OK\r\n")?;
                } else {
                    write!(f, "+OK {}\r\n", status)?;
                }

                // Output each line, byte-stuffing as needed
                for line in lines {
                    // If a line starts with a period, prepend another period
                    if line.starts_with('.') {
                        write!(f, ".{}\r\n", line)?;
                    } else {
                        write!(f, "{}\r\n", line)?;
                    }
                }

                // Terminating line
                write!(f, ".\r\n")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_response() {
        assert_eq!(Response::ok("").to_string(), "+OK\r\n");
        assert_eq!(Response::ok("Success").to_string(), "+OK Success\r\n");
    }

    #[test]
    fn test_err_response() {
        assert_eq!(Response::err("").to_string(), "-ERR\r\n");
        assert_eq!(Response::err("Failed").to_string(), "-ERR Failed\r\n");
    }

    #[test]
    fn test_multiline_response() {
        let lines = vec!["Line 1".to_string(), "Line 2".to_string()];
        let response = Response::multiline("List follows", &lines);
        assert_eq!(
            response.to_string(),
            "+OK List follows\r\nLine 1\r\nLine 2\r\n.\r\n"
        );
    }

    #[test]
    fn test_multiline_byte_stuffing() {
        let lines = vec![".hidden".to_string(), "normal".to_string()];
        let response = Response::multiline("", &lines);
        assert_eq!(response.to_string(), "+OK\r\n..hidden\r\nnormal\r\n.\r\n");
    }
}
