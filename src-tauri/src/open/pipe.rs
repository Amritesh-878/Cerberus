use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};

// A request is a handful of names; a longer one is not from Cerberus.
const MOST_REQUEST: u64 = 16 * 1024;
// Askers that never write could otherwise pile up threads and handles without end.
const MOST_ASKERS: usize = 8;
// A real asker writes its line at once; one still silent by then is cut off and frees its slot.
#[cfg(windows)]
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);

/// Per Windows session, so two people signed in at once each reach their own app.
pub fn name(identifier: &str) -> String {
    format!(r"\\.\pipe\{identifier}-open-{}", session())
}

#[cfg(windows)]
fn session() -> u32 {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut session = 0;
    // On failure this stays 0, which still names one pipe for the whole machine.
    unsafe { ProcessIdToSessionId(std::process::id(), &raw mut session) };
    session
}

#[cfg(not(windows))]
fn session() -> u32 {
    0
}

/// The running app's answer: an exit code and what to print, or `None` when no app is listening.
pub fn ask(name: &str, args: &[String]) -> io::Result<Option<(i32, String)>> {
    let Some(mut pipe) = connect(name)? else {
        return Ok(None);
    };
    allow_focus(&pipe);
    writeln!(pipe, "{}", serde_json::to_string(args)?)?;
    let mut reply = String::new();
    pipe.read_to_string(&mut reply)?;
    let (code, message) = reply.split_once('\n').unwrap_or((&reply, ""));
    let code = code.parse().map_err(|_| {
        io::Error::other("the running Cerberus sent an answer this one cannot read")
    })?;
    Ok(Some((code, message.to_owned())))
}

