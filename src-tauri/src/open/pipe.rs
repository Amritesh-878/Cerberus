use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as sys;
#[cfg(windows)]
use windows as sys;

pub use sys::name;

// A request is a handful of names; a longer one is not from Cerberus.
const MOST_REQUEST: u64 = 16 * 1024;
// Askers that never write could otherwise pile up threads and handles without end.
const MOST_ASKERS: usize = 8;
// A real asker writes its line at once; one still silent by then is cut off and frees its slot.
const DEADLINE: Duration = Duration::from_secs(2);

/// The running app's answer: an exit code and what to print, or `None` when no app is listening.
pub fn ask(name: &str, args: &[String]) -> io::Result<Option<(i32, String)>> {
    let Some(mut stream) = sys::connect(name)? else {
        return Ok(None);
    };
    writeln!(stream, "{}", serde_json::to_string(args)?)?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    let (code, message) = reply.split_once('\n').unwrap_or((&reply, ""));
    let code = code.parse().map_err(|_| {
        io::Error::other("the running Cerberus sent an answer this one cannot read")
    })?;
    Ok(Some((code, message.to_owned())))
}

/// Listens for as long as the app runs; each asker gets its own thread, so a stalled one holds up nobody.
pub fn serve(
    name: &str,
    answer: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static,
) -> io::Result<()> {
    serve_within(name, DEADLINE, answer)
}

fn serve_within(
    name: &str,
    deadline: Duration,
    answer: impl Fn(&[String]) -> (i32, String) + Send + Sync + 'static,
) -> io::Result<()> {
    let mut listener = sys::Listener::bind(name)?;
    let answer = Arc::new(answer);
    let busy = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        loop {
            let stream = match listener.accept() {
                Ok(Ok(stream)) => stream,
                Ok(Err(e)) => {
                    eprintln!("a cerberus open asker left early: {e}");
                    continue;
                }
                Err(e) => {
                    eprintln!("cerberus open stopped listening: {e}");
                    return;
                }
            };
            let Some(slot) = Slot::take(&busy) else {
                // Closing the stream turns this asker away with an error, and keeps the listener free.
                continue;
            };
            let cut = match sys::cutter(&stream, deadline) {
                Ok(cut) => cut,
                Err(e) => {
                    eprintln!("could not answer cerberus open: {e}");
                    continue;
                }
            };
            let answer = Arc::clone(&answer);
            let worker = thread::spawn(move || {
                let _slot = slot;
                if let Err(e) = reply(&stream, &*answer) {
                    eprintln!("could not answer cerberus open: {e}");
                }
            });
            cut_off_after(worker, deadline, cut);
        }
    });
    Ok(())
}

/// Ends a worker's blocked calls once `deadline` passes, so it returns and frees its slot.
fn cut_off_after(
    worker: JoinHandle<()>,
    deadline: Duration,
    cut: impl Fn(&JoinHandle<()>) + Send + 'static,
) {
    thread::spawn(move || {
        thread::sleep(deadline);
        // Repeated, since a worker between calls has nothing to cancel yet and may block next.
        while !worker.is_finished() {
            cut(&worker);
            thread::sleep(Duration::from_millis(50));
        }
    });
}

/// One of the `MOST_ASKERS` answers allowed at once, given back when dropped.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(busy: &Arc<AtomicUsize>) -> Option<Self> {
        // Counted first and given back by `Drop` when full; `fetch_update` is deprecated on newer toolchains.
        let slot = Self(Arc::clone(busy));
        let before = busy.fetch_add(1, Ordering::AcqRel);
        (before < MOST_ASKERS).then_some(slot)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn reply(stream: &sys::Stream, answer: &dyn Fn(&[String]) -> (i32, String)) -> io::Result<()> {
    let mut line = String::new();
    BufReader::new(stream.take(MOST_REQUEST)).read_line(&mut line)?;
    let args: Vec<String> = serde_json::from_str(&line)?;
    let (code, message) = answer(&args);
    let mut out = stream;
    write!(out, "{code}\n{message}")?;
    sys::finish(stream)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    fn unique() -> String {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        sys::unique_name(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    fn connect(name: &str) -> sys::Stream {
        sys::connect(name).unwrap().unwrap()
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
        let _stalled = connect(&name);

        let answer = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn askers_past_the_limit_are_turned_away_and_slots_come_back() {
        let name = unique();
        serve(&name, |_| (0, "answered".to_owned())).unwrap();
        let stalled: Vec<sys::Stream> = (0..MOST_ASKERS).map(|_| connect(&name)).collect();

        assert!(ask(&name, &["open".to_owned()]).is_err());

        drop(stalled);
        let answer = (0..40).find_map(|_| {
            thread::sleep(Duration::from_millis(25));
            ask(&name, &["open".to_owned()]).ok().flatten()
        });
        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn silent_askers_are_cut_off_so_a_real_one_gets_through_while_they_stay() {
        let name = unique();
        serve_within(&name, Duration::from_millis(100), |_| {
            (0, "answered".to_owned())
        })
        .unwrap();
        let _stalled: Vec<sys::Stream> = (0..MOST_ASKERS).map(|_| connect(&name)).collect();

        // Real pipes need real time, so it polls up to 4 s rather than betting on one sleep.
        let answer = (0..160).find_map(|_| {
            thread::sleep(Duration::from_millis(25));
            ask(&name, &["open".to_owned()]).ok().flatten()
        });

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn askers_that_never_read_the_answer_are_cut_off_too() {
        let name = unique();
        // Slower than the deadline, so the first cancel finds nothing pending and the write blocks after.
        serve_within(&name, Duration::from_millis(100), |_| {
            thread::sleep(Duration::from_millis(150));
            (0, "answered".to_owned())
        })
        .unwrap();
        let _deaf: Vec<sys::Stream> = (0..MOST_ASKERS)
            .map(|_| {
                let mut stream = connect(&name);
                writeln!(stream, r#"["open"]"#).unwrap();
                stream
            })
            .collect();

        thread::sleep(Duration::from_millis(600));
        let answer = ask(&name, &["open".to_owned()]).unwrap();

        assert_eq!(answer, Some((0, "answered".to_owned())));
    }

    #[test]
    fn a_second_app_cannot_take_the_name() {
        let name = unique();
        serve(&name, |_| (0, String::new())).unwrap();

        assert!(serve(&name, |_| (0, String::new())).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_left_by_a_crashed_app_is_no_answer_and_is_taken_over() {
        let name = unique();
        drop(std::os::unix::net::UnixListener::bind(&name).unwrap());

        assert_eq!(ask(&name, &[]).unwrap(), None);
        serve(&name, |_| (0, "answered".to_owned())).unwrap();
        assert_eq!(
            ask(&name, &["open".to_owned()]).unwrap(),
            Some((0, "answered".to_owned()))
        );
    }
}
