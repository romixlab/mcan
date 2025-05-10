use crate::fdcan::Error;
use crate::message_ram_layout::{DataFieldSize, MessageRamLayout};
use crate::{MessageRamBuilder, MessageRamBuilderError, RamBuilderInitialState};

#[inline]
pub(crate) fn checked_wait<F: Fn() -> bool>(f: F, timeout_iterations: u32) -> Result<(), Error> {
    let mut elapsed = 0;
    while f() {
        elapsed += 1;
        if elapsed >= timeout_iterations {
            return Err(Error::Timeout);
        }
    }
    Ok(())
}

macro_rules! unwrap_or_return {
    ($expr:expr) => {
        match $expr {
            Ok(b) => b,
            Err(e) => return Err(e),
        }
    };
}

pub const fn basic_layout(
    builder: MessageRamBuilder<RamBuilderInitialState>,
) -> Result<(MessageRamLayout, MessageRamBuilder<RamBuilderInitialState>), MessageRamBuilderError> {
    let b = unwrap_or_return!(builder.allocate_11bit_filters(1));
    let b = unwrap_or_return!(b.allocate_29bit_filters(1));
    let b = unwrap_or_return!(b.allocate_rx_fifo0_buffers(1, DataFieldSize::_64Bytes));
    let b = unwrap_or_return!(b.allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes));
    let b = unwrap_or_return!(b.allocate_tx_event_fifo_buffers(1));
    let (layout, builder) = unwrap_or_return!(b.allocate_tx_buffers(1, 1, DataFieldSize::_64Bytes));
    Ok((layout, builder))
}
