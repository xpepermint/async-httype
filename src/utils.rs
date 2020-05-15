use async_std::prelude::*;
use async_std::io::{Read, Write};
use crate::{Error};

/// Returns true if the provided bytes array holds a specific sequance of bytes.
pub fn vec_has_sequence(bytes: &[u8], needle: &[u8]) -> bool {
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

/// Parses HTTP protocol headers into `lines`. What's left in the stream represents request body.
/// Limit in number of bytes for the protocol headers can be applied.
pub async fn read_protocol_lines<I>(input: &mut I, lines: &mut Vec<String>, limit: Option<usize>) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut buffer: Vec<u8> = Vec::new();
    let mut stage = 0; // 1 = first \r, 2 = first \n, 3 = second \r, 4 = second \n
    let mut count = 0; // total

    loop {
        let mut byte = [0u8];
        let size = match input.read(&mut byte).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let byte = byte[0];
        count += 1;

        if size == 0 { // unexpected
            break;
        } else if limit.is_some() && Some(count) >= limit {
            return Err(Error::SizeLimitExceeded(limit.unwrap()));
        } else if byte == 0x0D { // char \r
            if stage == 0 || stage == 2 {
                stage += 1;
            } else {
                return Err(Error::InvalidData);
            }
        } else if byte == 0x0A { // char \n
            if stage == 1 || stage == 3 {
                let line = match String::from_utf8(buffer.to_vec()) {
                    Ok(line) => line,
                    Err(_) => return Err(Error::InvalidData),
                };
                if stage == 3 {
                    break; // end
                } else {
                    lines.push(line);
                    buffer.clear();
                    stage += 1;
                }
            } else {
                return Err(Error::InvalidData);
            }
        } else { // arbitrary char
            buffer.push(byte);
            stage = 0;
        }
    }

    Ok(count)
}

/// Streams chunk body data from input to output. Body length is unknown but
/// we can provide size limit.
/// 
/// The method searches for `0\r\n\r\n` which indicates the end of an input
/// stream. If the limit is set and the body exceeds the allowed size then the
/// forwarding will be stopped with an event.
pub async fn read_chunked_stream<I>(stream: &mut I, source: &mut Vec<u8>, limit: Option<usize>) -> Result<usize, Error>
    where
    I: Read + Unpin,
{
    let mut buffer: Vec<u8> = Vec::new();
    let mut stage = 0; // 0=characters, 1=first\r, 2=first\n, 3=second\r, 4=second\n
    let mut count = 0; // total

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
                    } else if limit.is_some() && count + length > limit.unwrap() {
                        return Err(Error::SizeLimitExceeded(limit.unwrap()));
                    } else {
                        read_sized_stream(stream, source, length).await?;
                        read_sized_stream(stream, &mut Vec::new(), 2).await?;
                        count += length;
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

    Ok(count)
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

/// Streams chunk body data from input to output. Body length is unknown but
/// we can provide size limit.
/// 
/// The method searches for `0\r\n\r\n` which indicates the end of an input
/// stream. If the limit is set and the body exceeds the allowed size then the
/// forwarding will be stopped with an event.
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
        if vec_has_sequence(&buffer, &[48, 13, 10, 13, 10]) { // last chunk
            break;
        }
        buffer = (&buffer[buffer.len()-5..]).to_vec();
    }
    Ok(count)
}

/// Streams body data of known size from input to output. An exact body length
/// (e.g. `Content-Length` header) must be provided for this transfer type.
/// 
/// The method expects that the input holds only body data. This means that we
/// have to read input protocol headers before we call this method.
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
    async fn reads_chinked_stream() {
        let stream = String::from("6\r\nHello \r\n6\r\nWorld!\r\n0\r\n\r\n");
        let mut stream = stream.as_bytes();
        let mut source = Vec::new();
        read_chunked_stream(&mut stream, &mut source, None).await.unwrap();
        assert_eq!(String::from_utf8(source).unwrap(), "Hello World!");
    }

    #[async_std::test]
    async fn checks_vector_has_sequence() {
        assert!(vec_has_sequence(&[0x0D, 0x0A, 0x0D, 0x0A], &[0x0D, 0x0A, 0x0D, 0x0A]));
        assert!(vec_has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 21, 5]));
        assert!(!vec_has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 5]));
    }
}
