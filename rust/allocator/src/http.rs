//! Just enough HTTP/1.1 for the allocator: read one request or response
//! (Content-Length or chunked bodies), and write one. Every connection
//! carries one exchange and closes.

use std::io::{self, BufRead, BufReader, Read, Write};

/// Larger bodies are refused.
pub const MAX_BODY: usize = 1 << 20;

#[derive(Debug, PartialEq)]
pub struct Message {
    /// "POST /allocate" for a request, "200 OK" for a response.
    pub start: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Message {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// A request's method and path.
    pub fn method_path(&self) -> (&str, &str) {
        let mut parts = self.start.split(' ');
        (parts.next().unwrap_or(""), parts.next().unwrap_or(""))
    }

    /// A response's status code.
    pub fn status(&self) -> u16 {
        self.start
            .split(' ')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why.to_owned())
}

/// Reads one message: the start line (without the HTTP version), headers,
/// and a body by Content-Length, chunked encoding, or (for a response with
/// neither) to the end of the stream.
pub fn read_message(stream: impl Read, response: bool) -> io::Result<Message> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let line = line.trim_end();
    if line.is_empty() {
        return Err(invalid("empty message"));
    }
    let start = if response {
        // "HTTP/1.1 200 OK" -> "200 OK"
        line.split_once(' ').map_or("", |(_, rest)| rest).to_owned()
    } else {
        // "POST /allocate HTTP/1.1" -> "POST /allocate"
        line.rsplit_once(' ')
            .map_or(line, |(head, _)| head)
            .to_owned()
    };
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').ok_or_else(|| invalid("bad header"))?;
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
        if headers.len() > 100 {
            return Err(invalid("too many headers"));
        }
    }
    let mut message = Message {
        start,
        headers,
        body: Vec::new(),
    };
    if let Some(length) = message.header("Content-Length") {
        let length: usize = length.parse().map_err(|_| invalid("bad length"))?;
        if length > MAX_BODY {
            return Err(invalid("body too large"));
        }
        message.body.resize(length, 0);
        reader.read_exact(&mut message.body)?;
    } else if message
        .header("Transfer-Encoding")
        .is_some_and(|v| v.eq_ignore_ascii_case("chunked"))
    {
        message.body = read_chunked(&mut reader)?;
    } else if response {
        reader
            .take(MAX_BODY as u64 + 1)
            .read_to_end(&mut message.body)?;
        if message.body.len() > MAX_BODY {
            return Err(invalid("body too large"));
        }
    }
    Ok(message)
}

fn read_chunked(reader: &mut impl BufRead) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let size = line.trim().split(';').next().unwrap_or("");
        let size = usize::from_str_radix(size, 16).map_err(|_| invalid("bad chunk size"))?;
        if size == 0 {
            // Trailers, then the blank line.
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                    return Ok(body);
                }
            }
        }
        if body.len() + size > MAX_BODY {
            return Err(invalid("body too large"));
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf)?;
    }
}

/// Writes a request (`start` is "GET /path") or a response (`start` is
/// "200 OK") with a JSON body, and asks to close the connection.
pub fn write_message(mut stream: impl Write, start: &str, body: &[u8]) -> io::Result<()> {
    let line = if start.starts_with(|c: char| c.is_ascii_digit()) {
        format!("HTTP/1.1 {start}")
    } else {
        format!("{start} HTTP/1.1\r\nHost: localhost")
    };
    write!(
        stream,
        "{line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_request_with_a_body() {
        let raw = b"POST /allocate HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer k\r\nContent-Length: 4\r\n\r\n{}{}";
        let message = read_message(&raw[..], false).unwrap();
        assert_eq!(message.method_path(), ("POST", "/allocate"));
        assert_eq!(message.header("authorization"), Some("Bearer k"));
        assert_eq!(message.body, b"{}{}");
    }

    #[test]
    fn reads_a_chunked_response() {
        let raw = b"HTTP/1.1 201 Created\r\nTransfer-Encoding: chunked\r\n\r\n4\r\n{\"Id\r\n6\r\n\":\"a\"}\r\n0\r\n\r\n";
        let message = read_message(&raw[..], true).unwrap();
        assert_eq!(message.status(), 201);
        assert_eq!(message.body, br#"{"Id":"a"}"#);
    }

    #[test]
    fn refuses_huge_bodies() {
        let raw = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert!(read_message(raw.as_bytes(), false).is_err());
    }

    #[test]
    fn writes_what_it_reads() {
        let mut out = Vec::new();
        write_message(&mut out, "200 OK", br#"{"ok":true}"#).unwrap();
        let message = read_message(&out[..], true).unwrap();
        assert_eq!(message.status(), 200);
        assert_eq!(message.body, br#"{"ok":true}"#);
        let mut out = Vec::new();
        write_message(&mut out, "GET /containers/json", b"").unwrap();
        let message = read_message(&out[..], false).unwrap();
        assert_eq!(message.method_path(), ("GET", "/containers/json"));
    }
}
