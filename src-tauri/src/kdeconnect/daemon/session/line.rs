use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Reads one UTF-8, newline-delimited packet without allowing the line buffer
/// to grow past `max_bytes`. This mirrors `read_line` but rejects oversized
/// input before allocating it.
///
/// `scratch` must be reused between calls. Bytes already consumed from the
/// reader are kept there, so cancelling the future (for example, from a
/// `tokio::select!` branch) does not lose a partially read packet.
pub(super) async fn read_line_limited<R>(
    reader: &mut R,
    scratch: &mut Vec<u8>,
    line: &mut String,
    max_bytes: usize,
) -> io::Result<usize>
where
    R: AsyncBufRead + Unpin,
{
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if scratch.is_empty() {
                line.clear();
                return Ok(0);
            }
            let read = scratch.len();
            let text = std::str::from_utf8(scratch).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "packet line is not valid UTF-8")
            })?;
            line.clear();
            line.push_str(text);
            scratch.clear();
            return Ok(read);
        }

        let newline = available.iter().position(|byte| *byte == b'\n');
        let chunk_len = newline.map(|index| index + 1).unwrap_or(available.len());

        if scratch.len() + chunk_len > max_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("packet line exceeds {} bytes", max_bytes),
            ));
        }

        scratch.extend_from_slice(&available[..chunk_len]);
        reader.consume(chunk_len);

        if newline.is_some() {
            let read = scratch.len();
            let text = std::str::from_utf8(scratch).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "packet line is not valid UTF-8")
            })?;
            line.clear();
            line.push_str(text);
            scratch.clear();
            return Ok(read);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn reads_normal_line() {
        let mut reader = BufReader::new(&b"hello\n"[..]);
        let mut scratch = Vec::new();
        let mut line = String::new();

        let read = read_line_limited(&mut reader, &mut scratch, &mut line, 1024)
            .await
            .expect("line should fit");

        assert_eq!(read, 6);
        assert_eq!(line, "hello\n");
    }

    #[tokio::test]
    async fn reads_eof_line_without_newline() {
        let mut reader = BufReader::new(&b"hello"[..]);
        let mut scratch = Vec::new();
        let mut line = String::new();

        let read = read_line_limited(&mut reader, &mut scratch, &mut line, 1024)
            .await
            .expect("line should fit");

        assert_eq!(read, 5);
        assert_eq!(line, "hello");
    }

    #[tokio::test]
    async fn rejects_oversized_line() {
        let mut reader = BufReader::new(&b"hello\n"[..]);
        let mut scratch = Vec::new();
        let mut line = String::new();

        let error = read_line_limited(&mut reader, &mut scratch, &mut line, 4)
            .await
            .expect_err("oversized line must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn rejects_invalid_utf8() {
        let mut reader = BufReader::new(&[0xff, b'\n'][..]);
        let mut scratch = Vec::new();
        let mut line = String::new();

        let error = read_line_limited(&mut reader, &mut scratch, &mut line, 1024)
            .await
            .expect_err("invalid UTF-8 must be rejected");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn resumes_after_cancelled_partial_read() {
        let (mut writer, reader) = tokio::io::duplex(64);
        let mut reader = BufReader::new(reader);
        let mut scratch = Vec::new();
        let mut line = String::new();

        writer.write_all(b"hello ").await.expect("write first half");
        let cancelled = tokio::time::timeout(
            Duration::from_millis(50),
            read_line_limited(&mut reader, &mut scratch, &mut line, 1024),
        )
        .await;
        assert!(
            cancelled.is_err(),
            "read should still be waiting for newline"
        );
        assert!(line.is_empty());

        writer
            .write_all(b"world\n")
            .await
            .expect("write second half");
        let read = read_line_limited(&mut reader, &mut scratch, &mut line, 1024)
            .await
            .expect("line should complete");

        assert_eq!(read, 12);
        assert_eq!(line, "hello world\n");
    }
}
