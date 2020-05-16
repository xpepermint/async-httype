use std::fmt;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use async_std::io::{Read};
use crate::{Error, read_head, validate_size_constraint, read_headers};

#[derive(Debug)]
pub struct Request {
    method: String,
    uri: String,
    version: String,
    headers: HashMap<String, String>,
}

impl Request {

    pub fn new() -> Self {
        Self {
            method: String::from("GET"),
            uri: String::from("/"),
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
        req.set_method(match head.get(0) {
            Some(method) => method,
            None => return Err(Error::InvalidData),
        });
        req.set_uri(match head.get(1) {
            Some(uri) => uri,
            None => return Err(Error::InvalidData),
        });
        req.set_version(match head.get(2) {
            Some(version) => version,
            None => "HTTP/0.9",
        });

        if !req.has_version("HTTP/0.9") {
            read_headers(stream, &mut req.headers, match limit {
                Some(limit) => Some(limit - length),
                None => None,
            }).await?;
        }

        Ok(req)
    }

    pub fn method(&self) -> &String {
        &self.method
    }

    pub fn uri(&self) -> &String {
        &self.uri
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

    pub fn has_method<S: Into<String>>(&self, value: S) -> bool {
        self.method == value.into()
    }

    pub fn has_version<S: Into<String>>(&self, value: S) -> bool {
        self.version == value.into()
    }

    pub fn has_headers(&self) -> bool {
        !self.headers.is_empty()
    }

    pub fn has_header<N: Into<String>>(&self, name: N) -> bool {
        self.headers.contains_key(&name.into())
    }

    pub fn set_method<V: Into<String>>(&mut self, value: V) {
        self.method = value.into();
    }

    pub fn set_uri<V: Into<String>>(&mut self, value: V) {
        self.uri = value.into();
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
        if self.has_version("HTTP/0.9") {
            output.push_str(&format!("GET {}\r\n", self.uri));
        } else {
            output.push_str(&format!("{} {} {}\r\n", self.method, self.uri, self.version));
            for (name, value) in self.headers.iter() {
                output.push_str(&format!("{}: {}\r\n", name, value));
            }
            output.push_str("\r\n");
        }
        output
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

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn creates_from_stream() {
        let stream = String::from("GET / HTTP/1.1\r\nH: V\r\n\r\n");
        let req = Request::read(&mut stream.as_bytes(), None).await.unwrap();
        assert_eq!(req.method(), "GET");
        assert_eq!(req.uri(), "/");
        assert_eq!(req.version(), "HTTP/1.1");
        assert_eq!(req.headers().len(), 1);
        assert_eq!(req.header("H").unwrap(), "V");
    }
}
