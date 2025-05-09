# A try at writing Bosch MCAN Rust driver

# Goals

* [x] Stand-alone, no external pac/hal dependencies
* [ ] Support for STM32 G0, G4, H7, L5 (should be possible to support others as well)
* [x] No panics or asserts, always return Result if something goes wrong
* [x] No blocking waits without timeout
* [ ] Optional async support (embassy or RTIC?)
    * Interrupt handling
    * Bus off management task
    * async tx/rx
* [ ] Optional sync mode with channels?
* [x] Use stm32-data generated register abstraction layer
* [x] Minimize usage of macros
* [x] Reduce number of generics, for example, FdCan is not generic over CAN peripheral instance
* Support raw timestamping in both Classical and FD modes, let users handle time conversions
* [x] RAM layout configuration builder (similar to how usbd builds descriptors)
    * Possibility to change layout, for each instance individually or recombine layouts into one and start over.
* Dynamic reconfiguration
* [x] Clock disable, ensuring that all instances are in powered down mode
* [ ] TX completion tracking (i.e., to support backpressure)
* [ ] Time that frame spends in transmit queue before actual transmission (if requested to measure)
* [ ] Dedicated TX slots and RX buffers
* [ ] RX FIFO watermarking
* [ ] Manual timings control + predefined common configurations as consts
* [ ] Time-Triggered mode (TTCAN)
* [ ] Configure all bells and whistles (DAR, transmit pause, sampling point debug, etc)
* [ ] Sleep and wakeup

# Examples

* [ ] H7 + embassy
* [ ] H7 + RTIC
* [ ] H7 + stm32h7xx-hal
* [ ] G0, G4
* [ ] L5
* [ ] Advanced examples

# Based on

* [fdcan](https://github.com/stm32-rs/fdcan)
* [embassy fdcan](https://github.com/embassy-rs/embassy/tree/main/embassy-stm32/src/can/fd)
* [hansihe embassy fork](https://github.com/hansihe/embassy/tree/fdcan-rewrite/embassy-stm32/src/can/fd)