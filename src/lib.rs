#![no_std]

pub mod config;
pub mod message_ram;
pub mod pac_traits;

pub use message_ram::{
    DataFieldSize, MessageRamBuilder, MessageRamBuilderError, MessageRamLayout,
    RamBuilderInitialState,
};
pub mod fdcan;
pub mod pac;
mod util;

#[cfg(feature = "embassy")]
pub mod embassy;
mod tx_rx;

pub use fdcan::{
    ConfigMode, Error, FdCan, FdCanInstance, FdCanInstances, InternalLoopbackMode, PoweredDownMode,
};

// we must wait two peripheral clock cycles before the clock is active
// http://efton.sk/STM32/gotcha/g183.html
const CLOCK_DOMAIN_SYNCHRONIZATION_DELAY: u32 = 100;
