# A try at writing Bosch MCAN Rust driver

# Goals

* No panics or asserts, always return Result if something goes wrong
* No blocking waits without timeout
* Stand-alone, no external pac/hal dependencies
* Optional async support (embassy)
    * Interrupt handling
    * Bus off management task
    * async tx/rx
* Use stm32-data generated register abstraction layer
* Minimize usage of macros
* Reduce number of generics, for example, FdCan is not generic over CAN peripheral instance
* Support raw timestamping in both Classical and FD modes, let users handle time conversions
* RAM layout configuration builder (similar to how usbd builds descriptors)
    * Possibility to change layout, for each instance individually or recombine layouts into one and start over.
* Dynamic reconfiguration
* TX completion tracking (i.e., to support backpressure)
* Time that frame spends in transmit queue before actual transmission (if requested to measure)
* Dedicated TX slots
* RX FIFO watermarking
* Manual timings control + predefined common configurations as consts
* Time-Triggered mode (TTCAN)
* Configure all bells and whistles (DAR, transmit pause, sampling point debug, etc)
* Sleep and wakeup

# Based on

* [fdcan](https://github.com/stm32-rs/fdcan)
* [embassy fdcan](https://github.com/embassy-rs/embassy/tree/main/embassy-stm32/src/can/fd)
* [hansihe embassy fork](https://github.com/hansihe/embassy/tree/fdcan-rewrite/embassy-stm32/src/can/fd)