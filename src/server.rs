use slab::Slab;

use std::{
    cmp::min,
    ffi::OsStr,
    future::Future,
    io::{self, ErrorKind, IoSlice},
    path::Path,
    pin::Pin,
    process::Stdio,
    task::{Context, Poll},
};

use tokio::{
    io::{stdout, AsyncRead, AsyncWrite, ReadBuf},
    net::UnixListener,
    process::{Child, Command},
};

use crate::{
    connection::{Connection, StdioConnection, UnixConnection},
    ring_buffer::RingBuffer,
};

#[derive(Debug)]
pub struct Server {
    listener: UnixListener,
    connections: Slab<Connection<4096>>,
    process: Child,
    output_buffer: RingBuffer<u8, 4096>,
}

impl Server {
    pub fn listen(
        path: impl AsRef<Path>,
        program: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> io::Result<Self> {
        let output_buffer = RingBuffer::new();

        let mut connections = Slab::with_capacity(1);
        connections.insert(Connection::Stdio(StdioConnection {
            stdout: stdout(),
            cursor: output_buffer.end(),
        }));

        Ok(Self {
            listener: UnixListener::bind(path)?,
            connections,
            process: Command::new(program)
                .args(args)
                .stdout(Stdio::piped())
                .spawn()?,
            output_buffer,
        })
    }

    fn poll_inner(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while self.poll_accept(cx)?.is_ready() {}

        while {
            while self.poll_write(cx)?.is_ready() {}
            self.poll_read(cx)?.is_ready()
        } {}

        Poll::Pending
    }

    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        match self.listener.poll_accept(cx)? {
            Poll::Ready((socket, _addr)) => Poll::Ready(Ok(self.connections.insert(
                Connection::Unix(UnixConnection {
                    socket,
                    cursor: self.output_buffer.end(),
                }),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_write(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        if self.output_buffer.is_empty() {
            return Poll::Pending;
        }

        // TODO: Figure out how to only poll the connection that woke up the task by wrapping `cx` passed to `poll_write_vectored`
        let mut min_bytes_written = self.output_buffer.len();
        for connection_id in 0..self.connections.capacity() {
            if let Some(connection) = self.connections.get_mut(connection_id) {
                let slices = self
                    .output_buffer
                    .slices_from(*connection.cursor())
                    .map(IoSlice::new);

                match Pin::new(&mut connection.async_write()).poll_write_vectored(cx, &slices) {
                    Poll::Ready(Ok(bytes_written)) => {
                        debug_assert!(bytes_written != 0);
                        *connection.cursor_mut() += bytes_written;
                        min_bytes_written = min(bytes_written, min_bytes_written);
                    }
                    Poll::Ready(Err(err)) if err.kind() == ErrorKind::BrokenPipe => {
                        self.connections.remove(connection_id);
                    }
                    Poll::Ready(Err(err)) => Err(err)?,
                    Poll::Pending => min_bytes_written = 0,
                }
            }
        }

        if min_bytes_written > 0 {
            self.output_buffer.remove(min_bytes_written);
            Poll::Ready(Ok(min_bytes_written))
        } else {
            Poll::Pending
        }
    }

    fn poll_read(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<usize>> {
        let mut total_bytes_read = 0;
        for unused_slice in &mut self.output_buffer.unused_slices() {
            if unused_slice.len() == 0 {
                break;
            }

            let mut read_buf = ReadBuf::uninit(*unused_slice);
            match Pin::new(&mut self.process.stdout.as_mut().unwrap())
                .poll_read(cx, &mut read_buf)?
            {
                Poll::Ready(()) => {
                    let bytes_read = read_buf.filled().len();
                    debug_assert!(bytes_read != 0);
                    total_bytes_read += bytes_read
                }
                Poll::Pending => break,
            }
        }

        if total_bytes_read > 0 {
            unsafe { self.output_buffer.assume_init(total_bytes_read) }
            Poll::Ready(Ok(total_bytes_read))
        } else {
            Poll::Pending
        }
    }
}

impl Future for Server {
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::into_inner(self).poll_inner(cx)
    }
}
