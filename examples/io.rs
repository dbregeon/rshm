use std::{
    io::{Read, Write},
    mem::size_of,
};

use rshm::shm::{OwnedShmMap, ShmMap};

/// An ShmReader reads bytes from a shared memory buffer.
/// There is no notification or wake-up mechanism built-in. This would have to be built
/// separately. See the [LogConsumer] and [LogProducer] examples for a possible implementation.
pub struct ShmReader {
    _shm: ShmMap,
    written_bytes_ptr: *const u8,
    last_read_ptr: *const u8,
    read: usize,
}

impl ShmReader {
    pub fn new(shm: ShmMap) -> Self {
        // We keep the number of written bytes of the beginning
        let written_bytes_ptr = shm.head();
        let last_read_ptr = unsafe { written_bytes_ptr.add(1) };
        Self {
            _shm: shm,
            written_bytes_ptr: written_bytes_ptr,
            last_read_ptr: last_read_ptr,
            read: 0,
        }
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
    pub fn new(shm: OwnedShmMap) -> Self {
        let available = shm.definition.size - size_of::<u8>();

        // We keep the number of written bytes of the beginning
        let written_bytes_ptr = shm.head() as *mut u8;
        let end_ptr = unsafe { written_bytes_ptr.add(1) } as *mut u8;
        unsafe { *written_bytes_ptr = 0 };
        Self {
            _shm: shm,
            written_bytes_ptr,
            end_ptr,
            available,
        }
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use crate::{ShmReader, ShmWriter};
    use rshm::shm::ShmDefinition;

    #[test]
    fn reader_reads_what_writer_wrote() {
        let writer_definition = ShmDefinition {
            path: "test_writer".to_string(),
            size: 10,
        };
        let writer_shm = writer_definition.create().unwrap();
        let mut writer = ShmWriter::new(writer_shm);

        writer.write("test1".as_bytes()).unwrap();
        writer.flush().unwrap();

        let reader_definition = ShmDefinition {
            path: "test_writer".to_string(),
            size: 10,
        };
        let reader_shm = reader_definition.open().unwrap();
        let mut reader = ShmReader::new(reader_shm);
        let mut reader_buffer = vec![0 as u8; 1024];
        let count = reader.read(&mut reader_buffer).unwrap();

        assert_eq!(
            format!("{}", std::str::from_utf8(&reader_buffer[0..count]).unwrap()),
            "test1"
        )
    }
}
