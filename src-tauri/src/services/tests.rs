use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::accounts::testdir::TestDir;
use crate::services::url::strip_escapes;
use crate::sessions::procs::is_running;

const WAIT: Duration = Duration::from_secs(40);

struct TestSink {
    views: Arc<Mutex<Vec<View>>>,
}

impl Sink for TestSink {
    fn changed(&self, view: View) {
        self.views.lock().unwrap().push(view);
    }

    fn shell(&self, _wanted: Option<&str>) -> Option<ProgramChoice> {
        test_shell()
    }
}

#[cfg(windows)]
fn test_shell() -> Option<ProgramChoice> {
    crate::programs::Finder::from_env()
        .choices()
        .into_iter()
        .find(|c| c.id == "powershell" && c.program.is_some())
}

#[cfg(unix)]
fn test_shell() -> Option<ProgramChoice> {
    Some(ProgramChoice {
        id: "sh".into(),
        label: "sh".into(),
        program: Some("/bin/sh".into()),
        args: vec!["-l".into()],
        image_paste_key: None,
        is_shell: true,
        install_command: None,
        install_shell: None,
    })
}

// Starts a child that outlives the shell's own wish to exit, prints a fake address, then exits.
#[cfg(windows)]
const STAND_IN: &str = "$c = Start-Process -FilePath powershell -ArgumentList '-NoProfile','-Command','Start-Sleep 120' -PassThru -WindowStyle Hidden; Write-Output \"child=$($c.Id)=\"; Write-Output 'ready on http://localhost:4321'; Start-Sleep 2; exit 3";
#[cfg(unix)]
const STAND_IN: &str =
    "sleep 120 & echo \"child=$!=\"; echo 'ready on http://localhost:4321'; sleep 2; exit 3";

#[cfg(windows)]
const STAYS_UP: &str = "$c = Start-Process -FilePath powershell -ArgumentList '-NoProfile','-Command','Start-Sleep 120' -PassThru -WindowStyle Hidden; Write-Output \"child=$($c.Id)=\"; Start-Sleep 120";
#[cfg(unix)]
const STAYS_UP: &str = "sleep 120 & echo \"child=$!=\"; sleep 120";

fn draft(command: &str, restart: bool) -> Draft {
    Draft {
        name: "stand-in".into(),
        command: command.into(),
        folder: None,
        shell: None,
        start_with_app: false,
        restart,
    }
}

fn services(dir: &TestDir) -> (Services, Arc<Mutex<Vec<View>>>) {
    let views = Arc::default();
    let sink = TestSink {
        views: Arc::clone(&views),
    };
    (
        Services::new(dir.path(), dir.path().to_path_buf(), sink),
        views,
    )
}

/// The child pids the stand-in printed so far, from the replayed output.
fn children(services: &Services, id: ServiceId) -> Vec<u32> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let into = Arc::clone(&seen);
    let watch = services
        .watch(id, move |bytes| {
            into.lock().unwrap().extend_from_slice(bytes)
        })
        .unwrap();
    services.unwatch(watch);
    let text = strip_escapes(&String::from_utf8_lossy(&seen.lock().unwrap()));
    text.split("child=")
        .skip(1)
        .filter_map(|rest| rest.split('=').next()?.trim().parse().ok())
        .collect()
}

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn phase(services: &Services) -> Phase {
    services.view().services[0].phase
}

fn gone(pid: u32) -> bool {
    !is_running(pid)
}

#[test]
fn a_service_that_exits_is_restarted_and_its_last_child_goes_with_it() {
    let dir = TestDir::new();
    let (services, views) = services(&dir);
    services.add(&draft(STAND_IN, true)).unwrap();

    services.start(0).unwrap();
    wait_for("the address", || services.url(0).is_some());
    let url = services.url(0);
    wait_for("a second run", || children(&services, 0).len() >= 2);
    let pids = children(&services, 0);

    assert_eq!(url.as_deref(), Some("http://localhost:4321/"));
    wait_for("the first run's child to end", || gone(pids[0]));
    let seen: Vec<Phase> = views
        .lock()
        .unwrap()
        .iter()
        .map(|v| v.services[0].phase)
        .collect();
    assert!(seen.contains(&Phase::Restarting), "{seen:?}");

    services.stop(0).unwrap();
    assert_eq!(phase(&services), Phase::Stopped);
    assert_eq!(services.url(0), None);
    wait_for("the second run's child to end", || gone(pids[1]));
}

