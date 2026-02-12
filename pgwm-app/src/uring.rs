use crate::error::{Error, Result};
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::vec::Vec;
use core::time::Duration;
use rusl::error::Errno;
use rusl::io_uring::{
    io_uring_enter, io_uring_register_buffers, io_uring_register_files, setup_io_uring,
};
use rusl::platform::{
    Fd, IoSliceMut, IoUring, IoUringBorrowedSqe, IoUringEnterFlags, IoUringParamFlags,
    IoUringSQEFlags, IoUringSubmissionQueueEntry, NonNegativeI32,
};
use tiny_std::unix::fd::RawFd;
use xcb_rust_protocol::con::SocketIo;

const SOCK_FD_INDEX: NonNegativeI32 = NonNegativeI32::comptime_checked_new(0);
#[cfg(feature = "status-bar")]
const BAT_FD_INDEX: NonNegativeI32 = NonNegativeI32::comptime_checked_new(1);
#[cfg(feature = "status-bar")]
const NET_FD_INDEX: NonNegativeI32 = NonNegativeI32::comptime_checked_new(2);
#[cfg(feature = "status-bar")]
const MEM_FD_INDEX: NonNegativeI32 = NonNegativeI32::comptime_checked_new(3);
#[cfg(feature = "status-bar")]
const CPU_FD_INDEX: NonNegativeI32 = NonNegativeI32::comptime_checked_new(4);

const SOCK_IN_BUF_INDEX: usize = 0;
const SOCK_OUT_BUF_INDEX: usize = 1;
#[cfg(feature = "status-bar")]
const BAT_BUF_INDEX: usize = 2;
#[cfg(feature = "status-bar")]
const NET_BUF_INDEX: usize = 3;
#[cfg(feature = "status-bar")]
const MEM_BUF_INDEX: usize = 4;
#[cfg(feature = "status-bar")]
const CPU_BUF_INDEX: usize = 5;

const SOCK_READ_USER_DATA: u64 = 0;
const SOCK_WRITE_USER_DATA: u64 = 1;
#[cfg(feature = "status-bar")]
const BAT_READ_USER_DATA: u64 = 2;
#[cfg(feature = "status-bar")]
const BAT_TIMEOUT_USER_DATA: u64 = 3;
#[cfg(feature = "status-bar")]
const NET_READ_USER_DATA: u64 = 4;
#[cfg(feature = "status-bar")]
const NET_TIMEOUT_USER_DATA: u64 = 5;
#[cfg(feature = "status-bar")]
const MEM_READ_USER_DATA: u64 = 6;
#[cfg(feature = "status-bar")]
const MEM_TIMEOUT_USER_DATA: u64 = 7;
#[cfg(feature = "status-bar")]
const CPU_READ_USER_DATA: u64 = 8;
#[cfg(feature = "status-bar")]
const CPU_TIMEOUT_USER_DATA: u64 = 9;
#[cfg(feature = "status-bar")]
const DATE_TIMEOUT_USER_DATA: u64 = 10;

#[cfg(feature = "status-bar")]
const NUM_CHECKS: usize = 6;
#[cfg(not(feature = "status-bar"))]
const NUM_CHECKS: usize = 1;

/// There seems to be a limit at around 300 for the amount of messages you can push through at
/// once over the socket to x11 causing an exhausting bug where some messages are not recorded
/// by the server, causing a sequence mismatch between client and server with no other errors.
/// I'll guess that the limit is 256 and that this capacity should be set conservatively below that.
const URING_CAPACITY: u32 = 128;

/// A write stream buffer shared with the kernel logically consisting of three sections
/// 0 -> `user_provided` -> `kernel_committed` -> end.
/// The first section, 0 -> `kernel_committed` are "already written" or currently writing.
/// This area is subject to data-races, since the kernel may be in the process of reading from it.
/// The second section `kernel_committed` -> `user_provided`, are pending writes, not yet pushed
/// to the kernel, it's safe to edit.
/// The last section, `user_provided` -> end, is available space to write new data.
#[derive(Debug)]
pub struct KernelSharedStreamWriteBuffer {
    bytes: Vec<u8>,
    user_provided: usize,
    kernel_committed: usize,
}

