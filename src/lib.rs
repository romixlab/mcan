#![no_std]

pub mod config;
#[cfg(feature = "h7")]
pub mod message_ram_builder;

pub mod fdcan;
pub mod pac;
pub mod util;

#[cfg(feature = "asynchronous")]
pub mod asynchronous;
#[cfg(feature = "embassy")]
pub mod embassy;
pub mod id;
pub mod interrupt;
mod message_ram_layout;
pub mod status;
pub mod tx_rx;

pub use config::{BitTimingError, DataBitTiming, NominalBitTiming, TransceiverDelayCompensation};
pub use fdcan::{
    ConfigMode, Error, FdCan, FdCanInstance, FdCanInstances, InternalLoopbackMode, PoweredDownMode,
    TestMode, TxPinControl,
};
pub use id::{ExtendedId, Id, StandardId};
pub use interrupt::{InterruptLine, Interrupts, on_interrupt};
#[cfg(feature = "h7")]
pub use message_ram_builder::{MessageRamBuilder, MessageRamBuilderError, RamBuilderInitialState};
pub use message_ram_layout::RxFifo;
#[cfg(feature = "h7")]
pub use message_ram_layout::{DataFieldSize, MessageRamLayout, TxBufferIdx};
pub use status::{Activity, ErrorCounters, ErrorState, LastErrorCode, ProtocolStatus};
pub use tx_rx::{RxFrameHeader, TxFrameHeader};

#[cfg(feature = "rcc")]
// we must wait two peripheral clock cycles before the clock is active
// http://efton.sk/STM32/gotcha/g183.html
const CLOCK_DOMAIN_SYNCHRONIZATION_DELAY: u32 = 100;
