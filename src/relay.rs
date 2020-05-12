use async_std::io::{Read, Write};
use crate::{Error};
use crate::utils::{relay_chunked_stream, relay_sized_stream};

#[derive(Debug)]
pub struct Relay {
    length: usize,
    length_limit: Option<usize>,
}

impl Relay {

    pub fn new() -> Self {
        Self {
            length: 0,
            length_limit: None,
        }
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

    pub async fn relay_chunked_stream<I, O>(&mut self, input: &mut I, output: &mut O) -> Result<usize, Error>
        where
        I: Write + Read + Unpin,
        O: Write + Read + Unpin,
    {
        let limit = match self.length_limit {
            Some(limit) => match limit == 0 {
                true => return Err(Error::SizeLimitExceeded(limit)),
                false => Some(limit - self.length),
            },
            None => None,
        };
        
        let length = relay_chunked_stream(input, output, limit).await?;
        self.length += length;

        Ok(length)
    }
    
    pub async fn relay_sized_stream<I, O>(&mut self, input: &mut I, output: &mut O, length: usize) -> Result<usize, Error>
        where
        I: Read + Unpin + ?Sized,
        O: Write + Unpin + ?Sized,
    {
        match self.length_limit {
            Some(limit) => match length + self.length > limit {
                true => return Err(Error::SizeLimitExceeded(limit)),
                false => (),
            },
            None => (),
        };

        let length = relay_sized_stream(input, output, length).await?;
        self.length += length;

        Ok(length)
    }
    
    pub fn clear(&mut self) {
        self.length = 0;
        self.length_limit = None;
    }
}
