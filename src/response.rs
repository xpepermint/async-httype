use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use async_std::io::{Read};
use crate::{Error, read_head, read_headers, validate_size_constraint};

#[derive(Debug)]
pub struct Response {
    status_code: usize,
    status_message: String,
    version: String,
    headers: HashMap<String, String>,
}

impl Response {

    pub fn new() -> Self {
        Self {
            status_code: 200,
            status_message: String::from("OK"),
            version: String::from("HTTP/1.1"),
            headers: HashMap::with_hasher(RandomState::new()),
        }
    }

    pub async fn read<I>(stream: &mut I, limit: Option<usize>) -> Result<Self, Error>
        where
        I: Read + Unpin,
    {
        let mut req = Self::new();
        let mut length = 0;

        let mut head = Vec::new();
        length += read_head(stream, &mut head).await?;
        validate_size_constraint(length, limit)?;
        req.set_version(match head.get(0) {
            Some(version) => version,
            None => return Err(Error::InvalidData),
        });
        req.set_status_code(match head.get(1) {
            Some(code) => match code.parse::<usize>() {
                Ok(code) => code,
                Err(_) => return Err(Error::InvalidData),
            },
            None => return Err(Error::InvalidData),
        });
        req.set_status_message(match head.get(2) {
            Some(message) => message,
            None => return Err(Error::InvalidData),
        });

        read_headers(stream, &mut req.headers, match limit {
            Some(limit) => Some(limit - length),
            None => None,
        }).await?;

        Ok(req)
    }

    pub fn status_code(&self) -> usize {
        self.status_code
    }

    pub fn status_message(&self) -> &String {
        &self.status_message
    }

    pub fn version(&self) -> &String {
        &self.version
    }

    pub fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn header<N: Into<String>>(&self, name: N) -> Option<&String> {
        self.headers.get(&name.into())
    }

    pub fn has_status_code(&self, value: usize) -> bool {
        self.status_code == value
    }

    pub fn has_version<V: Into<String>>(&self, value: V) -> bool {
        self.version == value.into()
    }

    pub fn has_headers(&self) -> bool {
        !self.headers.is_empty()
    }

    pub fn has_header<N: Into<String>>(&self, name: N) -> bool {
        self.headers.contains_key(&name.into())
    }

    pub fn set_status_code(&mut self, value: usize) {
        self.status_code = value;
    }

    pub fn set_status_message<V: Into<String>>(&mut self, value: V) {
        self.status_message = value.into();
    }

    pub fn set_version<V: Into<String>>(&mut self, value: V) {
        self.version = value.into();
    }

    pub fn set_header<N: Into<String>, V: Into<String>>(&mut self, name: N, value: V) {
        self.headers.insert(name.into(), value.into());
    }

    pub fn remove_header<N: Into<String>>(&mut self, name: N) {
        self.headers.remove(&name.into());
    }

    pub fn clear_headers(&mut self) {
        self.headers.clear();
    }

    pub fn to_string(&self) -> String {
        let mut output = String::new();
        if !self.has_version("HTTP/0.9") {
            output.push_str(&format!("{} {} {}\r\n", self.version, self.status_code, self.status_message));
            for (name, value) in self.headers.iter() {
                output.push_str(&format!("{}: {}\r\n", name, value));
            }
            output.push_str("\r\n");
        }
        output
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[async_std::test]
    async fn creates_from_stream() {
        let stream = String::from("HTTP/1.1 200 OK\r\nH: V\r\n\r\n");
        let res = Response::read(&mut stream.as_bytes(), None).await.unwrap();
        assert_eq!(res.status_code(), 200);
        assert_eq!(res.status_message(), "OK");
        assert_eq!(res.version(), "HTTP/1.1");
        assert_eq!(res.headers().len(), 1);
        assert_eq!(res.header("H").unwrap(), "V");
    }
}