#[test]
fn stop_ends_the_shell_and_every_child_and_no_restart_follows() {
    let dir = TestDir::new();
    let (services, _) = services(&dir);
    services.add(&draft(STAYS_UP, true)).unwrap();
    services.start(0).unwrap();
    wait_for("the child", || !children(&services, 0).is_empty());
    let child = children(&services, 0)[0];
    assert!(is_running(child));

    services.stop(0).unwrap();

    wait_for("the child to end", || gone(child));
    thread::sleep(Duration::from_millis(1500));
    assert_eq!(
        phase(&services),
        Phase::Stopped,
        "Stop caused the exit, so nothing restarts"
    );
    assert_eq!(children(&services, 0).len(), 1);
}

#[test]
fn without_restart_an_exit_leaves_it_stopped() {
    let dir = TestDir::new();
    let (services, _) = services(&dir);
    services.add(&draft("exit 3", false)).unwrap();

    services.start(0).unwrap();

    wait_for("the exit", || phase(&services) == Phase::Stopped);
    thread::sleep(Duration::from_millis(1500));
    assert_eq!(phase(&services), Phase::Stopped);
}

#[test]
fn the_list_comes_back_after_a_restart_of_the_app_and_never_holds_the_command() {
    let dir = TestDir::new();
    {
        let (services, _) = services(&dir);
        services
            .add(&draft("echo FAKE-SECRET-token", true))
            .unwrap();
        services.fold(true).unwrap();
    }

    let (again, _) = services(&dir);

    let view = again.view();
    assert!(view.folded);
    assert_eq!(view.services[0].name, "stand-in");
    assert_eq!(view.services[0].phase, Phase::Stopped);
    assert!(
        !serde_json::to_string(&view)
            .unwrap()
            .contains("FAKE-SECRET")
    );
    assert_eq!(again.command(0).unwrap(), "echo FAKE-SECRET-token");
}

#[test]
fn removing_a_service_stops_it_and_forgets_it() {
    let dir = TestDir::new();
    let (services, _) = services(&dir);
    services.add(&draft(STAYS_UP, false)).unwrap();
    services.start(0).unwrap();
    wait_for("the child", || !children(&services, 0).is_empty());
    let child = children(&services, 0)[0];

    services.remove(0).unwrap();

    assert!(services.view().services.is_empty());
    wait_for("the child to end", || gone(child));
    assert!(matches!(services.start(0), Err(StoreError::Gone)));
}

#[test]
fn a_change_that_cannot_be_saved_changes_nothing() {
    let dir = TestDir::new();
    let (services, _) = services(&dir);
    services.add(&draft(STAYS_UP, false)).unwrap();
    services.start(0).unwrap();
    wait_for("the child", || !children(&services, 0).is_empty());
    let child = children(&services, 0)[0];
    // A folder where the file goes makes every later write fail, on both platforms.
    std::fs::remove_file(dir.path().join("services.json")).unwrap();
    std::fs::create_dir(dir.path().join("services.json")).unwrap();
    let mut other = draft("echo two", false);
    other.name = "two".into();
    let mut renamed = draft("echo changed", false);
    renamed.name = "renamed".into();

    assert!(services.add(&other).is_err());
    assert!(services.update(0, &renamed).is_err());
    assert!(services.remove(0).is_err());
    assert!(services.fold(true).is_err());

    let view = services.view();
    assert_eq!(view.services.len(), 1);
    assert_eq!(view.services[0].name, "stand-in");
    assert_eq!(services.command(0).unwrap(), STAYS_UP);
    assert!(!view.folded);
    assert!(is_running(child), "a failed remove leaves it running");
    assert!(
        matches!(services.add(&other), Err(StoreError::Io(_))),
        "the name is still free, only the write failed"
    );
    services.stop(0).unwrap();
    wait_for("the child to end", || gone(child));
}
