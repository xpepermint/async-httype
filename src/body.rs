use std::fmt;
use std::collections::HashMap;
use async_std::io::{Read, Write};
use crate::{Error, read_chunked_stream, read_sized_stream, write_to_stream, flush_stream};

pub struct Body {
    bytes: Vec<u8>,
    length: usize,
    length_limit: Option<usize>,
}

impl Body {

    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            length: 0,
            length_limit: None,
        }
    }

    pub fn bytes(&self) -> &Vec<u8> {
        &self.bytes
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn length_limit(&self) -> Option<usize> {
        self.length_limit
    }

    pub fn has_length_limit(&self) -> bool {
        self.length_limit.is_some()
    }

    pub fn set_length_limit(&mut self, limit: usize) {
        self.length_limit = Some(limit);
    }

    pub fn remove_length_limit(&mut self) {
        self.length_limit = None;
    }

    pub async fn read<I>(&mut self, stream: &mut I, res: &HashMap<String, String>) -> Result<usize, Error>
        where
        I: Read + Unpin,
    {
        let length = res.get("Content-Length");
        let encoding = res.get("Transfer-Encoding");

        if encoding.is_some() && encoding.unwrap().contains(&String::from("chunked")) {
            self.read_chunked(stream).await
        } else {
            let length = match length {
                Some(length) => match length.parse::<usize>() {
                    Ok(length) => length,
                    Err(_) => return Err(Error::InvalidHeader(String::from("Content-Length"))),
                },
                None => return Err(Error::InvalidHeader(String::from("Content-Length"))),
            };
            self.read_sized(stream, length).await
        }
    }

    pub async fn read_chunked<I>(&mut self, stream: &mut I) -> Result<usize, Error>
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
        
        let length = read_chunked_stream(stream, &mut self.bytes, limit).await?;
        self.length += length;

        Ok(length)
    }
    
    pub async fn read_sized<I>(&mut self, stream: &mut I, length: usize) -> Result<usize, Error>
        where
        I: Read + Unpin,
    {
        match self.length_limit {
            Some(limit) => match length + self.length > limit {
                true => return Err(Error::SizeLimitExceeded(limit)),
                false => (),
            },
            None => (),
        };

        let length = read_sized_stream(stream, &mut self.bytes, length).await?;
        self.length += length;

        Ok(length)
    }
    
    pub async fn write<I>(&mut self, stream: &mut I) -> Result<usize, Error>
        where
        I: Write + Unpin,
    {
        let size = write_to_stream(stream, &self.bytes()).await?;
        flush_stream(stream).await?;
        Ok(size)
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
        self.length = 0;
        self.length_limit = None;
    }
}

impl fmt::Display for Body {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{:?}", self.bytes())
    }
}
