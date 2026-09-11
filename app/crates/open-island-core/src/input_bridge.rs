//! Session-addressed input, independent of the terminal emulator.
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub const ENV: &str = "OPEN_ISLAND_INPUT_SOCKET";
pub const MAX_TEXT: usize = 64 * 1024;
pub const MAX_FRAME: usize = MAX_TEXT * 6 + 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub pid: u32,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub error: Option<String>,
}

pub fn read_frame<T: serde::de::DeserializeOwned>(reader: impl Read) -> io::Result<T> {
    let mut bytes = Vec::new();
    BufReader::new(reader.take((MAX_FRAME + 1) as u64)).read_until(b'\n', &mut bytes)?;
    if bytes.len() > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Mensagem incompleta ou muito grande.",
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Mensagem muito grande.",
        ));
    }
    writer.write_all(&bytes)
}

pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > MAX_TEXT {
        return Err("A mensagem deve conter de 1 a 65536 bytes.".into());
    }
    // Never allow pasted text to terminate bracketed paste or act as a terminal key.
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("A mensagem contém caracteres de controle não permitidos.".into());
    }
    Ok(())
}

pub fn send(path: &Path, pid: u32, text: &str) -> Result<(), String> {
    validate_text(text)?;
    let result = (|| -> io::Result<Response> {
        let mut socket = UnixStream::connect(path)?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        write_frame(
            &mut socket,
            &Request {
                pid,
                text: text.into(),
            },
        )?;
        read_frame(socket)
    })();
    match result {
        Ok(Response { error: None }) => Ok(()),
        Ok(Response { error: Some(error) }) => Err(error),
        // A lost acknowledgement is ambiguous: never retry automatically.
        Err(error) => Err(format!("Não foi possível confirmar o envio à sessão: {error}. Confira o terminal antes de reenviar.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_multiline_roundtrip_and_control_rejection() {
        let mut bytes = Vec::new();
        write_frame(
            &mut bytes,
            &Request {
                pid: 42,
                text: "olá\n\t'$(echo x)'".into(),
            },
        )
        .unwrap();
        let request: Request = read_frame(bytes.as_slice()).unwrap();
        assert_eq!(request.pid, 42);
        assert_eq!(request.text, "olá\n\t'$(echo x)'");
        assert!(validate_text(&request.text).is_ok());
        for text in ["\x1b[201~danger", "a\0b", "\x03", "\u{85}", " "] {
            assert!(validate_text(text).is_err());
        }
    }
    #[test]
    fn malformed_and_oversized_frames_are_bounded() {
        assert!(read_frame::<Request>(&b"{}"[..]).is_err());
        assert!(read_frame::<Request>(&b"{}\n"[..]).is_err());
        assert!(read_frame::<Request>(vec![b'x'; MAX_FRAME + 100].as_slice()).is_err());
        assert!(validate_text(&"x".repeat(MAX_TEXT + 1)).is_err());
    }
}
