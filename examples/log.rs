/// This example shows how the rshm library can be used to create a log with a single producer
/// and multiple consumers, using shared condvars to notify consumers of a new record in the log.
///
use std::mem::size_of;

use rshm::{
    condvar::Condvar,
    shm::{OwnedShmMap, ShmMap},
};

/// A LogConsumer reads records from the log as they become available,
/// as signalled by a LogProducer through a condvar.
/// ```
/// use rand::Rng;
/// use rshm::shm::ShmDefinition;
///
/// use crate::{LogConsumer, LogProducer};
///
/// #[test]
/// fn log_consumer_reads_record_added_by_log_producer() {
///     let definition_producer = ShmDefinition {
///     path: "test".to_string(),
///         size: 1024,
///     };
///     let producer_shm = definition_producer.create().unwrap();
///     let mut producer = LogProducer::new(producer_shm);
///     // Wait so that the consumer has time to start before we drop the shared memory.
///     let consumer = std::thread::spawn(|| {
///         // Wait so that the producer has time to start before e read the shared memory.
///         let definition_consumer = ShmDefinition {
///             path: "test".to_string(),
///             size: 1024,
///        };
///         let consumer_shm = definition_consumer.open().unwrap();
///         let mut consumer = LogConsumer::new(consumer_shm);
///         consumer.next().unwrap()
///     });
///     let record = rand::thread_rng().gen::<u64>();
///     producer.insert(record).unwrap();
///     let consumer_read = consumer.join().unwrap();
///     assert_eq!(record, consumer_read);
/// }
/// ```
pub struct LogConsumer<E: Copy> {
    _map: ShmMap,
    condvar: *const Condvar,
    sequence_number: *const u64,
    end_ptr: *const E,
    next_sequence: u64,
}

impl<E: Copy> LogConsumer<E> {
    /// Creates a new LogConsumer from the given [rshm::shm::ShmMap].
    /// The memory block is expected to contain:
    /// * a [rshm::condvar::Condvar] used to wait for available records
    /// * a [u64] sequence number indicating the last record's index
    /// * aligned records in sequence order
    pub fn new(map: ShmMap) -> Self {
        let condvar_ptr = map.head() as *const Condvar;
        // We keep the number of written bytes of the beginning
        let sequence_number = unsafe { (condvar_ptr.add(1)) as *const u64 };
        let size_of_e = size_of::<E>();
        let alignment_offset = (size_of::<Condvar>() + size_of::<u64>()) / size_of_e + 1;
        // Ensure Alignment
        let end_ptr = unsafe { (map.head() as *const E).add(alignment_offset) };
        Self {
            _map: map,
            condvar: condvar_ptr,
            sequence_number,
            end_ptr,
            next_sequence: 1,
        }
    }

    /// Returns the next available record from the log.
    /// This method will block and wait on the log's [rshm::condvar::Condvar].
    ///
    /// It will return
    /// * Some(record) when a record was read
    /// * None when the wait is interrupted or when the condition changes but no
    /// new records are available (the current sequence in shared memory is still lower than the next
    /// sequence we expect to read)
    pub fn next(&mut self) -> Option<E> {
        let current_sequence = unsafe { self.sequence_number.read_volatile() };
        if current_sequence < self.next_sequence {
            match unsafe { (*self.condvar).wait() } {
                Err(_) => return None,
                _ => {}
            }
        }
        if current_sequence >= self.next_sequence {
            let record = unsafe { self.end_ptr.read_volatile() };
            self.next_sequence += 1;

            unsafe {
                self.end_ptr = self.end_ptr.add(1);
            }
            Some(record)
        } else {
            None
        }
    }
}

/// A LogProducer writes records into the log and signals new data is available through a Condvar.
pub struct LogProducer<E: Copy> {
    _map: OwnedShmMap,
    condvar: *const Condvar,
    sequence_number: *mut u64,
    end_ptr: *mut E,
    available: usize,
}

impl<E: Copy> LogProducer<E> {
    /// Creates a new LogProducer using the given [rshm::shm::OwnedShmMap].
    /// The memory block will contain:
    /// * a [rshm::condvar::Condvar] used to signal the availability of records
    /// * a [u64] sequence number indicating the last record's index
    /// * aligned records in sequence order
    pub fn new(map: OwnedShmMap) -> Self {
        let condvar_ptr = map.head() as *const Condvar;
        // We keep the number of written bytes of the beginning
        let sequence_number = unsafe { (condvar_ptr.add(1)) as *mut u64 };
        let size_of_e = size_of::<E>();
        let alignment_offset = (size_of::<Condvar>() + size_of::<u64>()) / size_of_e + 1;
        // Ensure Alignment
        let end_ptr = unsafe { (map.head() as *mut E).add(alignment_offset) };
        let map_size = map.definition.size;
        Self {
            _map: map,
            condvar: condvar_ptr,
            sequence_number,
            end_ptr,
            available: map_size / size_of::<E>() - alignment_offset,
        }
    }

    /// inserts a new record at the end of the log.
    /// The sequence number will be incremented and the condvar will be notified.
    pub fn insert(&mut self, record: E) -> Result<(), ErrorCode> {
        if self.available > 0 {
            let sequence_number = unsafe { self.sequence_number.read_volatile() };
            unsafe {
                self.end_ptr.write(record);
                self.sequence_number.write_volatile(sequence_number + 1);
                self.end_ptr = self.end_ptr.add(1);
            };
            self.available -= 1;
            unsafe { (*self.condvar).notify_all() }
                .map(|_| ())
                .map_err(|_| ErrorCode::NotifyAllFailed)
        } else {
            Err(ErrorCode::NoSpaceLeftInSharedMemory)
        }
    }
}

/// Enumeration of the errors that can occur in this module.
#[derive(Debug)]
pub enum ErrorCode {
    /// The allocated memory for the log has been exhausted and new records cannot be inserted.
    NoSpaceLeftInSharedMemory,
    /// The condvar notification to signal consumers a new record is available failed.
    NotifyAllFailed,
}

#[cfg(test)]
mod tests {}
