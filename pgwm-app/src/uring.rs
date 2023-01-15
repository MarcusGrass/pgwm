use crate::error::{Error, Result};
use alloc::format;
use alloc::string::ToString;
use rusl::io_uring::{
    io_uring_enter, io_uring_register_buffers, io_uring_register_files, setup_io_uring,
};
use rusl::platform::{
    Fd, IoSliceMut, IoUring, IoUringEnterFlags, IoUringParamFlags, IoUringSQEFlags,
    IoUringSubmissionQueueEntry,
};
use tiny_std::unix::fd::RawFd;
use xcb_rust_connection::connection::SocketConnection;
use xcb_rust_protocol::con::XcbBuffers;
use xcb_rust_protocol::XcbConnection;

const SOCK_IN_INDEX: usize = 0;
const SOCK_OUT_INDEX: usize = 1;
const BAT_INDEX: usize = 2;
const NET_INDEX: usize = 3;
const MEM_INDEX: usize = 4;
const CPU_INDEX: usize = 5;

const IN_BUF_SIZE: usize = 65536;
const OUT_BUF_SIZE: usize = 65536;

pub struct UringWrapper<'a> {
    inner: IoUring,
    counter: UringCounter,
    xcb_sock_buffers: XcbBuffers<'a>,
    bat_buf: &'a mut [u8],
    net_buf: &'a mut [u8],
    mem_buf: &'a mut [u8],
    cpu_buf: &'a mut [u8],
}

