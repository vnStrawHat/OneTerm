//! A connection from start to end (specs/oneterm, "M7 in detail"): the socket, the host key, the
//! authentication, the shell, and what the shell's channel says until it closes.

use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use russh::ChannelMsg;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::auth::CANCELLED;
use crate::prompt::{Ask, Cmd, Event, Out, Prompt, Told};
use crate::route::Link;
use crate::{Options, Shared};

const PING: Duration = Duration::from_secs(5);

pub(crate) async fn run(
    options: Options,
    input: UnboundedReceiver<Cmd>,
    out: Out,
    told: Told,
    exited: Box<dyn FnOnce(i32) + Send>,
    shared: Arc<Shared>,
) {
    let prompt = Arc::new(Prompt::new(out, told, input, options.rows, options.cols));
    // A connection that fails before its shell is asked about: again, or given up.
    let code = loop {
        let why = match shell(&options, prompt.clone(), &shared).await {
            Ok(code) => break code,
            Err(why) => why,
        };
        if prompt.opened() || prompt.ask(Ask::Failed { reason: why.clone() }).await.is_none() {
            prompt.say(&format!("\n[connection closed: {why}]\n"));
            break 255;
        }
        // A cancel that came after the failure is not the new attempt's.
        let mut stale = pin!(shared.cancel.notified());
        let _ = stale.as_mut().poll(&mut Context::from_waker(Waker::noop()));
    };
    exited(code);
}

/// Reaches the target unless `Ssh::cancel` (or `close`) gives it up first.
async fn reach(
    options: &Options,
    prompt: &Arc<Prompt>,
    shared: &Arc<Shared>,
) -> Result<(Link, Vec<Link>), String> {
    let mut reaching = pin!(crate::route::reach(options, prompt, &shared.traffic));
    let mut cancelled = pin!(shared.cancel.notified());
    poll_fn(|cx| match reaching.as_mut().poll(cx) {
        Poll::Ready(done) => Poll::Ready(done),
        Poll::Pending => cancelled.as_mut().poll(cx).map(|()| Err(CANCELLED.to_string())),
    })
    .await
}

async fn shell(options: &Options, prompt: Arc<Prompt>, shared: &Arc<Shared>) -> Result<i32, String> {
    let (handle, jumps) = reach(options, &prompt, shared).await?;
    // What came while connecting: the last size is kept, typing is dropped.
    {
        let mut input = prompt.input.lock().await;
        while let Ok(cmd) = input.try_recv() {
            if !prompt.take(cmd) {
                return Err("closed".into());
            }
        }
    }
    let channel = handle.channel_open_session().await.map_err(|e| e.to_string())?;
    let (rows, cols) = prompt.size();
    let fail = |e: russh::Error| e.to_string();
    channel.request_pty(false, "xterm-256color", cols.into(), rows.into(), 0, 0, &[]).await.map_err(fail)?;
    channel.set_env(false, "COLORTERM", "truecolor").await.map_err(fail)?;
    channel.request_shell(false).await.map_err(fail)?;
    prompt.tell(Event::Open);
    let (mut read, write) = channel.split();
    // A ping every 5 s: how long the server takes to answer.
    let handle = Arc::new(handle);
    let forwards = crate::forward::start(&handle, &options.forwards, &prompt).await;
    *shared.handle.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle.clone());
    let (pings, latency) = (handle.clone(), shared.clone());
    let pinger = tokio::spawn(async move {
        loop {
            let start = Instant::now();
            if pings.send_ping().await.is_err() {
                break;
            }
            let micros = start.elapsed().as_micros().clamp(1, u128::from(u32::MAX)) as u32;
            latency.latency.store(micros, Ordering::Relaxed);
            tokio::time::sleep(PING).await;
        }
    });
    // What is typed goes to the shell on a task of its own.
    let typing = prompt.clone();
    let writer = tokio::spawn(async move {
        let mut input = typing.input.lock().await;
        while let Some(cmd) = input.recv().await {
            let sent = match cmd {
                Cmd::Input(bytes) => write.data_bytes(bytes).await,
                Cmd::Resize(rows, cols) => write.window_change(cols.into(), rows.into(), 0, 0).await,
                Cmd::Answer(_) => Ok(()),
                Cmd::Close => break,
            };
            if sent.is_err() {
                break;
            }
        }
        let _ = write.close().await;
    });
    let mut code = None;
    while let Some(message) = read.wait().await {
        match message {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => prompt.write(&data),
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status as i32),
            ChannelMsg::Close => break,
            _ => {}
        }
    }
    writer.abort();
    pinger.abort();
    for task in forwards {
        task.abort();
    }
    // The files go with the shell (specs/oneterm).
    *shared.handle.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *shared.files.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // The target first, then the jump hosts inward out.
    let _ = handle.disconnect(russh::Disconnect::ByApplication, "", "").await;
    for jump in jumps.iter().rev() {
        let _ = jump.disconnect(russh::Disconnect::ByApplication, "", "").await;
    }
    code.ok_or_else(|| "the connection dropped".to_string())
}
