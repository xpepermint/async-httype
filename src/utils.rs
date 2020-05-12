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
    I: Write + Read + Unpin,
{
    let mut buffer: Vec<u8> = Vec::new();
    let mut count = 0;
    loop {

        let mut bytes = [0u8; 1024];
        let size = match stream.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let mut bytes = &mut bytes[0..size].to_vec();
        count += size;

        if limit.is_some() && count >= limit.unwrap() {
            return Err(Error::SizeLimitExceeded(limit.unwrap()));
        }

        source.append(&mut bytes);

        buffer.append(&mut bytes);
        buffer = (&buffer[buffer.len()-5..]).to_vec();
        if vec_has_sequence(&buffer, &[48, 13, 10, 13, 10]) { // last chunk
            break;
        }
        buffer = (&buffer[buffer.len()-5..]).to_vec();
    }
    Ok(count)
}

pub async fn read_sized_stream<I>(stream: &mut I, source: &mut Vec<u8>, length: usize) -> Result<usize, Error>
    where
    I: Read + Unpin + ?Sized,
{
    let mut count = 0;
    loop {
        let mut bytes = [0u8; 1024];
        let size = match stream.read(&mut bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotReadable),
        };
        let mut bytes = &mut bytes[0..size].to_vec();
        count += size;

        source.append(&mut bytes);

        if size == 0 || count == length {
            break;
        } else if count > length {
            return Err(Error::SizeLimitExceeded(length));
        }
    }
    Ok(count)
}

/// Streams body data from input to output.
// pub async fn forward_body<I, O>(mut input: &mut I, mut output: &mut O, lines: &Vec<String>, limit: Option<usize>) -> Result<(), Error>
//     where
//     I: Write + Read + Unpin,
//     O: Write + Read + Unpin,
// {
//     let encoding = match read_header(&lines, "Transfer-Encoding") {
//         Some(encoding) => encoding,
//         None => String::from("identity"),
//     };

//     if encoding.contains("chunked") {
//         forward_chunked_body(&mut input, &mut output, limit).await
//     } else {
//         let length = match read_header(&lines, "Content-Length") {
//             Some(encoding) => match string_to_usize(&encoding) {
//                 Some(encoding) => encoding,
//                 None => return Err(Error::InvalidHeader(String::from("Content-Length"))),
//             },
//             None => 0,
//         };
//         if length == 0 {
//             return Ok(());
//         } else if limit.is_some() && length > limit.unwrap() {
//             return Err(Error::SizeLimitExceeded(limit.unwrap()));
//         }
//         forward_sized_body(&mut input, &mut output, length).await
//     }
// }

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

        match output.write(&bytes).await {
            Ok(source) => source,
            Err(_) => return Err(Error::StreamNotWritable),
        };
        match output.flush().await {
            Ok(_) => (),
            Err(_) => return Err(Error::StreamNotWritable),
        };

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
    I: Read + Unpin + ?Sized,
    O: Write + Unpin + ?Sized,
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

        match output.write(&bytes).await {
            Ok(size) => size,
            Err(_) => return Err(Error::StreamNotWritable),
        };
        match output.flush().await {
            Ok(_) => (),
            Err(_) => return Err(Error::StreamNotWritable),
        };

        if size == 0 || count == length {
            break;
        } else if count > length {
            return Err(Error::SizeLimitExceeded(length));
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[async_std::test]
    async fn checks_vector_has_sequence() {
        assert!(vec_has_sequence(&[0x0D, 0x0A, 0x0D, 0x0A], &[0x0D, 0x0A, 0x0D, 0x0A]));
        assert!(vec_has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 21, 5]));
        assert!(!vec_has_sequence(&[1, 4, 6, 10, 21, 5, 150], &[10, 5]));
    }
}