impl KernelSharedStreamWriteBuffer {
    /// The part of the buffer where new data can be added
    #[inline]
    pub fn user_writeable(&mut self) -> &mut [u8] {
        &mut self.bytes[self.user_provided..]
    }

    /// The part of the buffer that hasn't already been passed along to the kernel
    #[inline]
    pub fn kernel_readable(&mut self) -> &mut [u8] {
        &mut self.bytes[self.kernel_committed..self.user_provided]
    }

    #[inline]
    pub fn advance_written(&mut self, bytes: usize) {
        self.user_provided += bytes;
    }

    /// Mark that bytes have been flushed to the kernel
    #[inline]
    pub fn mark_flushed(&mut self) {
        self.kernel_committed = self.user_provided;
    }

    /// Reset offsets, will not manipulate the underlying data
    /// # Safety
    /// If called before waiting until the kernel has processed all data up until `kernel_committed`
    /// inconsistency between user- and kernel-space handling of the buffer will occur.
    /// Will cause UB if the buffer is cleared before kernel processing and new data is written
    /// into the buffer, since data that the kernel may be currently reading will be overwritten.
    #[inline]
    pub unsafe fn clear(&mut self) {
        self.kernel_committed = 0;
        self.user_provided = 0;
    }

    #[inline]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            user_provided: 0,
            kernel_committed: 0,
        }
    }
}

/// A read buffer shared with the kernel consisting logically of three areas:
/// 0 -> `user_consumed`, bytes marked as read by the user.
/// `user_consumed` -> `kernel_committed`, bytes not marked as read, but safe to read.
/// `kernel_committed` -> end, bytes that the kernel may be currently writing to.
/// Inverse to the `KernelSharedStreamWriteBuffer` the last area is subject to data-races with the
/// kernel.
#[derive(Debug)]
pub struct KernelSharedStreamReadBuffer {
    bytes: Vec<u8>,
    user_consumed: usize,
    kernel_committed: usize,
    /// Important in the case of `x11`, we might check the buffer but require more bytes to be read
    /// for a full message. Then we don't care about bytes that aren't committed, we want to wait
    /// until we have more bytes committed than last time.
    has_unchecked_data: bool,
}

impl KernelSharedStreamReadBuffer {
    /// Get the section of the buffer that the kernel has written data into.
    #[inline]
    pub fn user_readable(&self) -> &[u8] {
        &self.bytes[self.user_consumed..self.kernel_committed]
    }

    /// Get the section of the buffer available for the kernel to write into.
    #[inline]
    pub fn kernel_writeable(&mut self) -> &mut [u8] {
        &mut self.bytes[self.kernel_committed..]
    }

    /// Mark number read bytes (from the `user_readable` section).
    #[inline]
    pub fn advance_read(&mut self, bytes: usize) {
        self.user_consumed += bytes;
        self.has_unchecked_data = false;
    }

    /// Mark number of bytes written from the kernel, which are now safe to read.
    #[inline]
    pub unsafe fn advance_written(&mut self, bytes: usize) {
        self.kernel_committed += bytes;
        self.has_unchecked_data = true;
    }

    /// Reset this buffer's offsets, shifting back unread bytes to the beginning of the buffer.
    /// # Safety
    /// As with `KernelSharedStreamWriteBuffer::clear` this is only safe if the kernel is not
    /// currently writing into the buffer, which would cause a data-race.
    #[inline]
    pub unsafe fn clear_read(&mut self) {
        if self.kernel_committed != 0 && self.user_consumed != 0 {
            let rem = self.kernel_committed - self.user_consumed;
            self.bytes
                .copy_within(self.user_consumed..self.kernel_committed, 0);
            self.user_consumed = 0;
            self.kernel_committed = rem;
        }
    }

    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            user_consumed: 0,
            kernel_committed: 0,
            has_unchecked_data: false,
        }
    }
}

