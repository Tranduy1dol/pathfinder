//! Starting and maintaining processes, and the main entry point

use super::{sub_process::launch_python, Command, Handle, SharedReceiver, SubProcessEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};

/// Starts to maintain a pool of `count` sub-processes which execute the calls.
///
/// In general, the launching currently assumes:
///
/// - user has entered the python virtual environment created for this project per instructions
/// under `$REPO_ROOT/py/README.md`
/// - `call.py` can be found from the `$VIRTUAL_ENV/../src/call.py`
/// - user has compatible python, 3.7+ should work just fine
///
/// Returns an error if executing calls in a sub-process is not supported.
pub async fn start(
    database_path: PathBuf,
    count: std::num::NonZeroUsize,
    stop_flag: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<(Handle, tokio::task::JoinHandle<()>)> {
    // channel sizes are conservative but probably enough for many workers; should investigate mpmc
    // if the lock overhead on command_rx before making these deeper.
    let (command_tx, command_rx) = mpsc::channel(1);
    let (status_tx, mut status_rx) = mpsc::channel(1);
    // this will never need to become deeper
    let (child_shutdown_tx, _) = broadcast::channel(1);
    let command_rx: SharedReceiver<Command> = Arc::new(Mutex::new(command_rx));

    // TODO: might be better to use tokio's JoinSet?
    let mut joinhandles = futures::stream::FuturesUnordered::new();

    let jh = tokio::task::spawn(launch_python(
        database_path.clone(),
        Arc::clone(&command_rx),
        status_tx.clone(),
        child_shutdown_tx.subscribe(),
    ));

    joinhandles.push(jh);

    match status_rx.recv().await {
        Some(SubProcessEvent::ProcessLaunched(_pid)) => {
            // good, now we can launch the other processes requested later
        }
        Some(SubProcessEvent::Failure(_maybe_pid, e)) => {
            return Err(e.context("Launch first python executor"));
        }
        Some(SubProcessEvent::CommandHandled(..)) => {
            unreachable!("first message must not be CommandHandled");
        }
        None => unreachable!("the status_tx exists"),
    }

    let handle = Handle {
        command_tx: command_tx.clone(),
    };

    let jh = tokio::task::spawn(async move {
        use futures::stream::StreamExt;
        const WAIT_BEFORE_SPAWN: std::time::Duration = std::time::Duration::from_secs(1);

        // use a sleep activated periodically before launching new processes
        // not to overwhelm the system
        let wait_before_spawning = tokio::time::sleep(WAIT_BEFORE_SPAWN);
        tokio::pin!(wait_before_spawning);

        tokio::pin!(stop_flag);

        loop {
            let mut spawn = false;
            tokio::select! {
                _ = &mut stop_flag => {
                    // this should be enough to kick everyone off the locking, queue receiving
                    let _ = child_shutdown_tx.send(());
                    let _ = joinhandles.into_future().await;
                    // just exit
                    return;
                }
                Some(evt) = status_rx.recv() => {
                    match evt {
                        SubProcessEvent::ProcessLaunched(_) => {},
                        SubProcessEvent::CommandHandled(_pid, timings, status) => {
                            println!("{status:?}: {timings:?}");
                        },
                        SubProcessEvent::Failure(..) => { /* this is really needed just for startup */ },
                    }
                },
                Some(_maybe_info) = joinhandles.next() => {
                    println!("one of our python processes have expired: {_maybe_info:?}");
                    // we should spawn it immediatedly if empty
                    spawn = joinhandles.is_empty();
                }
                _ = &mut wait_before_spawning => {
                    // spawn if needed
                    spawn = count.get() > joinhandles.len();
                }
            }

            if spawn {
                let jh = tokio::task::spawn(launch_python(
                    database_path.clone(),
                    Arc::clone(&command_rx),
                    status_tx.clone(),
                    child_shutdown_tx.subscribe(),
                ));

                joinhandles.push(jh);
            } else if count.get() > joinhandles.len() && wait_before_spawning.is_elapsed() {
                wait_before_spawning
                    .as_mut()
                    .reset(tokio::time::Instant::now() + WAIT_BEFORE_SPAWN);
            }
        }
    });

    Ok((handle, jh))
}
