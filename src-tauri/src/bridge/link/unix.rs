use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub type Stream = UnixStream;

/// In the user's own temp folder, short enough for `sun_path`.
pub fn name_for(id: &str) -> String {
    // 16 hex digits keep the path well inside the 104 bytes macOS allows.
    let short = &id[..id.len().min(16)];
    std::env::temp_dir()
        .join(format!("cerberus-bridge-{short}.sock"))
        .to_string_lossy()
        .into_owned()
}

pub fn connect(name: &str) -> io::Result<Option<UnixStream>> {
    match UnixStream::connect(name) {
        Ok(stream) => Ok(Some(stream)),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

/// A socket write is in the peer's buffer once it returns, so closing loses nothing.
pub fn finish(_stream: &UnixStream) -> io::Result<()> {
    Ok(())
}

/// The other end of a socket, watched through a second handle to it.
pub struct Peer(UnixStream);

impl Peer {
    /// A socket reads and writes independently, so a read held open here never delays a write.
    pub fn of(stream: &UnixStream) -> io::Result<Self> {
        stream.try_clone().map(Self)
    }

    /// Blocks until the other end has closed, as it does when its process exits.
    pub fn gone(self) {
        let mut buf = [0u8; 256];
        loop {
            match (&self.0).read(&mut buf) {
                Ok(0) => return,
                Err(e) if e.kind() != io::ErrorKind::Interrupted => return,
                _ => {}
            }
        }
    }
}

/// Removes its socket file when dropped.
pub struct Listener {
    listener: UnixListener,
    path: PathBuf,
}

impl Listener {
    pub fn bind(name: &str) -> io::Result<Self> {
        let path = PathBuf::from(name);
        let listener = UnixListener::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(Self { listener, path })
    }

    /// The outer error stops listening; the inner one is a client that left early.
    pub fn accept(&mut self) -> io::Result<io::Result<UnixStream>> {
        let accepted = self.listener.accept().map(|(stream, _)| stream);
        if accepted.is_err() {
            // Out of file handles, most likely; a pause keeps the loop from spinning.
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(accepted)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn the_socket_is_the_users_alone() {
        let name = super::super::new_name().unwrap();
        let _listener = Listener::bind(&name).unwrap();

        let mode = fs::metadata(&name).unwrap().permissions().mode();

        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
    }
}
