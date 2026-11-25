//! The way to the target (specs/oneterm, "M10 in detail"): through each jump host's
//! `direct-tcpip` channel, signed in at every hop.

use std::sync::Arc;
use std::time::Duration;

use russh::client::{self, Handle};
use tokio::net::TcpStream;

use crate::auth::CANCELLED;
use crate::hostkey::Checker;
use crate::prompt::{Ask, Event, Prompt};
use crate::traffic::{Counted, Traffic};
use crate::{Hop, Options};

const CONNECT: Duration = Duration::from_secs(20);
/// The most jump hosts on a way.
const HOPS: usize = 4;

pub(crate) type Link = Handle<Checker>;

fn said(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Connects to the target through the jump hosts: the target's connection, and the jump
/// hosts' in order, which must live as long as it.
pub(crate) async fn reach(
    options: &Options,
    prompt: &Arc<Prompt>,
    traffic: &Arc<Traffic>,
) -> Result<(Link, Vec<Link>), String> {
    let mut hops: Vec<Hop> = options.jumps.iter().take(HOPS).cloned().collect();
    hops.push(Hop { target: options.target.clone(), auth: options.auth.clone() });
    let last = hops.len() - 1;
    let mut through: Vec<Link> = Vec::new();
    for (at, hop) in hops.iter_mut().enumerate() {
        // No user given: asked, the OS's filled in.
        if hop.target.user.is_empty() {
            let ask = Ask::User { host: hop.target.label(), user: crate::target::os_user() };
            let user = prompt.ask(ask).await.ok_or(CANCELLED)?;
            hop.target.user =
                if user.trim().is_empty() { crate::target::os_user() } else { user.trim().into() };
        }
        let hop = &*hop;
        let named = |why: String| match at == last {
            true => why,
            false => format!("jump host {}: {why}", hop.target.label()),
        };
        prompt.tell(Event::Connecting(hop.target.label()));
        let mut handle = open(hop, through.last(), options, prompt, traffic).await.map_err(named)?;
        prompt.tell(Event::Authenticating(hop.target.label()));
        crate::auth::authenticate(&mut handle, hop, options, prompt).await.map_err(named)?;
        through.push(handle);
    }
    let target = through.pop().ok_or("no target")?;
    Ok((target, through))
}

/// The connection to one hop: by TCP for the first, else through the hop before.
async fn open(
    hop: &Hop,
    before: Option<&Link>,
    options: &Options,
    prompt: &Arc<Prompt>,
    traffic: &Arc<Traffic>,
) -> Result<Link, String> {
    let (host, port) = (hop.target.host.clone(), hop.target.port);
    let config = Arc::new(client::Config {
        keepalive_interval: options.keepalive,
        keepalive_max: options.keepalive_max.max(1),
        ..Default::default()
    });
    let checker = Checker {
        host: host.clone(),
        port,
        file: options.known_hosts.clone(),
        prompt: prompt.clone(),
        // Only the target is asked for remote forwards.
        remotes: options.forwards.clone(),
    };
    let refused = |e: russh::Error| match e {
        russh::Error::UnknownKey => {
            let why = prompt.why.lock().unwrap_or_else(|e| e.into_inner()).take();
            why.unwrap_or_else(|| "host key verification failed".into())
        }
        e => e.to_string(),
    };
    match before {
        None => {
            let socket = match tokio::time::timeout(CONNECT, TcpStream::connect((host.as_str(), port))).await
            {
                Ok(socket) => socket.map_err(said)?,
                Err(_) => return Err("timed out".into()),
            };
            let _ = socket.set_nodelay(true);
            let socket = Counted { inner: socket, traffic: traffic.clone() };
            client::connect_stream(config, socket, checker).await.map_err(refused)
        }
        Some(before) => {
            let channel =
                before.channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0).await.map_err(said)?;
            client::connect_stream(config, channel.into_stream(), checker).await.map_err(refused)
        }
    }
}
