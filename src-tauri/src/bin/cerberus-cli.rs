// A console program, unlike the app, so a shell waits for it and sees what it prints.
fn main() -> std::process::ExitCode {
    cerberus_lib::console()
}