fn connect(name: &str) -> io::Result<Option<File>> {
    // ERROR_PIPE_BUSY: the app is answering someone else, and a fresh instance follows at once.
    const PIPE_BUSY: i32 = 231;
    let mut tries = 20;
    loop {
        match File::options().read(true).write(true).open(name) {
            Ok(pipe) => return Ok(Some(pipe)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && tries > 1 => {
                tries -= 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Windows lets only the foreground process raise a window, so the asker passes that right on.
#[cfg(windows)]
fn allow_focus(pipe: &File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;
    let mut server = 0;
    unsafe {
        if GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut server) != 0 {
            AllowSetForegroundWindow(server);
        }
    }
}

#[cfg(not(windows))]
fn allow_focus(_pipe: &File) {}

/// Listens for as long as the app runs; each asker gets its own thread, so a stalled one holds up nobody.
#[cfg(windows)]
pub fn serve(
    name: &str,
    answer: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static,
) -> io::Result<()> {
    serve_within(name, DEADLINE, answer)
}

#[cfg(windows)]
fn serve_within(
    name: &str,
    deadline: std::time::Duration,
    answer: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static,
) -> io::Result<()> {
    let mut waiting = listen(name, true)?;
    let name = name.to_owned();
    let answer = std::sync::Arc::new(answer);
    let busy = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    std::thread::spawn(move || {
        loop {
            let pipe = accept(waiting);
            // The next instance exists before this one can close, so an asker never finds the name missing.
            waiting = match listen(&name, false) {
                Ok(next) => next,
                Err(e) => {
                    eprintln!("cerberus open stopped listening: {e}");
                    return;
                }
            };
            let pipe = match pipe {
                Ok(pipe) => pipe,
                Err(e) => {
                    eprintln!("a cerberus open asker left early: {e}");
                    continue;
                }
            };
            let Some(slot) = Slot::take(&busy) else {
                // Closing the pipe turns this asker away with an error, and keeps the listener free.
                continue;
            };
            let answer = std::sync::Arc::clone(&answer);
            let worker = std::thread::spawn(move || {
                let _slot = slot;
                if let Err(e) = reply(pipe, &*answer) {
                    eprintln!("could not answer cerberus open: {e}");
                }
            });
            cut_off_after(worker, deadline);
        }
    });
    Ok(())
}

/// Ends a worker's blocked pipe calls once `deadline` passes, so it returns and frees its slot.
#[cfg(windows)]
fn cut_off_after(worker: std::thread::JoinHandle<()>, deadline: std::time::Duration) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::CancelSynchronousIo;
    std::thread::spawn(move || {
        std::thread::sleep(deadline);
        // Repeated, since a worker between calls has nothing to cancel yet and may block next.
        while !worker.is_finished() {
            // The handle stays valid while `worker` is held; a worker with nothing pending just fails the call.
            unsafe { CancelSynchronousIo(worker.as_raw_handle()) };
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

/// One of the `MOST_ASKERS` answers allowed at once, given back when dropped.
#[cfg(windows)]
struct Slot(std::sync::Arc<std::sync::atomic::AtomicUsize>);

#[cfg(windows)]
impl Slot {
    fn take(busy: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> Option<Self> {
        // Counted first and given back by `Drop` when full; `fetch_update` is deprecated on newer toolchains.
        let slot = Self(std::sync::Arc::clone(busy));
        let before = busy.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        (before < MOST_ASKERS).then_some(slot)
    }
}

#[cfg(windows)]
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(not(windows))]
pub fn serve(
    _name: &str,
    _answer: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static,
) -> io::Result<()> {
    Ok(())
}

/// A new pipe instance; the first one fails if another process already owns the name.
#[cfg(windows)]
fn listen(name: &str, first: bool) -> io::Result<Listening> {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let open_mode = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // A valid handle that nothing else owns, so the File closes it.
    Ok(Listening(File::from(unsafe {
        OwnedHandle::from_raw_handle(handle)
    })))
}

#[cfg(windows)]
struct Listening(File);

/// Waits for the next asker.
#[cfg(windows)]
fn accept(pipe: Listening) -> io::Result<File> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED;
    use windows_sys::Win32::System::Pipes::ConnectNamedPipe;
    let Listening(pipe) = pipe;
    let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
    if connected == 0 {
        let e = io::Error::last_os_error();
        // A client that connected before the wait began is already there.
        if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED.cast_signed()) {
            return Err(e);
        }
    }
    Ok(pipe)
}

#[cfg(windows)]
fn reply(mut pipe: File, answer: &dyn Fn(&[String]) -> (i32, String)) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new((&pipe).take(MOST_REQUEST)).read_line(&mut line)?;
    let args: Vec<String> = serde_json::from_str(&line)?;
    let (code, message) = answer(&args);
    write!(pipe, "{code}\n{message}")?;
    // Closing without this can drop a reply the asker has not read yet.
    pipe.sync_all()
}

#[cfg(all(test, windows))]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    fn unique() -> String {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        format!(r"\\.\pipe\cerberus-test-{}-{n}", std::process::id())
    }

    #[test]
    fn nobody_listening_is_no_answer() {
        assert_eq!(ask(&unique(), &[]).unwrap(), None);
    }

    #[test]
    fn the_running_app_answers_each_request() {
        let name = unique();
        serve(&name, |args| {
            (args.len().try_into().unwrap(), args.join("|"))
        })
        .unwrap();

        let first = ask(&name, &["open".to_owned(), "a b\nc".to_owned()]).unwrap();
        let second = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(first, Some((2, "open|a b\nc".to_owned())));
        assert_eq!(second, Some((1, "open".to_owned())));
    }

    #[test]
    fn an_asker_that_never_writes_holds_up_nobody() {
        let name = unique();
        serve(&name, |_| (0, "answered".to_owned())).unwrap();
        let _stalled = connect(&name).unwrap().unwrap();

        let answer = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn askers_past_the_limit_are_turned_away_and_slots_come_back() {
        let name = unique();
        serve(&name, |_| (0, "answered".to_owned())).unwrap();
        let stalled: Vec<File> = (0..MOST_ASKERS)
            .map(|_| connect(&name).unwrap().unwrap())
            .collect();

        assert!(ask(&name, &["open".to_owned()]).is_err());

        drop(stalled);
        let answer = (0..40).find_map(|_| {
            std::thread::sleep(std::time::Duration::from_millis(25));
            ask(&name, &["open".to_owned()]).ok().flatten()
        });
        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn silent_askers_are_cut_off_so_a_real_one_gets_through_while_they_stay() {
        let name = unique();
        serve_within(&name, std::time::Duration::from_millis(100), |_| {
            (0, "answered".to_owned())
        })
        .unwrap();
        let _stalled: Vec<File> = (0..MOST_ASKERS)
            .map(|_| connect(&name).unwrap().unwrap())
            .collect();

        std::thread::sleep(std::time::Duration::from_millis(400));
        let answer = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn askers_that_never_read_the_answer_are_cut_off_too() {
        let name = unique();
        // Slower than the deadline, so the first cancel finds nothing pending and the write blocks after.
        serve_within(&name, std::time::Duration::from_millis(100), |_| {
            std::thread::sleep(std::time::Duration::from_millis(150));
            (0, "answered".to_owned())
        })
        .unwrap();
        let _deaf: Vec<File> = (0..MOST_ASKERS)
            .map(|_| {
                let mut pipe = connect(&name).unwrap().unwrap();
                writeln!(pipe, r#"["open"]"#).unwrap();
                pipe
            })
            .collect();

        std::thread::sleep(std::time::Duration::from_millis(600));
        let answer = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn a_second_app_cannot_take_the_name() {
        let name = unique();
        serve(&name, |_| (0, String::new())).unwrap();

        assert!(serve(&name, |_| (0, String::new())).is_err());
    }
}
