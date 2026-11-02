use std::io;
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread::JoinHandle;

pub type Stream = UnixStream;

/// A socket in the app's data folder, which is the user's own, so each user reaches their own app.
pub fn name(identifier: &str) -> String {
    let dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    dir.join(identifier)
        .join("open.sock")
        .to_string_lossy()
        .into_owned()
}

pub fn connect(name: &str) -> io::Result<Option<UnixStream>> {
    match UnixStream::connect(name) {
        Ok(stream) => Ok(Some(stream)),
        // A socket file left by an app that crashed refuses, which also means nobody is listening.
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

pub struct Listener(UnixListener);

impl Listener {
    pub fn bind(name: &str) -> io::Result<Self> {
        let path = Path::new(name);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match UnixListener::bind(path) {
            Ok(listener) => Ok(Self(listener)),
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => Self::reclaim(path, e),
            Err(e) => Err(e),
        }
    }

    /// Takes over a socket file only when no app answers on it.
    fn reclaim(path: &Path, in_use: io::Error) -> io::Result<Self> {
        if connect(&path.to_string_lossy())?.is_some() {
            return Err(in_use);
        }
        std::fs::remove_file(path)?;
        UnixListener::bind(path).map(Self)
    }

    /// The outer error stops listening; the inner one is an asker that left early.
    pub fn accept(&mut self) -> io::Result<io::Result<UnixStream>> {
        let accepted = self.0.accept().map(|(stream, _)| stream);
        if accepted.is_err() {
            // Out of file handles, most likely; a pause keeps the loop from spinning.
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(accepted)
    }
}

/// Shutting the socket down wakes a worker blocked on it, which then fails its call and returns.
pub fn cutter(stream: &UnixStream) -> io::Result<impl Fn(&JoinHandle<()>) + Send + 'static> {
    let stream = stream.try_clone()?;
    Ok(move |_: &JoinHandle<()>| {
        // Fails only once the asker is gone, and then there is nothing left to wake.
        let _ = stream.shutdown(Shutdown::Both);
    })
}

/// A socket keeps a written reply for the asker after it closes, so there is nothing to wait for.
pub fn finish(_stream: &UnixStream) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
pub fn unique_name(n: u32) -> String {
    let file = format!("cerberus-test-{}-{n}.sock", std::process::id());
    std::env::temp_dir()
        .join(file)
        .to_string_lossy()
        .into_owned()
}
