#![no_std]

pub mod config;
#[cfg(feature = "h7")]
pub mod message_ram_builder;
pub mod pac_traits;

pub mod fdcan;
pub mod pac;
pub mod util;

#[cfg(feature = "asynchronous")]
pub mod asynchronous;
#[cfg(feature = "embassy")]
pub mod embassy;
pub mod id;
mod message_ram_layout;
pub mod tx_rx;

pub use config::{DataBitTiming, NominalBitTiming};
pub use fdcan::{
    ConfigMode, Error, FdCan, FdCanInstance, FdCanInstances, FdCanInterrupt, InternalLoopbackMode,
    PoweredDownMode,
};
pub use id::{ExtendedId, Id, StandardId};
#[cfg(feature = "h7")]
pub use message_ram_builder::{MessageRamBuilder, MessageRamBuilderError, RamBuilderInitialState};
#[cfg(feature = "h7")]
pub use message_ram_layout::{DataFieldSize, MessageRamLayout, TxBufferIdx};
pub use tx_rx::TxFrameHeader;

// we must wait two peripheral clock cycles before the clock is active
// http://efton.sk/STM32/gotcha/g183.html
const CLOCK_DOMAIN_SYNCHRONIZATION_DELAY: u32 = 100;
