use std::collections::HashMap;
use async_std::prelude::*;
use async_std::io::{Read, Write};
use crate::{Error};

pub fn validate_size_constraint(length: usize, limit: Option<usize>) -> Result<(), Error> {
    if limit.is_some() && limit.unwrap() < length {
        Err(Error::SizeLimitExceeded(limit.unwrap()))
    } else {
        Ok(())
    }
}

pub fn has_sequence(bytes: &[u8], needle: &[u8]) -> bool {
    let mut found = 0;
    let nsize = needle.len();
    for byte in bytes.into_iter() {
        if *byte == needle[found] {
            found += 1;
        } else {
            found = 0;
        }
        if found == nsize {
            return true;
        }
    }
    false
}

pub async fn read_head<I>(input: &mut I, parts: &mut Vec<String>) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut buff = String::new();
    let mut length = 0;
    let mut stage = 0; // 0..data, 1..\r, 2..\n

    loop {
        let mut bytes = [0u8];
        let size = match input.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        length += size;

        if size == 0 {
            break;
        } else if length == 265 { // method + url + version 
            return Err(Error::InvalidData);
        } else if bytes[0] == 32 { // space
            parts.push(buff.clone());
            buff.clear();
            continue;
        } else if bytes[0] == 13 { // \r
            stage = 1;
            continue;
        } else if bytes[0] == 10 { // \n
            if stage == 1 {
                parts.push(buff.clone());
                break;
            } else {
                return Err(Error::InvalidData);
            }
        }

        buff.push(bytes[0] as char);
    }

    Ok(length)
}

pub async fn read_headers<I>(input: &mut I, output: &mut HashMap<String, String>, limit: Option<usize>) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut name = String::new();
    let mut value = String::new();
    let mut length = 0;
    let mut stage = 0; // 0..name, 1..:, 2..space, 3..value, 4..\r, 5..\n

    loop {
        let mut bytes = [0u8];
        let size = match input.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        length += size;

        if size == 0 {
            break;
        } else if limit.is_some() && limit.unwrap() < length {
            return Err(Error::SizeLimitExceeded(limit.unwrap()));
        } else if bytes[0] == 58 { // :
            if stage == 0 {
                stage = 1;
                continue;
            } else {
                return Err(Error::InvalidData);
            }
        } else if bytes[0] == 32 { // space
            if stage == 1 {
                stage = 2;
                continue;
            } else {
                return Err(Error::InvalidData);
            }
        } else if bytes[0] == 13 { // \r
            if stage == 0 || stage == 2 {
                stage = 3;
                continue;
            } else {
                return Err(Error::InvalidData);
            }
        } else if bytes[0] == 10 { // \n
            if stage == 3 {
                if name.is_empty() && value.is_empty() {
                    break; // end
                }
                output.insert(name.clone(), value.clone());
                name.clear();
                value.clear();
                stage = 0;
                continue;
            } else {
                return Err(Error::InvalidData);
            }
        }

        if stage == 0 {
            name.push(bytes[0] as char);
        } else if stage == 2 {
            value.push(bytes[0] as char);
        }
    }

    Ok(length)
}

pub async fn read_chunked_stream<I>(stream: &mut I, source: &mut Vec<u8>, limit: Option<usize>) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut buffer: Vec<u8> = Vec::new();
    let mut stage = 0; // 0=characters, 1=first\r, 2=first\n, 3=second\r, 4=second\n
    let mut total = 0; // total

    loop {
        let mut byte = [0u8];
        let size = match stream.read(&mut byte).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let byte = byte[0];

        if size == 0 { // unexpected
            break;
        } else if byte == 0x0D { // char \r
            if stage == 0 || stage == 2 {
                stage += 1;
            } else {
                return Err(Error::InvalidData);
            }
        } else if byte == 0x0A { // char \n
            if stage == 1 || stage == 3 {
                if stage == 3 {
                    break; // end
                } else {
                    let length = match String::from_utf8(buffer.to_vec()) {
                        Ok(length) => match i64::from_str_radix(&length, 16) {
                            Ok(length) => length as usize,
                            Err(_) => return Err(Error::InvalidData),
                        },
                        Err(_) => return Err(Error::InvalidData),
                    };
                    if length == 0 {
                        break;
                    } else if limit.is_some() && total + length > limit.unwrap() {
                        return Err(Error::SizeLimitExceeded(limit.unwrap()));
                    } else {
                        read_sized_stream(stream, source, length).await?;
                        read_sized_stream(stream, &mut Vec::new(), 2).await?;
                        total += length;
                    }
                    buffer.clear();
                    stage = 0;
                }
            } else {
                return Err(Error::InvalidData);
            }
        } else { // arbitrary char
            buffer.push(byte);
        }
    }

    Ok(total)
}

