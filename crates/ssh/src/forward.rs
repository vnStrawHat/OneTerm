//! A connection's forwards (specs/oneterm, "M10 in detail"): local ports carried through
//! `direct-tcpip` channels, ports the server is asked to listen on carried back here, and a
//! SOCKS5 proxy whose connections go out from the server.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;

use russh::Channel;
use russh::client::Msg;
use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::prompt::Prompt;
use crate::route::Link;
use crate::{Forward, Way};

/// Starts each forward; one that cannot listen, or that the server refuses, is said in the
/// space. The tasks end when they are aborted.
pub(crate) async fn start(handle: &Arc<Link>, forwards: &[Forward], prompt: &Prompt) -> Vec<JoinHandle<()>> {
    let mut tasks = Vec::new();
    for forward in forwards {
        if forward.way == Way::Remote {
            if let Err(e) = handle.tcpip_forward("localhost", forward.listen.into()).await {
                prompt.say(&format!("[forward {} was refused: {e}]\n", forward.label()));
            }
            continue;
        }
        let listener = match TcpListener::bind(("127.0.0.1", forward.listen)).await {
            Ok(listener) => listener,
            Err(e) => {
                prompt.say(&format!("[forward {} could not listen: {e}]\n", forward.label()));
                continue;
            }
        };
        let (handle, forward) = (handle.clone(), forward.clone());
        tasks.push(tokio::spawn(async move {
            while let Ok((socket, from)) = listener.accept().await {
                let (handle, forward) = (handle.clone(), forward.clone());
                tokio::spawn(async move {
                    let _ = match forward.way {
                        Way::Dynamic => socks(socket, &handle).await,
                        _ => local(socket, &handle, &forward, from.port()).await,
                    };
                });
            }
        }));
    }
    tasks
}

async fn local(mut socket: TcpStream, handle: &Link, to: &Forward, from: u16) -> std::io::Result<()> {
    let opened = handle.channel_open_direct_tcpip(to.host.clone(), to.port.into(), "127.0.0.1", from.into());
    if let Ok(channel) = opened.await {
        copy_bidirectional(&mut socket, &mut channel.into_stream()).await?;
    }
    Ok(())
}

/// The remote forward the server listens on `port` for, if one was asked.
pub(crate) fn asked(port: u32, remotes: &[Forward]) -> Option<Forward> {
    remotes.iter().find(|it| it.way == Way::Remote && u32::from(it.listen) == port).cloned()
}

/// A connection the server opened back on a remote forward's port, accepted: carried to its
/// host and port here.
pub(crate) fn back(channel: Channel<Msg>, to: Forward) {
    tokio::spawn(async move {
        if let Ok(mut socket) = TcpStream::connect((to.host.as_str(), to.port)).await {
            let _ = copy_bidirectional(&mut socket, &mut channel.into_stream()).await;
        }
    });
}

/// One SOCKS5 connection, no auth and CONNECT only (RFC 1928): the address asked is reached
/// through a channel from the server.
async fn socks(mut socket: TcpStream, handle: &Link) -> std::io::Result<()> {
    let mut head = [0u8; 2];
    socket.read_exact(&mut head).await?;
    let mut methods = vec![0u8; head[1].into()];
    socket.read_exact(&mut methods).await?;
    if head[0] != 5 || !methods.contains(&0) {
        return socket.write_all(&[5, 0xff]).await;
    }
    socket.write_all(&[5, 0]).await?;
    let mut ask = [0u8; 4];
    socket.read_exact(&mut ask).await?;
    let host = match ask[3] {
        1 => Ipv4Addr::from(socket.read_u32().await?).to_string(),
        4 => Ipv6Addr::from(socket.read_u128().await?).to_string(),
        3 => {
            let mut name = vec![0u8; socket.read_u8().await?.into()];
            socket.read_exact(&mut name).await?;
            String::from_utf8_lossy(&name).into_owned()
        }
        _ => return reply(&mut socket, 8).await,
    };
    let port = socket.read_u16().await?;
    if ask[1] != 1 {
        return reply(&mut socket, 7).await;
    }
    match handle.channel_open_direct_tcpip(host, port.into(), "127.0.0.1", 0).await {
        Ok(channel) => {
            reply(&mut socket, 0).await?;
            copy_bidirectional(&mut socket, &mut channel.into_stream()).await.map(|_| ())
        }
        Err(_) => reply(&mut socket, 5).await,
    }
}

/// SOCKS's answer: `code` (0 done, 5 refused, 7 a command not taken, 8 an address not
/// taken), the bound address left empty.
async fn reply(socket: &mut TcpStream, code: u8) -> std::io::Result<()> {
    socket.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await
}
