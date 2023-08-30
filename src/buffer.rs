use std::prelude::v1::*;

use std::io::{Error, ErrorKind, Write};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct ExpandVec<T> {
    pub raw: Vec<Vec<T>>,
    size: usize,
}

impl<T: Clone> ExpandVec<T> {
    pub fn new() -> Self {
        Self {
            raw: vec![],
            size: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn last_msg(&self) -> Option<&[T]> {
        self.raw.last().map(|n| n.as_slice())
    }

    pub fn pop(&mut self) -> Option<Vec<T>> {
        self.raw.pop()
    }

    pub fn push(&mut self, data: &[T]) {
        self.size += data.len();
        self.raw.push(data.to_vec());
    }

    pub fn move_to(&mut self, data: &mut Vec<T>) {
        data.reserve_exact(data.len() + self.size);
        let mut new = Vec::new();
        std::mem::swap(&mut self.raw, &mut new);
        for d in &new {
            data.extend_from_slice(d.as_slice());
        }
        self.size = 0;
    }
}

#[derive(Debug)]
pub struct BufferVec {
    pub raw: Vec<u8>,
    size: usize,
}

impl From<Vec<BufferVec>> for BufferVec {
    fn from(list: Vec<BufferVec>) -> Self {
        let cap = list.iter().map(|b| b.cap()).sum();
        let mut buf = Self::new(cap);
        for mut item in list {
            buf.copy_from(item.read());
            item.clear();
        }
        buf
    }
}

impl BufferVec {
    pub fn new(size: usize) -> Self {
        Self {
            raw: vec![0_u8; size],
            size: 0,
        }
    }

    pub fn move_to(&mut self, target: &mut Self) {
        target.copy_from(self.read());
        self.clear();
    }

    pub fn from_slice(slice: &[u8], cap: usize) -> Self {
        let mut buf = Self::new(cap);
        buf.copy_from(slice);
        buf
    }

    pub fn from_vec(mut vec: Vec<u8>, mut cap: usize) -> Self {
        if cap < vec.len() {
            cap = vec.len();
        }
        let size = vec.len();
        vec.resize(cap, 0);
        Self { raw: vec, size }
    }

    pub fn to_vec(mut self) -> Vec<u8> {
        self.raw.truncate(self.size);
        self.raw
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn ends_with(&self, needle: &[u8]) -> bool {
        self.read().ends_with(needle)
    }

    pub fn resize_cap(&mut self, size: usize) {
        self.raw.resize(size, 0);
        if self.size > self.raw.len() {
            self.size = self.raw.len();
        }
    }

    pub fn is_full(&self) -> bool {
        self.raw[self.size..].len() == 0
    }

    pub fn cap(&self) -> usize {
        self.raw.len()
    }

    pub fn read_n(&self, n: usize) -> Option<&[u8]> {
        if self.size >= n {
            return Some(&self.read()[..n]);
        }
        None
    }

    pub fn read(&self) -> &[u8] {
        &self.raw[..self.size]
    }

    pub fn write(&mut self) -> &mut [u8] {
        &mut self.raw[self.size..]
    }

    pub fn advance(&mut self, n: usize) {
        self.size += n;
    }

    pub fn rotate_left(&mut self, n: usize) {
        self.raw.rotate_left(n);
        self.size -= n;
    }

    pub fn clear(&mut self) {
        self.size = 0;
    }

    /// try to read from `reader` until it's fulled.
    pub fn fill_all_with<R>(&mut self, reader: &mut R) -> Result<(), Error>
    where
        R: std::io::Read,
    {
        while !self.is_full() {
            match reader.read(self.write()) {
                Ok(0) => return Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF")),
                Ok(incoming_bytes) => {
                    self.advance(incoming_bytes);
                    if self.is_full() {
                        break;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    pub fn fill_with<R>(&mut self, reader: &mut R) -> Result<usize, Error>
    where
        R: std::io::Read,
    {
        match reader.read(self.write()) {
            Ok(0) => Err(Error::new(ErrorKind::UnexpectedEof, "unexpected EOF")),
            Ok(incoming_bytes) => {
                self.advance(incoming_bytes);
                Ok(incoming_bytes)
            }
            Err(err) => Err(err),
        }
    }

    pub fn copy_from(&mut self, mut buf: &[u8]) -> usize {
        if self.write().len() < buf.len() {
            buf = &buf[..self.write().len()];
        }
        self.write().write_all(buf).unwrap();
        self.advance(buf.len());
        buf.len()
    }
}

#[derive(Debug)]
pub enum IOError {
    WouldBlock, // the data is not accept
    EOF { temporary: bool },
    Other(Error),
}

impl From<Error> for IOError {
    fn from(err: Error) -> IOError {
        use ErrorKind::*;
        if err.kind() == WouldBlock {
            return IOError::WouldBlock;
        }
        if [UnexpectedEof, BrokenPipe, ConnectionAborted].contains(&err.kind()) {
            return IOError::EOF { temporary: false };
        }
        IOError::Other(err)
    }
}

#[derive(Debug)]
pub struct WriteBuffer {
    cap: usize,
    buf: Vec<BufferVec>,
    idle_instant: Instant,
}

impl WriteBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap,
            buf: vec![BufferVec::new(cap)],
            idle_instant: Instant::now(),
        }
    }

    pub fn idle_duration(&self) -> Duration {
        self.idle_instant.elapsed()
    }

    // Write to the writer or copy to the buffer, no WouldBlock
    pub fn must_write<W: Write>(&mut self, writer: &mut W, data: &[u8]) -> Result<(), IOError> {
        let written = match self.flush_buffer(writer) {
            Ok(()) => match Self::raw_write(writer, data, &mut self.idle_instant) {
                Ok(written) => written,
                Err(IOError::WouldBlock) => 0,
                Err(err) => return Err(err),
            },
            Err(IOError::WouldBlock) => 0,
            Err(err) => return Err(err),
        };
        self.copy_to_buffer(&data[written..]);
        // glog::info!("must_write remaining buf: {}", self.buffered());
        Ok(())
    }

    pub fn write<W: Write>(&mut self, writer: &mut W, data: &[u8]) -> Result<(), IOError> {
        if self.buffered() > 0 {
            self.flush_buffer(writer)?;
        }
        let written = Self::raw_write(writer, data, &mut self.idle_instant)?;
        self.copy_to_buffer(&data[written..]);
        return Ok(());
    }

    // flush all buffered data or WouldBlock or EOF
    pub fn flush_buffer<W: Write>(&mut self, writer: &mut W) -> Result<(), IOError> {
        let mut result = Ok(());
        for buf in &mut self.buf {
            match Self::raw_write(writer, buf.read(), &mut self.idle_instant) {
                Ok(written) if written == buf.read().len() => {
                    buf.rotate_left(written);
                }
                Ok(written) => {
                    buf.rotate_left(written);
                    result = Err(IOError::WouldBlock);
                    break;
                }
                Err(err) => {
                    result = Err(err);
                    break;
                }
            }
        }

        // compact the buffer
        loop {
            match self.buf.first() {
                Some(first) => {
                    if first.len() == 0 {
                        self.buf.remove(0);
                    } else {
                        break;
                    }
                }
                None => break,
            }
        }

        result
    }

    pub fn buffered(&self) -> usize {
        self.buf.iter().map(|b| b.len()).sum()
    }

    fn copy_to_buffer(&mut self, data: &[u8]) {
        if data.len() == 0 {
            return;
        }
        match self.buf.last_mut() {
            Some(buf) => {
                if buf.write().len() > data.len() {
                    buf.write().write(data).unwrap();
                    buf.advance(data.len());
                    return;
                }
            }
            None => {}
        };
        let buf = BufferVec::from_slice(data, self.cap.max(data.len()));
        self.buf.push(buf);
    }

    fn raw_write<W>(w: &mut W, mut buf: &[u8], s: &mut Instant) -> Result<usize, IOError>
    where
        W: Write,
    {
        let mut written = 0;
        while !buf.is_empty() {
            let result = w.write(&buf);
            // glog::info!("raw write: {}, {:?}", buf.len(), result);
            match result {
                Ok(n) => {
                    written += n;
                    buf = &buf[n..]
                }
                Err(err)
                    if matches!(err.kind(), ErrorKind::WouldBlock|ErrorKind::NotConnected) =>
                {
                    if written > 0 {
                        break;
                    }
                    return Err(IOError::WouldBlock);
                }
                Err(err) => return Err(err.into()),
            }
        }
        *s = Instant::now();
        Ok(written)
    }
}
