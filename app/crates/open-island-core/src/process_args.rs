//! KERN_PROCARGS2: argc, executable path, padding, argv, environment.
pub fn parse(bytes: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if !(1..=65536).contains(&argc) {
        return None;
    }
    let mut offset = 4 + bytes.get(4..)?.iter().position(|b| *b == 0)? + 1;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let start = offset;
    for _ in 0..argc {
        offset += bytes.get(offset..)?.iter().position(|b| *b == 0)? + 1;
    }
    Some((
        bytes.get(start..offset)?.to_vec(),
        bytes.get(offset..)?.to_vec(),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_padding_arguments_and_environment() {
        let mut input = 2i32.to_ne_bytes().to_vec();
        input.extend_from_slice(
            b"/bin/codex\0\0\0codex\0a b\0TERM_PROGRAM=Apple_Terminal\0SECRET=no\0",
        );
        let (command, env) = parse(&input).unwrap();
        assert_eq!(command, b"codex\0a b\0");
        let allowed = crate::discovery::parse_environment(&env);
        assert_eq!(
            allowed.get("TERM_PROGRAM").map(String::as_str),
            Some("Apple_Terminal")
        );
        assert!(!allowed.contains_key("SECRET"));
        assert!(parse(&input[..10]).is_none());
    }
}