pub(crate) struct UringWrapper {
    inner: IoUring,
    pub(crate) counter: UringCounter,
    sock_read_buffer: KernelSharedStreamReadBuffer,
    sock_write_buffer: KernelSharedStreamWriteBuffer,
    #[cfg(feature = "status-bar")]
    bat_buf: Vec<u8>,
    #[cfg(feature = "status-bar")]
    net_buf: Vec<u8>,
    #[cfg(feature = "status-bar")]
    mem_buf: Vec<u8>,
    #[cfg(feature = "status-bar")]
    cpu_buf: Vec<u8>,
    to_submit: u32,
}

#[derive(Debug)]
pub(crate) struct UringCounter {
    pending_sock_writes: usize,
    pub(crate) pending_sock_read: ReadStatus,
    #[cfg(feature = "status-bar")]
    pending_bat_read: ReadStatus,
    #[cfg(feature = "status-bar")]
    pending_net_read: ReadStatus,
    #[cfg(feature = "status-bar")]
    pending_mem_read: ReadStatus,
    #[cfg(feature = "status-bar")]
    pending_cpu_read: ReadStatus,
    #[cfg(feature = "status-bar")]
    pending_date_read: ReadStatus,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum UringReadEvent {
    SockIn,
    #[cfg(feature = "status-bar")]
    Bat,
    #[cfg(feature = "status-bar")]
    Net,
    #[cfg(feature = "status-bar")]
    Mem,
    #[cfg(feature = "status-bar")]
    Cpu,
    #[cfg(feature = "status-bar")]
    DateTimeout,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ReadStatus {
    Inactive,
    Pending,
    #[cfg(feature = "status-bar")]
    Ready(usize),
}

macro_rules! impl_submit_check {
    ($fn_name: ident, $counter_name: ident, $buf: ident, $user_data: expr, $timeout_user_data: expr, $fd_index: expr, $buf_index: expr) => {
        #[inline]
        #[cfg(feature = "status-bar")]
        pub fn $fn_name(&mut self, execute_at: &tiny_std::time::Instant) -> Result<()> {
            if self.counter.$counter_name != ReadStatus::Inactive {
                crate::debug!(
                    "Tried to submit multiple reads for {}, status: {:?}",
                    stringify!($fn_name),
                    self.counter.$counter_name
                );
            } else if *execute_at >= tiny_std::time::Instant::now() {
                self.submit_indexed_timeout($timeout_user_data, execute_at);
                self.counter.$counter_name = ReadStatus::Pending;
            } else {
                let addr = self.$buf.as_ptr() as u64;
                let space = self.$buf.len();
                self.submit_indexed_read($fd_index, $buf_index, $user_data, addr, space);
            }
            Ok(())
        }
    };
}

macro_rules! impl_read_check {
    ($fn_name: ident, $counter_name: ident, $buf: ident) => {
        #[inline]
        #[cfg(feature = "status-bar")]
        pub fn $fn_name(&mut self) -> Option<&[u8]> {
            match self.counter.$counter_name {
                ReadStatus::Ready(ind) => {
                    self.counter.$counter_name = ReadStatus::Inactive;
                    Some(&self.$buf[..ind])
                }
                _ => None,
            }
        }
    };
}

impl UringWrapper {
    /// Async submit a write by writing a new SQE into the kernel shared memory surface.
    /// No IO overhead if using `SQPoll`, but does include a `release` ordered memory write
    pub fn submit_socket_write<E, F: FnOnce(&mut [u8]) -> core::result::Result<usize, E>>(
        &mut self,
        write_op: F,
    ) -> Result<Option<E>> {
        // Check for a slot before starting to commit to the buffer, otherwise there may be bugs
        // where bytes flushed desyncs from sqe/cqe count and other bookkeeping.
        let slot = if let Some(slot) = self.inner.get_next_sqe_slot() {
            slot
        } else {
            let loop_count = 0;
            let start = tiny_std::time::Instant::now();
            loop {
                if loop_count > 0 {
                    tiny_std::eprintln!("[WARN] Failed to get next SQE slot for socket write, attempting to flush buffer count={loop_count}, elapsed={:.2} seconds", start.elapsed().unwrap_or_default().as_secs_f32());
                }
                self.await_write_completions()?;
                let Some(slot) = self.inner.get_next_sqe_slot() else {
                    tiny_std::thread::sleep(core::time::Duration::from_millis(10)).unwrap();
                    continue;
                };
                if loop_count > 0 {
                    tiny_std::eprintln!("[INFO] Successfully got next SQE slot for socket write after flushing count={loop_count}, elapsed={:.2} seconds", start.elapsed().unwrap_or_default().as_secs_f32());
                }
                break slot;
            }
        };
        // A slot is acquired, now the buffer can be written to
        let write_result = (write_op)(self.sock_write_buffer.user_writeable());
        let consumed_bytes = match write_result {
            Ok(b) => b,
            Err(e) => return Ok(Some(e)),
        };
        self.sock_write_buffer.advance_written(consumed_bytes);
        let to_write = self.sock_write_buffer.kernel_readable().len();
        if to_write == 0 {
            return Err(Error::Uring(
                "[BUG] Attempted to submit an empty socket write".to_owned(),
            ));
        }
        // It's possible that we want to block for prev writes to finish before issuing another,
        // even though we're writing from different offsets of the buffer, depending on which execution
        // order we get we might get out of order message writes.
        // I haven't noticed that as a problem, and it introduces both latency and throughput
        // costs if we have to block here.
        let addr = self.sock_write_buffer.kernel_readable().as_mut_ptr();
        let entry = unsafe {
            IoUringSubmissionQueueEntry::new_writev_fixed(
                // Same file index for both sockets
                SOCK_FD_INDEX,
                SOCK_OUT_BUF_INDEX as u16,
                addr as u64,
                to_write as u32,
                SOCK_WRITE_USER_DATA,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            )
        };
        slot.write(entry);
        self.sock_write_buffer.mark_flushed();
        self.counter.pending_sock_writes += 1;
        self.finish_submit();
        Ok(None)
    }

    /// Same as `submit_socket_write` but a read operation
    pub fn submit_sock_read(&mut self) -> Result<()> {
        if self.counter.pending_sock_read != ReadStatus::Inactive {
            return Err(Error::Uring(format!(
                "Tried to submit multiple sock reads, status: {:?}",
                self.counter.pending_sock_read
            )));
        }
        let addr = self.sock_read_buffer.kernel_writeable().as_mut_ptr();
        let space = self.sock_read_buffer.kernel_writeable().len();
        unsafe {
            let entry = IoUringSubmissionQueueEntry::new_readv_fixed(
                SOCK_FD_INDEX as Fd,
                SOCK_IN_BUF_INDEX as u16,
                addr as u64,
                space as u32,
                SOCK_READ_USER_DATA,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            );
            self.await_and_use_next_sqe_slot("submit sock read", |sqe| sqe.write(entry));
        };
        self.counter.pending_sock_read = ReadStatus::Pending;
        self.finish_submit();
        Ok(())
    }

    impl_submit_check!(
        submit_bat_read,
        pending_bat_read,
        bat_buf,
        BAT_READ_USER_DATA,
        BAT_TIMEOUT_USER_DATA,
        BAT_FD_INDEX,
        BAT_BUF_INDEX
    );
    impl_submit_check!(
        submit_net_read,
        pending_net_read,
        net_buf,
        NET_READ_USER_DATA,
        NET_TIMEOUT_USER_DATA,
        NET_FD_INDEX,
        NET_BUF_INDEX
    );
    impl_submit_check!(
        submit_cpu_read,
        pending_cpu_read,
        cpu_buf,
        CPU_READ_USER_DATA,
        CPU_TIMEOUT_USER_DATA,
        CPU_FD_INDEX,
        CPU_BUF_INDEX
    );
    impl_submit_check!(
        submit_mem_read,
        pending_mem_read,
        mem_buf,
        MEM_READ_USER_DATA,
        MEM_TIMEOUT_USER_DATA,
        MEM_FD_INDEX,
        MEM_BUF_INDEX
    );

    #[inline]
    #[cfg(feature = "status-bar")]
    fn submit_indexed_timeout(
        &mut self,
        timeout_user_data: u64,
        execute_at: &tiny_std::time::Instant,
    ) {
        unsafe {
            let timeout = IoUringSubmissionQueueEntry::new_timeout(
                execute_at.as_ref(),
                false,
                None,
                timeout_user_data,
                IoUringSQEFlags::empty(),
            );
            self.await_and_use_next_sqe_slot("submit indexed timeout", |sqe| sqe.write(timeout));
        }
        self.finish_submit();
    }

    #[inline]
    #[cfg(feature = "status-bar")]
    fn submit_indexed_read(
        &mut self,
        fd_ind: NonNegativeI32,
        buf_ind: usize,
        user_data: u64,
        addr: u64,
        space: usize,
    ) {
        unsafe {
            let entry = IoUringSubmissionQueueEntry::new_readv_fixed(
                fd_ind,
                buf_ind as u16,
                addr,
                space as u32,
                user_data,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            );
            self.await_and_use_next_sqe_slot("submit indexed read", |sqe| sqe.write(entry));
        };
        self.finish_submit();
    }

    #[inline]
    #[cfg(feature = "status-bar")]
    pub fn submit_date_timeout(&mut self, execute_at: &tiny_std::time::Instant) {
        if self.counter.pending_date_read != ReadStatus::Inactive {
            crate::debug!(
                "Tried to submit multiple date timeouts, status: {:?}",
                self.counter.pending_date_read
            );
        } else if *execute_at >= tiny_std::time::Instant::now() {
            unsafe {
                let entry = IoUringSubmissionQueueEntry::new_timeout(
                    execute_at.as_ref(),
                    false,
                    None,
                    DATE_TIMEOUT_USER_DATA,
                    IoUringSQEFlags::empty(),
                );
                self.await_and_use_next_sqe_slot("submit date", |sqe| sqe.write(entry));
            };
            self.counter.pending_date_read = ReadStatus::Pending;
            self.finish_submit();
        } else {
            self.counter.pending_date_read = ReadStatus::Ready(0);
        }
    }

    #[inline]
    fn finish_submit(&mut self) {
        // Flush queue, could optimize this a bit on the tiny-std side
        // with something like `flush_new` but that has some negatives if we've added
        // more than one submission
        self.inner.flush_submission_queue();
        self.to_submit += 1;
    }

    fn enter_until_not_interrupted(
        &mut self,
        min_complete: u32,
        flags: IoUringEnterFlags,
    ) -> Result<()> {
        loop {
            match io_uring_enter(self.inner.fd, self.to_submit, min_complete, flags) {
                Ok(submitted) => {
                    self.to_submit = self.to_submit.saturating_sub(submitted as u32);
                    if self.to_submit == 0 {
                        return Ok(());
                    }
                }
                Err(e) if e.code == Some(Errno::EINTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    impl_read_check!(read_bat, pending_bat_read, bat_buf);
    impl_read_check!(read_net, pending_net_read, net_buf);
    impl_read_check!(read_mem, pending_mem_read, mem_buf);
    impl_read_check!(read_cpu, pending_cpu_read, cpu_buf);

    #[inline]
    #[cfg(feature = "status-bar")]
    pub fn read_date(&mut self) {
        match self.counter.pending_date_read {
            ReadStatus::Ready(_ind) => {
                self.counter.pending_date_read = ReadStatus::Inactive;
            }
            _ => panic!("Date not ready on read."),
        }
    }

    pub(crate) fn check_ready_cached(
        &mut self,
    ) -> heapless::Vec<UringReadEvent, { NUM_CHECKS + 1 }> {
        let mut ready = heapless::Vec::new();
        #[cfg(feature = "status-bar")]
        {
            if matches!(self.counter.pending_bat_read, ReadStatus::Ready(_)) {
                let _ = ready.push(UringReadEvent::Bat);
            }
            if matches!(self.counter.pending_net_read, ReadStatus::Ready(_)) {
                let _ = ready.push(UringReadEvent::Net);
            }
            if matches!(self.counter.pending_mem_read, ReadStatus::Ready(_)) {
                let _ = ready.push(UringReadEvent::Mem);
            }
            if matches!(self.counter.pending_cpu_read, ReadStatus::Ready(_)) {
                let _ = ready.push(UringReadEvent::Cpu);
            }
            if matches!(self.counter.pending_date_read, ReadStatus::Ready(_)) {
                let _ = ready.push(UringReadEvent::DateTimeout);
            }
        }
        if self.sock_read_buffer.has_unchecked_data {
            let _ = ready.push(UringReadEvent::SockIn);
        }
        ready
    }

    pub(crate) fn handle_next_completion(&mut self) -> Result<Option<UringReadEvent>> {
        while let Some(cqe) = self.inner.get_next_cqe() {
            match cqe.0.user_data {
                SOCK_READ_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    unsafe {
                        self.sock_read_buffer.advance_written(cqe.0.res as usize);
                        self.sock_read_buffer.clear_read();
                    }
                    self.counter.pending_sock_read = ReadStatus::Inactive;
                    self.submit_sock_read()?;
                    return Ok(Some(UringReadEvent::SockIn));
                }
                SOCK_WRITE_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    self.counter.pending_sock_writes -= 1;
                }
                #[cfg(feature = "status-bar")]
                BAT_READ_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    self.counter.pending_bat_read = ReadStatus::Ready(cqe.0.res as usize);
                    return Ok(Some(UringReadEvent::Bat));
                }
                #[cfg(feature = "status-bar")]
                BAT_TIMEOUT_USER_DATA => {
                    let addr = self.bat_buf.as_ptr() as u64;
                    let space = self.bat_buf.len();
                    self.submit_indexed_read(
                        BAT_FD_INDEX,
                        BAT_BUF_INDEX,
                        BAT_READ_USER_DATA,
                        addr,
                        space,
                    );
                }
                #[cfg(feature = "status-bar")]
                NET_READ_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    self.counter.pending_net_read = ReadStatus::Ready(cqe.0.res as usize);
                    return Ok(Some(UringReadEvent::Net));
                }
                #[cfg(feature = "status-bar")]
                NET_TIMEOUT_USER_DATA => {
                    let addr = self.net_buf.as_ptr() as u64;
                    let space = self.net_buf.len();
                    self.submit_indexed_read(
                        NET_FD_INDEX,
                        NET_BUF_INDEX,
                        NET_READ_USER_DATA,
                        addr,
                        space,
                    );
                }
                #[cfg(feature = "status-bar")]
                MEM_READ_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    self.counter.pending_mem_read = ReadStatus::Ready(cqe.0.res as usize);
                    return Ok(Some(UringReadEvent::Mem));
                }
                #[cfg(feature = "status-bar")]
                MEM_TIMEOUT_USER_DATA => {
                    let addr = self.mem_buf.as_ptr() as u64;
                    let space = self.mem_buf.len();
                    self.submit_indexed_read(
                        MEM_FD_INDEX,
                        MEM_BUF_INDEX,
                        MEM_READ_USER_DATA,
                        addr,
                        space,
                    );
                }
                #[cfg(feature = "status-bar")]
                CPU_READ_USER_DATA => {
                    if cqe.0.res < 0 {
                        return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                    }
                    self.counter.pending_cpu_read = ReadStatus::Ready(cqe.0.res as usize);
                    return Ok(Some(UringReadEvent::Cpu));
                }
                #[cfg(feature = "status-bar")]
                CPU_TIMEOUT_USER_DATA => {
                    let addr = self.cpu_buf.as_ptr() as u64;
                    let space = self.cpu_buf.len();
                    self.submit_indexed_read(
                        CPU_FD_INDEX,
                        CPU_BUF_INDEX,
                        CPU_READ_USER_DATA,
                        addr,
                        space,
                    );
                }
                #[cfg(feature = "status-bar")]
                DATE_TIMEOUT_USER_DATA => {
                    self.counter.pending_date_read = ReadStatus::Ready(0);
                    return Ok(Some(UringReadEvent::DateTimeout));
                }
                _ => {
                    panic!("Io uring in inconsistent state");
                }
            }
        }
        Ok(None)
    }

    pub(crate) fn await_next_completion(&mut self) -> Result<UringReadEvent> {
        loop {
            if let Some(next) = self.handle_next_completion()? {
                return Ok(next);
            }
            self.enter_until_not_interrupted(1, IoUringEnterFlags::IORING_ENTER_GETEVENTS)?;
        }
    }

    pub fn await_write_completions(&mut self) -> Result<()> {
        if self.counter.pending_sock_writes == 0 {
            unsafe {
                self.sock_write_buffer.clear();
            }
            return Ok(());
        }
        loop {
            while self.handle_next_completion()?.is_some() {}
            if self.counter.pending_sock_writes == 0 {
                unsafe {
                    self.sock_write_buffer.clear();
                }
                return Ok(());
            }
            self.enter_until_not_interrupted(
                self.counter.pending_sock_writes as u32,
                IoUringEnterFlags::IORING_ENTER_GETEVENTS,
            )?;
        }
    }

    fn await_and_use_next_sqe_slot<F: FnOnce(IoUringBorrowedSqe)>(
        &mut self,
        label: &'static str,
        func: F,
    ) {
        let start = tiny_std::time::Instant::now();
        let mut loop_count = 0;
        loop {
            if let Some(sqe) = self.inner.get_next_sqe_slot() {
                if loop_count > 0 {
                    tiny_std::eprintln!(
                        "[{label}] awaiting sqe slot took {loop_count} loops and {:.2} seconds",
                        start.elapsed().unwrap_or_default().as_secs_f32()
                    );
                }
                func(sqe);
                return;
            }
            if loop_count == 0 && self.counter.pending_sock_writes > 0 {
                self.await_write_completions().unwrap();
            }
            loop_count += 1;
            let _ = tiny_std::thread::sleep(Duration::from_millis(10));
            if loop_count % 100 == 0 {
                tiny_std::eprintln!("[{label}] has awaited sqe slot in {loop_count} loops and {:.2} seconds", start.elapsed().unwrap_or_default().as_secs_f32());
            }
        }
    }

    pub fn new(
        mut read_buf: Vec<u8>,
        mut write_buf: Vec<u8>,
        xcb_sock_fd: RawFd,
        #[cfg(feature = "status-bar")] mut bat_buf: Vec<u8>,
        #[cfg(feature = "status-bar")] mut net_buf: Vec<u8>,
        #[cfg(feature = "status-bar")] mut mem_buf: Vec<u8>,
        #[cfg(feature = "status-bar")] mut cpu_buf: Vec<u8>,
        #[cfg(feature = "status-bar")] bat_fd: RawFd,
        #[cfg(feature = "status-bar")] net_fd: RawFd,
        #[cfg(feature = "status-bar")] mem_fd: RawFd,
        #[cfg(feature = "status-bar")] cpu_fd: RawFd,
    ) -> Result<Self> {
        let inner = setup_io_uring(
            URING_CAPACITY,
            IoUringParamFlags::IORING_SETUP_SINGLE_ISSUER,
            0,
            0,
        )?;
        unsafe {
            io_uring_register_buffers(
                inner.fd,
                &[
                    IoSliceMut::new(&mut read_buf),
                    IoSliceMut::new(&mut write_buf),
                    #[cfg(feature = "status-bar")]
                    IoSliceMut::new(&mut bat_buf),
                    #[cfg(feature = "status-bar")]
                    IoSliceMut::new(&mut net_buf),
                    #[cfg(feature = "status-bar")]
                    IoSliceMut::new(&mut mem_buf),
                    #[cfg(feature = "status-bar")]
                    IoSliceMut::new(&mut cpu_buf),
                ],
            )?;
        }
        io_uring_register_files(
            inner.fd,
            &[
                xcb_sock_fd,
                #[cfg(feature = "status-bar")]
                bat_fd,
                #[cfg(feature = "status-bar")]
                net_fd,
                #[cfg(feature = "status-bar")]
                mem_fd,
                #[cfg(feature = "status-bar")]
                cpu_fd,
            ],
        )?;
        Ok(Self {
            inner,
            counter: UringCounter {
                pending_sock_writes: 0,
                pending_sock_read: ReadStatus::Inactive,
                #[cfg(feature = "status-bar")]
                pending_bat_read: ReadStatus::Inactive,
                #[cfg(feature = "status-bar")]
                pending_net_read: ReadStatus::Inactive,
                #[cfg(feature = "status-bar")]
                pending_mem_read: ReadStatus::Inactive,
                #[cfg(feature = "status-bar")]
                pending_cpu_read: ReadStatus::Inactive,
                #[cfg(feature = "status-bar")]
                pending_date_read: ReadStatus::Inactive,
            },
            sock_read_buffer: KernelSharedStreamReadBuffer::new(read_buf),
            sock_write_buffer: KernelSharedStreamWriteBuffer::new(write_buf),
            #[cfg(feature = "status-bar")]
            bat_buf,
            #[cfg(feature = "status-bar")]
            net_buf,
            #[cfg(feature = "status-bar")]
            mem_buf,
            #[cfg(feature = "status-bar")]
            cpu_buf,
            to_submit: 0,
        })
    }
}

impl SocketIo for UringWrapper {
    fn block_for_more_data(&mut self) -> core::result::Result<(), &'static str> {
        if self.sock_read_buffer.has_unchecked_data {
            return Ok(());
        }
        loop {
            #[allow(unused_variables)]
            let evt = self.await_next_completion().map_err(|e| {
                crate::debug!("Got error waiting for next read {e}");
                "Got error waiting for more data to be read"
            })?;
            if evt == UringReadEvent::SockIn {
                return Ok(());
            }
        }
    }

