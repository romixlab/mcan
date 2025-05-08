# A try at writing Bosch MCAN Rust driver

# Goals
* No panics or asserts, always return Result if something goes wrong
* No blocking waits without timeout
* Stand-alone, no external pac/hal dependencies
* Optional async support (embassy)
* Minimize usage of macros
* Reduce amount of generics, for example, FdCan is not generic over CAN peripheral instance
* Support raw timestamping in both Classical and FD modes, let user handle time conversions
* RAM layout configuration builder (similar to how usbd builds descriptors)
* Bus off management task
* Dynamic reconfiguration
* TX completion tracking (i.e., to support backpressure)
* Time that frame spends in transmit queue before actual transmission (if requested to measure)
* Dedicated TX slots
* RX FIFO watermarking
* Manual timings control + predefined common configurations as consts
* Configure all bells and whistles (DAR, transmit pause, sampling point debug, etc)
* Sleep and wakeup

# Based on
* [fdcan](https://github.com/stm32-rs/fdcan)
* [embassy fdcan](https://github.com/embassy-rs/embassy/tree/main/embassy-stm32/src/can/fd)
* [hansihe embassy fork](https://github.com/hansihe/embassy/tree/fdcan-rewrite/embassy-stm32/src/can/fd)