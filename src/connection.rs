use std::{
    io::{self, IoSlice},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncWrite, Stdout},
    net::UnixStream,
};

use crate::ring_buffer::RingBufferCursor;

#[derive(Debug)]
pub enum Connection<const BUFFER_CAPACITY: usize> {
    Stdio(StdioConnection<BUFFER_CAPACITY>),
    Unix(UnixConnection<BUFFER_CAPACITY>),
}

impl<const BUFFER_CAPACITY: usize> Connection<BUFFER_CAPACITY> {
    pub fn cursor(&self) -> &RingBufferCursor<BUFFER_CAPACITY> {
        match self {
            Self::Stdio(stdio) => &stdio.cursor,
            Self::Unix(unix) => &unix.cursor,
        }
    }

    pub fn cursor_mut(&mut self) -> &mut RingBufferCursor<BUFFER_CAPACITY> {
        match self {
            Self::Stdio(stdio) => &mut stdio.cursor,
            Self::Unix(unix) => &mut unix.cursor,
        }
    }

    pub fn async_write(&mut self) -> ConnectionAsyncWrite {
        match self {
            Self::Stdio(stdio) => ConnectionAsyncWrite::Stdout(&mut stdio.stdout),
            Self::Unix(unix) => ConnectionAsyncWrite::Unix(&mut unix.socket),
        }
    }
}

#[derive(Debug)]
pub enum ConnectionAsyncWrite<'a> {
    Stdout(&'a mut Stdout),
    Unix(&'a mut UnixStream),
}

impl AsyncWrite for ConnectionAsyncWrite<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match Pin::into_inner(self) {
            Self::Stdout(stdout) => Pin::new(stdout).poll_write(cx, buf),
            Self::Unix(unix) => Pin::new(unix).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match Pin::into_inner(self) {
            Self::Stdout(stdout) => Pin::new(stdout).poll_flush(cx),
            Self::Unix(unix) => Pin::new(unix).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match Pin::into_inner(self) {
            Self::Stdout(stdout) => Pin::new(stdout).poll_shutdown(cx),
            Self::Unix(unix) => Pin::new(unix).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match Pin::into_inner(self) {
            Self::Stdout(stdout) => Pin::new(stdout).poll_write_vectored(cx, bufs),
            Self::Unix(unix) => Pin::new(unix).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Stdout(stdout) => stdout.is_write_vectored(),
            Self::Unix(unix) => unix.is_write_vectored(),
        }
    }
}

#[derive(Debug)]
pub struct StdioConnection<const BUFFER_CAPACITY: usize> {
    pub stdout: Stdout,
    pub cursor: RingBufferCursor<BUFFER_CAPACITY>,
}

#[derive(Debug)]
pub struct UnixConnection<const BUFFER_CAPACITY: usize> {
    pub socket: UnixStream,
    pub cursor: RingBufferCursor<BUFFER_CAPACITY>,
}
