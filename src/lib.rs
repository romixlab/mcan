#![no_std]

pub mod config;
pub mod message_ram_builder;
pub mod pac_traits;

pub use message_ram_builder::{MessageRamBuilder, MessageRamBuilderError, RamBuilderInitialState};
pub mod fdcan;
pub mod pac;
pub mod util;

#[cfg(feature = "embassy")]
pub mod embassy;
mod message_ram_layout;
pub mod tx_rx;

pub use fdcan::{
    ConfigMode, Error, FdCan, FdCanInstance, FdCanInstances, InternalLoopbackMode, PoweredDownMode,
};
pub use message_ram_layout::DataFieldSize;
pub use message_ram_layout::MessageRamLayout;

// we must wait two peripheral clock cycles before the clock is active
// http://efton.sk/STM32/gotcha/g183.html
const CLOCK_DOMAIN_SYNCHRONIZATION_DELAY: u32 = 100;
