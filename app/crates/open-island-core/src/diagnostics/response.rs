use serde_json::Value;
use std::{
    io,
    time::{Duration, Instant},
};

const LIMIT: usize = 256 * 1024;

pub fn read(
    mut receive: impl FnMut(&mut [u8], Duration) -> io::Result<usize>,
    deadline: Instant,
) -> Result<Value, ()> {
    let mut frame = Vec::new();
    let mut total = 0;
    loop {
        let remaining = deadline.checked_duration_since(Instant::now()).ok_or(())?;
        let mut buffer = [0; 8192];
        let count = match receive(&mut buffer, remaining) {
            Ok(0) => return Err(()),
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(()),
        };
        for byte in &buffer[..count] {
            total += 1;
            if total > LIMIT {
                return Err(());
            }
            if *byte != b'\n' {
                frame.push(*byte);
                continue;
            }
            let value: Value = serde_json::from_slice(&frame).map_err(|_| ())?;
            frame.clear();
            if value.get("id").and_then(Value::as_u64) != Some(1) {
                continue;
            }
            if value.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(());
            }
            return value.get("data").cloned().ok_or(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read, Write};
    #[test]
    fn large_fragmented_response_uses_blocks_and_preserves_utf8() {
        let data = serde_json::json!({"text":"ação".repeat(20000)});
        let wire = format!(
            "{{\"event\":\"ignored\"}}\n{}\n",
            serde_json::json!({"id":1,"ok":true,"data":data})
        );
        let mut cursor = Cursor::new(wire.as_bytes());
        let mut reads = 0;
        let result = read(
            |buffer, _| {
                reads += 1;
                cursor.read(buffer)
            },
            Instant::now() + Duration::from_secs(2),
        );
        assert_eq!(result.unwrap(), data);
        assert!(reads <= wire.len() / 8192 + 1);
    }
    #[test]
    fn limit_eof_and_expired_deadline_are_enforced() {
        for payload in [
            vec![b'x'; LIMIT + 1],
            b"{\"id\":1".to_vec(),
            b"{\"id\":1,\"ok\":false}\n".to_vec(),
        ] {
            let mut cursor = Cursor::new(payload);
            assert!(read(
                |buffer, _| cursor.read(buffer),
                Instant::now() + Duration::from_secs(2)
            )
            .is_err());
        }
        assert!(read(
            |_, _| panic!("expired deadline must not read"),
            Instant::now() - Duration::from_millis(1)
        )
        .is_err());
    }
    #[test]
    fn incoming_bytes_do_not_extend_the_socket_deadline() {
        use std::os::unix::net::UnixStream;
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer
            .set_write_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let producer = std::thread::spawn(move || {
            for _ in 0..100 {
                if writer.write_all(b" ").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        let started = Instant::now();
        let result = read(
            |buffer, remaining| {
                reader.set_read_timeout(Some(remaining))?;
                reader.read(buffer)
            },
            started + Duration::from_millis(70),
        );
        let elapsed = started.elapsed();
        drop(reader);
        producer.join().unwrap();
        assert!(result.is_err());
        assert!(elapsed < Duration::from_secs(1));
    }
}