    #[inline]
    fn use_read_buffer<
        F: FnOnce(&[u8]) -> core::result::Result<usize, xcb_rust_protocol::Error>,
    >(
        &mut self,
        read_op: F,
    ) -> core::result::Result<(), xcb_rust_protocol::Error> {
        let consumed_bytes = (read_op)(self.sock_read_buffer.user_readable())?;
        self.sock_read_buffer.advance_read(consumed_bytes);
        Ok(())
    }

    #[inline]
    fn use_write_buffer<
        F: FnOnce(&mut [u8]) -> core::result::Result<usize, xcb_rust_protocol::Error>,
    >(
        &mut self,
        write_op: F,
    ) -> core::result::Result<(), xcb_rust_protocol::Error> {
        // This control flow is absurd, Ok(Some(e)) is an error, but whatever
        match self.submit_socket_write(write_op) {
            Ok(None) => Ok(()),
            Ok(Some(err)) => Err(err),
            Err(e) => {
                tiny_std::eprintln!("failed to submit socket write: {e:?}");
                Err(xcb_rust_protocol::Error::User(
                    "failed to submit socket write when using write buffer",
                ))
            }
        }
    }

    #[inline]
    fn ensure_flushed(&mut self) -> core::result::Result<(), xcb_rust_protocol::Error> {
        self.await_write_completions().map_err(|e| {
            tiny_std::eprintln!("failed to await writes to socket ensuring flushed: {e:?}");
            xcb_rust_protocol::Error::Connection("failed to await writes to socket")
        })
    }
}

impl Drop for UringWrapper {
    fn drop(&mut self) {
        self.enter_until_not_interrupted(
            0,
            IoUringEnterFlags::IORING_ENTER_SQ_WAKEUP | IoUringEnterFlags::IORING_ENTER_SQ_WAIT,
        )
        .unwrap();
    }
}