pub async fn read_sized_stream<I>(stream: &mut I, source: &mut Vec<u8>, length: usize) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut bytes = vec![0u8; length];
    match stream.read_exact(&mut bytes).await {
        Ok(size) => size,
        Err(_) => return Err(Error::StreamNotReadable),
    };

    source.append(&mut bytes);

    Ok(length)
}

pub async fn relay_chunked_stream<I, O>(input: &mut I, output: &mut O, limit: Option<usize>) -> Result<usize, Error>
    where
    I: Write + Read + Unpin,
    O: Write + Read + Unpin,
{
    let mut buffer: Vec<u8> = Vec::new();
    let mut count = 0;
    loop {
        if limit.is_some() && count >= limit.unwrap() {
            return Err(Error::SizeLimitExceeded(limit.unwrap()));
        }

        let mut bytes = [0u8; 1024];
        let size = match input.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let mut bytes = &mut bytes[0..size].to_vec();
        count += size;

        write_to_stream(output, &bytes).await?;
        flush_stream(output).await?;

        buffer.append(&mut bytes);
        buffer = (&buffer[buffer.len()-5..]).to_vec();
        if has_sequence(&buffer, &[48, 13, 10, 13, 10]) { // last chunk
            break;
        }
        buffer = (&buffer[buffer.len()-5..]).to_vec();
    }

    Ok(count)
}

pub async fn relay_sized_stream<I, O>(input: &mut I, output: &mut O, length: usize) -> Result<usize, Error>
    where
    I: Read + Unpin,
    O: Write + Unpin,
{
    if length == 0 {
        return Ok(0);
    }

    let mut count = 0;
    loop {
        let mut bytes = [0u8; 1024];
        let size = match input.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let bytes = &mut bytes[0..size].to_vec();
        count += size;

        write_to_stream(output, &bytes).await?;
        flush_stream(output).await?;

        if size == 0 || count == length {
            break;
        } else if count > length {
            return Err(Error::SizeLimitExceeded(length));
        }
    }

    Ok(count)
}

pub async fn write_to_stream<S>(stream: &mut S, data: &[u8]) -> Result<usize, Error>
    where
    S: Write + Unpin,
{
    match stream.write(data).await {
        Ok(size) => Ok(size),
        Err(_) => Err(Error::StreamNotWritable),
    }
}

pub async fn flush_stream<S>(stream: &mut S) -> Result<(), Error>
    where
    S: Write + Unpin,
{
    match stream.flush().await {
        Ok(_) => Ok(()),
        Err(_) => Err(Error::StreamNotWritable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn reads_request_head() {
        let mut parts = Vec::new();
        read_head(&mut String::from("OPTIONS /path HTTP/1.1\r\n").as_bytes(), &mut parts).await.unwrap();
        assert_eq!(parts, vec!["OPTIONS", "/path", "HTTP/1.1"]);
    }

    #[async_std::test]
    async fn reads_http_headers() {
        let mut output = HashMap::new();
        read_headers(&mut String::from("n1: 111\r\nn2: 222\r\n\r\n").as_bytes(), &mut output, None).await.unwrap();
        assert_eq!(output.len(), 2);
        assert_eq!(output.get("n1").unwrap(), "111");
        assert_eq!(output.get("n2").unwrap(), "222");
    }

    #[async_std::test]
    async fn reads_chunked_stream() {
        let stream = String::from("6\r\nHello \r\n6\r\nWorld!\r\n0\r\n\r\n");
        let mut stream = stream.as_bytes();
        let mut source = Vec::new();
        read_chunked_stream(&mut stream, &mut source, None).await.unwrap();
        assert_eq!(String::from_utf8(source).unwrap(), "Hello World!");
    }

    #[async_std::test]
    async fn checks_vector_has_sequence() {
        assert!(has_sequence(&[0x0D, 0x0A, 0x0D, 0x0A], &[0x0D, 0x0A, 0x0D, 0x0A]));
        assert!(has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 21, 5]));
        assert!(!has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 5]));
    }
}
