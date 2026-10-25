//! Falhas de rede na camada TCP: o cliente vê conexão recusada ou cortada, como na vida real.

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

use crate::state::Shared;

pub(crate) struct FaultyListener {
    inner: TcpListener,
    shared: Arc<Shared>,
}

impl FaultyListener {
    pub(crate) fn new(inner: TcpListener, shared: Arc<Shared>) -> FaultyListener {
        FaultyListener { inner, shared }
    }
}

impl axum::serve::Listener for FaultyListener {
    type Io = FaultyIo;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (FaultyIo, SocketAddr) {
        loop {
            match self.inner.accept().await {
                // Sem rede, a conexão cai na primeira leitura (ver `FaultyIo::check`).
                Ok((inner, addr)) => {
                    return (
                        FaultyIo {
                            inner,
                            shared: self.shared.clone(),
                            dead: false,
                        },
                        addr,
                    );
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }
}

/// Conexão que cai quando o simulado fica sem rede ou esgota o limite de bytes.
pub(crate) struct FaultyIo {
    inner: TcpStream,
    shared: Arc<Shared>,
    /// Depois de cair, a conexão não volta: nem a resposta de erro chega ao cliente.
    dead: bool,
}

fn cut() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionReset,
        "conexão derrubada pelo Drive simulado",
    )
}

impl FaultyIo {
    fn check(&mut self, transferred: usize) -> io::Result<()> {
        self.dead |= self.shared.offline.load(Ordering::SeqCst) || self.shared.consume(transferred);
        if self.dead { Err(cut()) } else { Ok(()) }
    }
}

impl AsyncRead for FaultyIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check(0)?;
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = poll {
            self.check(buf.filled().len() - before)?;
        }
        poll
    }
}

impl AsyncWrite for FaultyIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.check(0)?;
        let poll = Pin::new(&mut self.inner).poll_write(cx, data);
        if let Poll::Ready(Ok(n)) = poll {
            self.check(n)?;
        }
        poll
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
