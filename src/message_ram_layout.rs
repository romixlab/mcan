use crate::message_ram_builder::ElevenBitFilters;
use crate::pac::message_ram::{TxBufferElementT0, TxBufferElementT1};
use crate::pac_traits::{RW, Reg};
use crate::{Error, FdCan, FdCanInstance, MessageRamBuilder};

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MessageRamLayout {
    pub(crate) eleven_bit_filters_addr: u16,
    pub(crate) eleven_bit_filters_len: u8,

    pub(crate) twenty_nine_bit_filters_addr: u16,
    pub(crate) twenty_nine_bit_filters_len: u8,

    pub(crate) rx_fifo0_addr: u16,
    pub(crate) rx_fifo0_len: u8,
    pub(crate) rx_fifo0_data_size: DataFieldSize,

    pub(crate) rx_fifo1_addr: u16,
    pub(crate) rx_fifo1_len: u8,
    pub(crate) rx_fifo1_data_size: DataFieldSize,

    pub(crate) rx_buffers_addr: u16,
    /// Only start address is used by the core, but len is used for bounds checks
    pub(crate) rx_buffers_len: u8,
    pub(crate) rx_buffers_data_size: DataFieldSize,

    pub(crate) tx_event_fifo_addr: u16,
    pub(crate) tx_event_fifo_len: u8,

    pub(crate) tx_buffers_addr: u16,
    /// Number of dedicated transmit buffers
    pub(crate) tx_buffers_len: u8,
    /// Transmit FIFO/Queue size
    pub(crate) tx_fifo_or_queue_len: u8,
    pub(crate) tx_buffers_data_size: DataFieldSize,

    #[cfg(feature = "h7")]
    pub(crate) trigger_memory_addr: u16,
    #[cfg(feature = "h7")]
    pub(crate) trigger_memory_len: u8,
}

impl MessageRamLayout {
    pub(crate) const fn default() -> Self {
        Self {
            eleven_bit_filters_addr: 0,
            eleven_bit_filters_len: 0,
            twenty_nine_bit_filters_addr: 0,
            twenty_nine_bit_filters_len: 0,
            rx_fifo0_addr: 0,
            rx_fifo0_len: 0,
            rx_fifo0_data_size: DataFieldSize::_8Bytes,
            rx_fifo1_addr: 0,
            rx_fifo1_len: 0,
            rx_fifo1_data_size: DataFieldSize::_8Bytes,
            rx_buffers_addr: 0,
            rx_buffers_len: 0,
            rx_buffers_data_size: DataFieldSize::_8Bytes,
            tx_event_fifo_addr: 0,
            tx_event_fifo_len: 0,
            tx_buffers_addr: 0,
            tx_buffers_len: 0,
            tx_fifo_or_queue_len: 0,
            tx_buffers_data_size: DataFieldSize::_8Bytes,
        }
    }
}

impl MessageRamLayout {
    // Turn this layout back into builder, useful if doing re-init of just one CAN instance, without touching others.
    pub fn relayout(self) -> MessageRamBuilder<ElevenBitFilters> {
        // pos: first non zero start, end: last non zero start+size?
        todo!()
    }
}

/// Data size of RX FIFO0/1, RX buffer and TX buffer element, total element size is 8 bytes longer (2 words header).
/// Should probably be all the same, and either 8 bytes or 64 bytes, unless some very specific configuration is desired.
#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum DataFieldSize {
    _8Bytes = 8,
    _12Bytes = 12,
    _16Bytes = 16,
    _20Bytes = 20,
    _24Bytes = 24,
    _32Bytes = 32,
    _48Bytes = 48,
    _64Bytes = 64,
}

impl DataFieldSize {
    pub(crate) fn max_len(&self) -> u8 {
        *self as u8
    }
}

impl DataFieldSize {
    pub(crate) const fn words(&self) -> u16 {
        match self {
            DataFieldSize::_8Bytes => 2,
            DataFieldSize::_12Bytes => 3,
            DataFieldSize::_16Bytes => 4,
            DataFieldSize::_20Bytes => 5,
            DataFieldSize::_24Bytes => 6,
            DataFieldSize::_32Bytes => 8,
            DataFieldSize::_48Bytes => 12,
            DataFieldSize::_64Bytes => 16,
        }
    }

    pub(crate) const fn config_register(&self) -> u8 {
        match self {
            DataFieldSize::_8Bytes => 0b000,
            DataFieldSize::_12Bytes => 0b001,
            DataFieldSize::_16Bytes => 0b010,
            DataFieldSize::_20Bytes => 0b011,
            DataFieldSize::_24Bytes => 0b100,
            DataFieldSize::_32Bytes => 0b101,
            DataFieldSize::_48Bytes => 0b110,
            DataFieldSize::_64Bytes => 0b111,
        }
    }
}

pub struct MessageRam<'a> {
    layout: &'a MessageRamLayout,
    instance: FdCanInstance,
}

/// TX buffer index, up to 32 buffers (dedicated or part of FIFO/Queue) could exist, but it depends on the particular peripheral
/// instance and RAM layout configuration. Contains an instance it belongs to as well, so it shouldn't be
/// possible to craft an invalid index outside of this crate.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TxBufferIdx {
    pub(crate) instance: FdCanInstance,
    idx: u8,
}

impl TxBufferIdx {
    pub(crate) fn idx(&self) -> usize {
        self.idx as usize
    }
}

pub enum FIFONr {
    FIFO0,
    FIFO1,
}

impl FIFONr {
    pub(crate) fn nr(&self) -> usize {
        match self {
            FIFONr::FIFO0 => 0,
            FIFONr::FIFO1 => 1,
        }
    }
}

pub(crate) struct TxBufferElement {
    pub(crate) t0: Reg<TxBufferElementT0, RW>,
    pub(crate) t1: Reg<TxBufferElementT1, RW>,
    pub(crate) data: &'static mut [u32],
}

impl<'a> MessageRam<'a> {
    pub(crate) fn tx_buffer(&self, idx: TxBufferIdx) -> Result<TxBufferElement, Error> {
        if idx.instance != self.instance {
            return Err(Error::WrongInstance);
        }
        if self.layout.tx_buffers_len == 0 || idx.idx >= self.layout.tx_buffers_len {
            return Err(Error::TxBufferIndexOutOfRange);
        }
        let offset = self.layout.tx_buffers_addr + idx.idx as u16;
        let tx_buffers_len = self.layout.tx_buffers_data_size.words() as usize;
        unsafe {
            let tx_buffer_t0 = crate::pac::FDCAN_MSGRAM_ADDR.add(offset as usize);
            Ok(TxBufferElement {
                t0: Reg::from_ptr(tx_buffer_t0 as *mut _),
                t1: Reg::from_ptr(tx_buffer_t0.add(1) as *mut _),
                data: core::slice::from_raw_parts_mut(tx_buffer_t0.add(2), tx_buffers_len),
            })
        }
    }

    // pub(crate) tx_fifo_put()
    // pub(crate) tx_queue_put()
}

impl<M> FdCan<M> {
    pub(crate) fn message_ram(&mut self) -> MessageRam<'_> {
        MessageRam {
            layout: &self.config.layout,
            instance: self.instance,
        }
    }
}
