use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use async_std::io::{Read};
use crate::{Error};
use crate::utils::{read_protocol_lines};

#[derive(Debug)]
pub struct Response {
    status_code: Option<usize>,
    status_message: Option<String>,
    version: Option<String>,
    headers: HashMap<String, String>,
    length: usize,
    length_limit: Option<usize>,
    lines: Vec<String>,
}

impl Response {

    pub fn new() -> Self {
        Self {
            status_code: None,
            status_message: None,
            version: None,
            headers: HashMap::with_hasher(RandomState::new()),
            length: 0,
            length_limit: None,
            lines: Vec::new(),
        }
    }

    pub fn status_code(&self) -> &Option<usize> {
        &self.status_code
    }

    pub fn status_message(&self) -> &Option<String> {
        &self.status_message
    }

    pub fn version(&self) -> &Option<String> {
        &self.version
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn header<N: Into<String>>(&self, name: N) -> Option<&String> {
        self.headers.get(&name.into())
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn length_limit(&self) -> Option<usize> {
        self.length_limit
    }

    pub fn lines(&self) -> &Vec<String> {
        &self.lines
    }

    pub fn has_status_code(&self) -> bool {
        self.status_code.is_some()
    }

    pub fn has_status_message(&self) -> bool {
        self.status_message.is_some()
    }

    pub fn has_version(&self) -> bool {
        self.version.is_some()
    }

    pub fn has_headers(&self) -> bool {
        !self.headers.is_empty()
    }

    pub fn has_header<N: Into<String>>(&self, name: N) -> bool {
        self.headers.contains_key(&name.into())
    }

    pub fn has_length_limit(&self) -> bool {
        self.length_limit.is_some()
    }

    pub fn set_status_code(&mut self, value: usize) {
        self.status_code = Some(value);
    }

    pub fn set_status_message<V: Into<String>>(&mut self, value: V) {
        self.status_message = Some(value.into());
    }

    pub fn set_version<V: Into<String>>(&mut self, value: V) {
        self.version = Some(value.into());
    }

    pub fn set_header<N: Into<String>, V: Into<String>>(&mut self, name: N, value: V) {
        self.headers.insert(name.into(), value.into());
    }

    pub fn set_length_limit(&mut self, limit: usize) {
        self.length_limit = Some(limit);
    }

    pub fn remove_status_code(&mut self) {
        self.status_code = None;
    }

    pub fn remove_status_message(&mut self) {
        self.status_message = None;
    }

    pub fn remove_version<V: Into<String>>(&mut self) {
        self.version = None;
    }

    pub fn remove_header<N: Into<String>>(&mut self, name: N) {
        self.headers.remove(&name.into());
    }

    pub fn remove_length_limit(&mut self) {
        self.length_limit = None;
    }

    pub async fn read_stream<I>(&mut self, stream: &mut I) -> Result<usize, Error>
        where
        I: Read + Unpin,
    {
        let limit = match self.length_limit {
            Some(limit) => match limit == 0 {
                true => return Err(Error::SizeLimitExceeded(limit)),
                false => Some(limit - self.length),
            },
            None => None,
        };

        let length = read_protocol_lines(stream, &mut self.lines, limit).await?;
        self.length += length;

        Ok(length)
    }

    pub async fn read_string<V: Into<String>>(&mut self, value: V) -> Result<(), Error> {
        let value = value.into();
        if !value.contains("\r\n") {
            return Err(Error::InvalidData);
        }

        let length = value.chars().count();
        match self.length_limit {
            Some(limit) => match length + self.length > limit {
                true => return Err(Error::SizeLimitExceeded(limit)),
                false => (),
            },
            None => (),
        };
        self.length += length;

        let mut lines: Vec<String> = value.split("\r\n").map(|s| s.to_string()).collect();
        self.lines.append(&mut lines);

        Ok(())
    }

    pub fn clear(&mut self) {
        self.status_code = None;
        self.version = None;
        self.headers.clear();
        self.length = 0;
        self.length_limit = None;
        self.lines.clear();
    }

    pub fn parse_head(&mut self) -> Result<(), Error> {
        let mut parts = match self.lines.first() {
            Some(head) => head.splitn(3, " "),
            None => return Err(Error::InvalidData),
        };

        self.version = match parts.next() {
            Some(version) => match version {
                "HTTP/1.0" => Some(String::from("1.0")),
                "HTTP/1.1" => Some(String::from("1.1")),
                _ => return Err(Error::InvalidData),
            },
            None => return Err(Error::InvalidData),
        };
        self.status_code = match parts.next() {
            Some(status_code) => match status_code.parse::<usize>() {
                Ok(status_code) => Some(status_code),
                Err(_) => return Err(Error::InvalidData),
            },
            None => return Err(Error::InvalidData),
        };

        Ok(())
    }

    pub fn parse_headers(&mut self) -> Result<(), Error> {
        for line in self.lines.iter().skip(1) {
            if line == "" {
                break;
            }
            let mut parts = line.splitn(2, ": ");
            let name = match parts.next() {
                Some(name) => String::from(name),
                None => return Err(Error::InvalidData),
            };
            let value = match parts.next() {
                Some(value) => String::from(value),
                None => return Err(Error::InvalidData),
            };
            self.headers.insert(name, value);
        }

        Ok(())
    }

    pub fn build_head(&mut self) -> Result<(), Error> {
        let version = match &self.version {
            Some(version) => format!("HTTP/{}", version),
            None => return Err(Error::InvalidData),
        };
        let status_code = match &self.status_code {
            Some(code) => code,
            None => return Err(Error::InvalidData),
        };
        let status_message = match &self.status_message {
            Some(message) => message,
            None => return Err(Error::InvalidData),
        };

        let head = format!("{} {} {}", version, status_code, status_message);
        if self.lines.is_empty() {
            self.lines.push(head);
        } else {
            self.lines[0] = head;
        }

        Ok(())
    }

    pub fn build_headers(&mut self) -> Result<(), Error> {
        let head = match self.lines.first() {
            Some(head) => Some(head.clone()),
            None => None,
        };
        self.lines.clear();
        if head.is_some() {
            self.lines.push(head.unwrap());
        }

        for (name, value) in &self.headers {
            self.lines.push(format!("{}: {}", name, value));
        }

        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_string().as_bytes().to_vec()
    }

    pub fn to_string(&self) -> String {
        self.lines.join("\r\n") + "\r\n\r\n"
    }
}

impl fmt::Display for Response {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.to_string())
    }
}

impl From<Response> for String {
    fn from(item: Response) -> String {
        item.to_string()
    }
}
