# RSHM
A Linux Shared Memory crate in Rust.

## Goal

The goal of this crate is to make it easier to use shm from rust in Linux. It 
provides basic functions to allocate or open a shared memory space. It also
provides a condvar implementation based on shared linux futexes.

## Future

It would be nice to support linux' hugepages as well as standard shm.

It would be nice to refine the examples to make that functionality available in
the library (e.g. gracefully wait for shared memory to be created by its owner, 
support expansion and overflow to file)

## Contributing

Please do raise issues and merge requests if you see a missing features or find a bug.
