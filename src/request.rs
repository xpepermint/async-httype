use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use async_std::io::{Read};
use crate::{Error};
use crate::utils::{read_protocol_lines};

#[derive(Debug)]
pub struct Request {
    method: Option<String>,
    uri: Option<String>,
    version: Option<String>,
    headers: HashMap<String, String>,
    length: usize,
    length_limit: Option<usize>,
    lines: Vec<String>,
}

impl Request {

    pub fn new() -> Self {
        Self {
            method: None,
            uri: None,
            version: None,
            headers: HashMap::with_hasher(RandomState::new()),
            length: 0,
            length_limit: None,
            lines: Vec::new(),
        }
    }

    pub fn method(&self) -> &Option<String> {
        &self.method
    }

    pub fn uri(&self) -> &Option<String> {
        &self.uri
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

    pub fn has_method(&self) -> bool {
        self.method.is_some()
    }

    pub fn has_uri(&self) -> bool {
        self.uri.is_some()
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

    pub fn set_method<V: Into<String>>(&mut self, value: V) {
        self.method = Some(value.into());
    }

    pub fn set_uri<V: Into<String>>(&mut self, value: V) {
        self.uri = Some(value.into());
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

    pub fn remove_method(&mut self) {
        self.method = None;
    }

    pub fn remove_uri(&mut self) {
        self.uri = None;
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
        self.method = None;
        self.uri = None;
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

        self.method = match parts.next() {
            Some(method) => Some(String::from(method)),
            None => return Err(Error::InvalidData),
        };
        self.uri = match parts.next() {
            Some(uri) => Some(String::from(uri)),
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

        Ok(())
    }

    pub fn parse_headers(&mut self) -> Result<(), Error> {
        for line in self.lines.iter().skip(1) {
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
        let method = match &self.method {
            Some(method) => method,
            None => return Err(Error::InvalidData),
        };
        let uri = match &self.uri {
            Some(uri) => uri,
            None => return Err(Error::InvalidData),
        };
        let version = match &self.version {
            Some(version) => format!("HTTP/{}", version),
            None => return Err(Error::InvalidData),
        };

        self.lines[0] = format!("{} {} {}", method, uri, version);

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

impl fmt::Display for Request {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.to_string())
    }
}

impl From<Request> for String {
    fn from(item: Request) -> String {
        item.to_string()
    }
}
