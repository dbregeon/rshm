use std::hash::Hash;
use std::{collections::HashMap, mem::size_of};

use nix::errno::Errno;
use nix::Result;
use rshm::shm::{OwnedShmMap, ShmMap};

/// The client of a shared memory dictionary.
pub struct ShmDictionaryClient<K, R: Record<K>> {
    _map: ShmMap,
    written_records_ptr: *const usize,
    end_ptr: *const R,
    next_read: usize,
    index: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, R: Record<K>> ShmDictionaryClient<K, R> {
    pub fn new(map: ShmMap) -> Self {
        // We keep the number of written bytes of the beginning
        let written_records_ptr = map.head() as *const usize;
        // Ensure Alignment
        let end_ptr = unsafe { (map.head() as *const R).add(1) };
        Self {
            _map: map,
            written_records_ptr: written_records_ptr,
            end_ptr: end_ptr,
            next_read: 0,
            index: HashMap::new(),
        }
    }

    pub fn get(&mut self, key: &K) -> Result<R> {
        let records_count = unsafe { self.written_records_ptr.read_volatile() };
        if !self.index.contains_key(key) {
            while self.next_read < records_count {
                let record = unsafe { self.end_ptr.read_volatile() };
                self.index.insert(record.key().clone(), self.next_read);
                self.next_read += 1;

                unsafe {
                    self.end_ptr = self.end_ptr.add(1);
                }
            }
        };
        self.index
            .get(key)
            .ok_or(Errno::ENOKEY)
            .and_then(|i| unsafe { Ok(self.end_ptr.sub(records_count - i).read_volatile()) })
    }
}

/// The owner of a shared memory dictionary
pub struct ShmDictionaryOwner<K, R: Record<K>> {
    _map: OwnedShmMap,
    written_records_ptr: *mut usize,
    end_ptr: *mut R,
    available: usize,
    index: HashMap<K, usize>,
}

impl<K: Eq + Hash, R: Record<K>> ShmDictionaryOwner<K, R> {
    pub fn new(map: OwnedShmMap) -> Self {
        let size = map.definition.size;

        // We keep the number of written bytes of the beginning
        let written_records_ptr = map.head() as *mut usize;
        // Ensure Alignment
        let end_ptr = unsafe { (map.head() as *mut R).add(1) };
        unsafe { *written_records_ptr = 0 };
        Self {
            _map: map,
            written_records_ptr: written_records_ptr,
            end_ptr: end_ptr,
            available: (size.get() - size_of::<u8>()) / size_of::<R>(),
            index: HashMap::new(),
        }
    }

    pub fn put(&mut self, record: R) -> Result<()> {
        if self.available > 0 {
            let key = record.key();
            let written_records = unsafe { self.written_records_ptr.read_volatile() };
            match self.index.get(&key) {
                Some(i) => {
                    unsafe {
                        self.end_ptr
                            .sub(written_records - i + 1)
                            .write_volatile(record)
                    };
                }
                None => {
                    unsafe {
                        self.end_ptr.write(record);
                        self.written_records_ptr.write_volatile(written_records + 1);
                        self.end_ptr = self.end_ptr.add(1);
                        self.index.insert(key, *self.written_records_ptr as usize);
                    };
                    self.available -= 1;
                }
            };
            Ok(())
        } else {
            Err(Errno::ENOMEM)
        }
    }
}

/// Definition of a record's Key.
pub trait Key: Eq + Hash + Clone {}

/// Records stored in the dictionary need a key.
pub trait Record<K>: Copy {
    fn key(&self) -> K;
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use rshm::shm::ShmDefinition;

    use crate::{Record, ShmDictionaryClient, ShmDictionaryOwner};

    #[derive(Clone, Copy)]
    pub struct TestRecord {
        pub value: (i32, i32),
    }

    impl Record<i32> for TestRecord {
        fn key(&self) -> i32 {
            println!("{:?}", self.value);
            self.value.0.clone()
        }
    }

    #[test]
    fn store_insertion_is_read_by_the_client() {
        let owner_definition = ShmDefinition {
            path: "test_store".to_string(),
            size: NonZero::new(1024).expect("1024 is not 0"),
        };
        let owner_shared_memory = owner_definition.create().unwrap();
        let mut owner_store: ShmDictionaryOwner<i32, TestRecord> =
            ShmDictionaryOwner::new(owner_shared_memory);

        let client_definition = ShmDefinition {
            path: "test_store".to_string(),
            size: NonZero::new(1024).expect("1024 is not 0"),
        };
        let client_shared_memory = client_definition.open().unwrap();
        let mut client_store: ShmDictionaryClient<i32, TestRecord> =
            ShmDictionaryClient::new(client_shared_memory);

        owner_store.put(TestRecord { value: (1, 11) }).unwrap();

        assert_eq!(client_store.get(&1).unwrap().value, (1, 11));
    }
}