struct UringCounter {
    pending_sock_writes: usize,
    pending_sock_read: ReadStatus,
    pending_bat_read: ReadStatus,
    pending_net_read: ReadStatus,
    pending_mem_read: ReadStatus,
    pending_cpu_read: ReadStatus,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum UringReadEvent {
    SOCK_IN,
    BAT,
    NET,
    MEM,
    CPU,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ReadStatus {
    Inactive,
    Pending,
    Ready(usize),
}

impl<'a> UringWrapper<'a> {
    #[inline]
    pub fn submit_socket_write(&mut self, con: &mut SocketConnection) {
        let addr = self.xcb_out_buffer().as_ptr() as usize + con.buf.out_offset;
        let to_write = OUT_BUF_SIZE - con.buf.out_offset;
        let entry = unsafe {
            IoUringSubmissionQueueEntry::new_writev_fixed(
                SOCK_OUT_INDEX as i32,
                SOCK_OUT_INDEX as u16,
                addr as u64,
                to_write as u32,
                SOCK_OUT_INDEX as u64,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            )
        };
        unsafe { self.inner.get_next_sqe_slot().unwrap().write(entry) };
        con.advance_writer(to_write);
        self.counter.pending_sock_writes += 1;
        self.finish_submit()?;
    }

    pub fn submit_sock_read(&mut self, con: &mut SocketConnection) -> Result<()> {
        if self.counter.pending_sock_read != ReadStatus::Inactive {
            return Err(crate::error::Error::Uring(
                "Tried to submit multiple sock reads.".to_string(),
            ));
        }
        let addr = self.xcb_sock_in_buf.as_ptr() as usize + con.buf.in_write_offset;
        let space = IN_BUF_SIZE - con.buf.in_write_offset;
        let entry = unsafe {
            IoUringSubmissionQueueEntry::new_readv_fixed(
                SOCK_IN_INDEX as Fd,
                SOCK_IN_INDEX as u16,
                addr as u64,
                space as u32,
                SOCK_IN_INDEX as u64,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            )
        };
        unsafe { self.inner.get_next_sqe_slot().unwrap().write(entry) };
        self.counter.pending_sock_read = ReadStatus::Pending;
        self.finish_submit()?;
        Ok(())
    }

    pub fn submit_bat_read(&mut self) -> Result<()> {
        if self.counter.pending_bat_read != ReadStatus::Inactive {
            return Err(crate::error::Error::Uring(
                "Tried to submit multiple bat reads.".to_string(),
            ));
        }
        let addr = self.bat_buf.as_ptr() as u64;
        let space = self.bat_buf.len();
        self.submit_indexed_read(BAT_INDEX, addr, space)?;
        self.counter.pending_bat_read = ReadStatus::Pending;
        self.finish_submit()?;
        Ok(())
    }

    #[inline]
    fn submit_indexed_read(&mut self, ind: usize, addr: u64, space: usize) -> Result<()> {
        let entry = unsafe {
            IoUringSubmissionQueueEntry::new_readv_fixed(
                ind as Fd,
                ind as u16,
                addr,
                space as u32,
                ind as u64,
                IoUringSQEFlags::IOSQE_FIXED_FILE,
            )
        };
        unsafe { self.inner.get_next_sqe_slot().unwrap().write(entry) };
        Ok(())
    }

    #[inline]
    fn finish_submit(&mut self) -> Result<()> {
        // Flush queue, could optimize this a bit on the tiny-std side
        // with something like `flush_new` but that has some negatives if we've added
        // more than one submission
        self.inner.flush_submission_queue();
        if self.inner.needs_wakeup() {
            // Needs to wakeup the SQ thread if not still awake
            io_uring_enter(
                self.inner.fd,
                0,
                0,
                IoUringEnterFlags::IORING_ENTER_SQ_WAKEUP,
            )?;
        }
        Ok(())
    }

    #[inline]
    pub fn read_bat(&mut self) -> Option<&[u8]> {
        match self.counter.pending_bat_read {
            ReadStatus::Ready(ind) => {
                self.counter.pending_bat_read = ReadStatus::Inactive;
                return Some(&self.bat_buf[..ind]);
            }
            _ => None,
        }
    }

    #[inline]
    fn await_next_completion(&mut self) -> Result<UringReadEvent> {
        loop {
            io_uring_enter(
                self.inner.fd,
                0,
                1,
                IoUringEnterFlags::IORING_ENTER_GETEVENTS
                    | IoUringEnterFlags::IORING_ENTER_SQ_WAKEUP,
            )?;
            while let Some(cqe) = self.inner.get_next_cqe() {
                if cqe.0.res < 0 {
                    return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                }
                match cqe.0.user_data {
                    SOCK_IN_INDEX => {
                        self.counter.pending_sock_read = ReadStatus::Ready(cqe.0.res as usize);
                        return Ok(UringReadEvent::SOCK_IN);
                    }
                    SOCK_OUT_INDEX => {
                        self.counter.pending_sock_writes -= 1;
                        continue;
                    }
                    BAT_INDEX => {
                        self.counter.pending_bat_read = ReadStatus::Ready(cqe.0.res as usize);
                        return Ok(UringReadEvent::BAT);
                    }
                    NET_INDEX => {
                        self.counter.pending_net_read = ReadStatus::Ready(cqe.0.res as usize);
                        return Ok(UringReadEvent::NET);
                    }
                    MEM_INDEX => {
                        self.counter.pending_mem_read = ReadStatus::Ready(cqe.0.res as usize);
                        return Ok(UringReadEvent::MEM);
                    }
                    CPU_INDEX => {
                        self.counter.pending_cpu_read = ReadStatus::Ready(cqe.0.res as usize);
                        return Ok(UringReadEvent::CPU);
                    }
                    _ => {
                        panic!("Io uring in inconsistent state");
                    }
                }
            }
        }
    }

    #[inline]
    pub fn await_write_completions(&mut self) -> Result<()> {
        if self.counter.pending_sock_writes == 0 {
            return Ok(());
        }
        loop {
            io_uring_enter(
                self.inner.fd,
                0,
                self.counter.pending_sock_writes as u32,
                IoUringEnterFlags::IORING_ENTER_GETEVENTS
                    | IoUringEnterFlags::IORING_ENTER_SQ_WAKEUP,
            )?;
            while let Some(cqe) = self.inner.get_next_cqe() {
                if cqe.0.res < 0 {
                    return Err(Error::Uring(format!("Got error on cqe {cqe:?}")));
                }
                match cqe.0.user_data as usize {
                    SOCK_IN_INDEX => {
                        self.counter.pending_sock_read = ReadStatus::Ready(cqe.0.res as usize)
                    }
                    SOCK_OUT_INDEX => self.counter.pending_sock_writes -= 1,
                    BAT_INDEX => {
                        self.counter.pending_bat_read = ReadStatus::Ready(cqe.0.res as usize)
                    }
                    NET_INDEX => {
                        self.counter.pending_net_read = ReadStatus::Ready(cqe.0.res as usize)
                    }
                    MEM_INDEX => {
                        self.counter.pending_mem_read = ReadStatus::Ready(cqe.0.res as usize)
                    }
                    CPU_INDEX => {
                        self.counter.pending_cpu_read = ReadStatus::Ready(cqe.0.res as usize)
                    }
                    _ => panic!("Uring inconsistent state got cqe {cqe:?}"),
                }
            }
            if self.counter.pending_sock_writes == 0 {
                return Ok(());
            }
        }
    }

    #[inline]
    pub fn xcb_buffers_mut(&mut self) -> &mut XcbBuffers {
        &mut self.xcb_sock_buffers
    }

    #[inline]
    pub fn xcb_out_buffer(&mut self) -> &mut [u8] {
        &mut self.xcb_sock_buffers.out_buffer
    }

    #[inline]
    pub fn xcb_in_buffer(&mut self) -> &mut [u8] {
        &mut self.xcb_sock_buffers.in_buffer
    }

    pub fn new(
        xcb_buffers: XcbBuffers,
        bat_buf: &'a mut [u8],
        net_buf: &'a mut [u8],
        mem_buf: &'a mut [u8],
        cpu_buf: &'a mut [u8],
        xcb_sock_fd: RawFd,
        bat_fd: RawFd,
        net_fd: RawFd,
        mem_fd: RawFd,
        cpu_fd: RawFd,
    ) -> Result<Self> {
        let mut inner = setup_io_uring(
            128,
            IoUringParamFlags::IORING_SETUP_SINGLE_ISSUER
                | IoUringParamFlags::IORING_SETUP_COOP_TASKRUN,
        )?;
        unsafe {
            io_uring_register_buffers(
                inner.fd,
                &[
                    IoSliceMut::new(xcb_buffers.in_buffer),
                    IoSliceMut::new(xcb_buffers.out_buffer),
                    IoSliceMut::new(bat_buf),
                    IoSliceMut::new(net_buf),
                    IoSliceMut::new(mem_buf),
                    IoSliceMut::new(cpu_buf),
                ],
            )?;
        }
        io_uring_register_files(inner.fd, &[xcb_sock_fd, bat_fd, net_fd, mem_fd, cpu_fd])?;
        Ok(Self {
            inner,
            counter: UringCounter {
                pending_sock_writes: 0,
                pending_sock_read: ReadStatus::Inactive,
                pending_bat_read: ReadStatus::Inactive,
                pending_net_read: ReadStatus::Inactive,
                pending_mem_read: ReadStatus::Inactive,
                pending_cpu_read: ReadStatus::Inactive,
            },
            xcb_sock_buffers: xcb_buffers,
            bat_buf,
            net_buf,
            mem_buf,
            cpu_buf,
        })
    }
}
