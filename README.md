# A try at writing Bosch MCAN Rust driver

Feature status, plans and known bugs: see [FEATURES.md](FEATURES.md) (source of truth).
Contributor / agent instructions: see [AGENTS.md](AGENTS.md).

# Goals

* Stand-alone, no external pac/hal dependencies (built-in RCC handling is optional, so it can also sit under a HAL)
* Support for STM32 G0, G4, H5, H7, L5 (should be possible to support others as well)
* No panics or asserts, always return Result if something goes wrong
* No blocking waits without timeout
* Optional async support (embassy or RTIC?): interrupt handling, bus off management task, async tx/rx
* Optional sync mode with channels?
* Use stm32-data generated register abstraction layer
* Driver logic in plain Rust, no HAL-style giant `macro_rules!` (macros only where they remove real repetition)
* Reduce number of generics, for example, FdCan is not generic over CAN peripheral instance
* Support raw timestamping in both Classical and FD modes, let users handle time conversions
* RAM layout configuration builder (similar to how usbd builds descriptors)
    * Possibility to change layout, for each instance individually or recombine layouts into one and start over.
* Dynamic reconfiguration
* TX completion tracking (i.e., to support backpressure)
* Time that frame spends in transmit queue before actual transmission (if requested to measure)
* Dedicated TX slots and RX buffers, RX FIFO watermarking
* Manual timings control + predefined common configurations as consts
* Time-Triggered mode (TTCAN), sleep and wakeup
* Configure all bells and whistles (DAR, transmit pause, sampling point debug, etc)
* Tested: 100% coverage goal, host unit tests and hardware-in-the-loop tests

# Based on

* [fdcan](https://github.com/stm32-rs/fdcan)
* [embassy fdcan](https://github.com/embassy-rs/embassy/tree/main/embassy-stm32/src/can/fd)
* [hansihe embassy fork](https://github.com/hansihe/embassy/tree/fdcan-rewrite/embassy-stm32/src/can/fd)
