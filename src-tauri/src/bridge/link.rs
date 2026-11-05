//! The bridge's local pipe: a named pipe on Windows, a Unix socket elsewhere, open only to this user.

use std::io;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub use unix::{Listener, Stream, connect, finish, name_for};
#[cfg(windows)]
pub use windows::{Listener, Stream, connect, finish, name_for};

/// From the OS generator, since tab tokens and pipe names must not be guessable.
pub fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(io::Error::other)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// A new address no other launch has used.
pub fn new_name() -> io::Result<String> {
    Ok(name_for(&random_hex(16)?))
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::thread;

    use super::*;

    #[test]
    fn a_client_line_reaches_the_listener_and_the_answer_comes_back() {
        let name = new_name().unwrap();
        let mut listener = Listener::bind(&name).unwrap();
        let server = thread::spawn(move || {
            let stream = listener.accept().unwrap().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            let mut out = &stream;
            out.write_all(line.to_uppercase().as_bytes()).unwrap();
            finish(&stream).unwrap();
        });

        let mut client = connect(&name).unwrap().unwrap();
        client.write_all(b"hello\n").unwrap();
        let mut answer = String::new();
        client.read_to_string(&mut answer).unwrap();
        server.join().unwrap();

        assert_eq!(answer, "HELLO\n");
    }

    #[test]
    fn nobody_listening_is_none() {
        assert!(connect(&new_name().unwrap()).unwrap().is_none());
    }

    #[test]
    fn names_differ_on_each_call() {
        assert_ne!(new_name().unwrap(), new_name().unwrap());
        assert_eq!(random_hex(16).unwrap().len(), 32);
    }
}
