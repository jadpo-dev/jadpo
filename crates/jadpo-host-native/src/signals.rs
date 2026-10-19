//! The signals that ask a native process to stop (chunk 6.7): SIGTERM,
//! which a deployment sends before it replaces the process, and SIGINT,
//! ctrl-c under `jadpo dev`.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;

/// Watches for SIGTERM and SIGINT on a thread of its own, giving a flag
/// that turns true at the first: the server then drains
/// ([`crate::http::Server::stop`]), its background work giving back what
/// it holds, and the process ends as it chooses. A second ends the
/// process at once, with the shell's code for that signal (143 for
/// SIGTERM, 130 for SIGINT). Once this has returned, neither signal ends
/// the process on its own any more.
pub fn stop_signals() -> io::Result<Arc<AtomicBool>> {
    let stop = Arc::new(AtomicBool::new(false));
    let (ready, listening) = std::sync::mpsc::channel();
    let flag = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("jadpo-signals".into())
        .spawn(move || watch(&flag, &ready))?;
    listening
        .recv()
        .map_err(|_| io::Error::other("the signal watcher didn't start"))??;
    Ok(stop)
}

/// Sets `stop` at the first signal and ends the process at the second,
/// once `ready` has been told whether the signals are watched.
#[cfg(unix)]
fn watch(stop: &AtomicBool, ready: &Sender<io::Result<()>>) {
    use std::task::Poll;
    use tokio::signal::unix::{SignalKind, signal};
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    runtime.block_on(async {
        let (mut term, mut interrupt) = match (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) {
            (Ok(term), Ok(interrupt)) => (term, interrupt),
            (Err(error), _) | (_, Err(error)) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        let _ = ready.send(Ok(()));
        loop {
            let code = std::future::poll_fn(|cx| {
                if term.poll_recv(cx).is_ready() {
                    return Poll::Ready(143);
                }
                if interrupt.poll_recv(cx).is_ready() {
                    return Poll::Ready(130);
                }
                Poll::Pending
            })
            .await;
            if stop.swap(true, Ordering::SeqCst) {
                std::process::exit(code);
            }
        }
    });
}

/// Ctrl-c only, where there is no SIGTERM.
#[cfg(not(unix))]
fn watch(stop: &AtomicBool, ready: &Sender<io::Result<()>>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    runtime.block_on(async {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                return;
            }
            if stop.swap(true, Ordering::SeqCst) {
                std::process::exit(130);
            }
        }
    });
}
