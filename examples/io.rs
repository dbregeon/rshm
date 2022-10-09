use std::{
    io::{Read, Write},
    mem::size_of,
};

use rshm::shm::{OwnedShmMap, ShmMap};

pub struct ShmReader {
    _shm: ShmMap,
    written_bytes_ptr: *const u8,
    last_read_ptr: *const u8,
    read: usize,
}

pub enum ErrorCode {}

impl ShmReader {
    pub fn new(shm: ShmMap) -> Result<Self, ErrorCode> {
        // We keep the number of written bytes of the beginning
        let written_bytes_ptr = shm.head();
        let last_read_ptr = unsafe { written_bytes_ptr.add(1) };
        Ok(Self {
            _shm: shm,
            written_bytes_ptr: written_bytes_ptr,
            last_read_ptr: last_read_ptr,
            read: 0,
        })
    }
}

impl Read for ShmReader {
    fn read(&mut self, out: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        let readable_size = out
            .len()
            .min(unsafe { *self.written_bytes_ptr as usize } - self.read);
        if readable_size > 0 {
            unsafe {
                self.last_read_ptr.copy_to(out.as_mut_ptr(), readable_size);
                self.last_read_ptr = self.last_read_ptr.add(readable_size);
            }
            self.read = self.read + readable_size;
            Ok(readable_size)
        } else {
            Ok(0)
        }
    }
}

pub struct ShmWriter {
    _shm: OwnedShmMap,
    written_bytes_ptr: *mut u8,
    end_ptr: *mut u8,
    available: usize,
}

impl ShmWriter {
    pub fn new(shm: OwnedShmMap) -> Result<Self, ErrorCode> {
        let available = shm.definition.size - size_of::<u8>();

        // We keep the number of written bytes of the beginning
        let written_bytes_ptr = shm.head() as *mut u8;
        let end_ptr = unsafe { written_bytes_ptr.add(1) } as *mut u8;
        unsafe { *written_bytes_ptr = 0 };
        Ok(Self {
            _shm: shm,
            written_bytes_ptr,
            end_ptr,
            available,
        })
    }
}

impl Write for ShmWriter {
    fn write(&mut self, value: &[u8]) -> std::result::Result<usize, std::io::Error> {
        let writable_size = self.available.min(value.len());
        if writable_size > 0 {
            unsafe {
                self.end_ptr.copy_from(value.as_ptr(), writable_size);
                self.end_ptr = self.end_ptr.add(writable_size);
                *self.written_bytes_ptr += writable_size as u8;
            }
            self.available = self.available - writable_size;
            Ok(writable_size)
        } else {
            Ok(0)
        }
    }

    fn flush(&mut self) -> std::result::Result<(), std::io::Error> {
        Ok(())
    }
}
