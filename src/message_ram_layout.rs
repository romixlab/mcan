use crate::pac::common::{RW, Reg};
use crate::pac::message_ram::{
    EventFIFOControl, Rtr, TimeStampCaptureEnable, TxBufferElementT0, TxBufferElementT1,
};
#[cfg(feature = "h7")]
use crate::pac::message_ram::{RxElementR0, RxElementR1, dlc_to_len};
use crate::tx_rx::{Dlc, TxFrameHeader};
use crate::{Error, FdCan, FdCanInstance};
use core::ops::Range;

/// Message RAM layout containing location and sizes of various buffers.
///
/// Note: only if core supports it, for example, G0 and G4 have fixed layout.
#[cfg(feature = "h7")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MessageRamLayout {
    /// Instance this layout was built for, `None` for the empty default layout.
    pub(crate) instance: Option<FdCanInstance>,
    /// Region of the message RAM owned by this layout, in words: `start_addr..end_addr`.
    pub(crate) start_addr: u16,
    pub(crate) end_addr: u16,

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

    pub(crate) trigger_memory_addr: u16,
    pub(crate) trigger_memory_len: u8,
}

#[cfg(feature = "h7")]
impl MessageRamLayout {
    pub(crate) const fn default() -> Self {
        Self {
            instance: None,
            start_addr: 0,
            end_addr: 0,
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

            trigger_memory_addr: 0,
            trigger_memory_len: 0,
        }
    }
}

#[cfg(feature = "h7")]
impl MessageRamLayout {
    /// Message RAM words owned by this layout, clamped to the message RAM.
    pub(crate) fn region(&self) -> Range<usize> {
        let end = (self.end_addr as usize).min(crate::pac::FDCAN_MSGRAM_LEN_WORDS);
        (self.start_addr as usize).min(end)..end
    }

    /// Word offset of element `idx` of an RX FIFO and its data size, `None` if it doesn't exist in this layout.
    pub(crate) const fn rx_fifo_element_addr(
        &self,
        fifo: RxFifo,
        idx: u8,
    ) -> Option<(u16, DataFieldSize)> {
        let (addr, len, size) = match fifo {
            RxFifo::Fifo0 => (
                self.rx_fifo0_addr,
                self.rx_fifo0_len,
                self.rx_fifo0_data_size,
            ),
            RxFifo::Fifo1 => (
                self.rx_fifo1_addr,
                self.rx_fifo1_len,
                self.rx_fifo1_data_size,
            ),
        };
        if idx >= len {
            return None;
        }
        // At most 64 elements of 18 words, no overflow.
        Some((addr + idx as u16 * size.element_words(), size))
    }

    /// Word offset of the dedicated TX buffer `idx`, `None` if it doesn't exist in this layout.
    pub(crate) const fn tx_buffer_addr(&self, idx: u8) -> Option<u16> {
        if idx >= self.tx_buffers_len {
            return None;
        }
        // At most 32 elements of 18 words, no overflow.
        Some(self.tx_buffers_addr + idx as u16 * self.tx_buffers_data_size.element_words())
    }
}

/// Words of the fixed per-instance message RAM block on FDCAN lite cores: 28 standard filters (1 word),
/// 8 extended filters (2), 2 RX FIFOs of 3 elements (18 each), 3 TX event elements (2) and 3 TX buffers (18).
/// Instance n starts at n * 212 words (RM0444, FDCAN message RAM; stm32-data: FDCAN2 RAM at +0x350).
#[cfg(not(feature = "h7"))]
pub(crate) const LITE_INSTANCE_WORDS: usize = 28 + 8 * 2 + 2 * 3 * 18 + 3 * 2 + 3 * 18;

/// Message RAM words owned by `instance` on FDCAN lite cores.
#[cfg(not(feature = "h7"))]
pub(crate) const fn lite_region(instance: FdCanInstance) -> Range<usize> {
    let start = instance as usize * LITE_INSTANCE_WORDS;
    start..start + LITE_INSTANCE_WORDS
}

/// Data size of RX FIFO0/1, RX buffer and TX buffer element, total element size is 8 bytes longer (2 words header).
/// Should probably be all the same, and either 8 bytes or 64 bytes, unless some very specific configuration is desired.
#[cfg(feature = "h7")]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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

#[cfg(feature = "h7")]
impl DataFieldSize {
    pub(crate) fn max_len(&self) -> u8 {
        *self as u8
    }
}

#[cfg(feature = "h7")]
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

    /// Size of a whole RX buffer / RX FIFO / TX buffer element: 2 header words plus data.
    pub(crate) const fn element_words(&self) -> u16 {
        2 + self.words()
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

#[cfg(feature = "h7")]
pub struct MessageRam<'a> {
    layout: &'a MessageRamLayout,
    instance: FdCanInstance,
}

#[cfg(not(feature = "h7"))]
pub struct MessageRam {
    instance: FdCanInstance,
}

/// Dedicated TX buffer index that can be obtained during RAM layout by calling allocate_dedicated_tx_buffer().
/// Not available on G0, G4 and L5, as there is no support for dedicated TX buffers on these MCUs.
///
/// Up to 32 buffers (dedicated or part of FIFO/Queue) could exist, but it depends on the particular peripheral
/// instance and RAM layout configuration. Contains an instance it belongs to as well, so trying to use an index from one CAN instance
/// with another will result in an Error::WrongInstance.
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TxBufferIdx {
    pub(crate) instance: FdCanInstance,
    pub(crate) idx: u8,
}

impl TxBufferIdx {
    pub(crate) fn idx(&self) -> usize {
        self.idx as usize
    }
}

/// One of the two RX FIFOs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum RxFifo {
    Fifo0,
    Fifo1,
}

impl RxFifo {
    /// Index into the RXFxS / RXFxA register arrays.
    pub(crate) const fn nr(&self) -> usize {
        match self {
            RxFifo::Fifo0 => 0,
            RxFifo::Fifo1 => 1,
        }
    }
}

pub(crate) struct TxBufferElement {
    pub(crate) t0: Reg<TxBufferElementT0, RW>,
    pub(crate) t1: Reg<TxBufferElementT1, RW>,
    pub(crate) data: &'static mut [u32],
}

impl TxBufferElement {
    pub(crate) fn fill(&mut self, tx_header: &TxFrameHeader, dlc: Dlc) {
        self.t0.write(|w| {
            w.set_esi(tx_header.error_state);
            w.set_xtd(tx_header.id.into());
            w.set_rtr(Rtr::TransmitDataFrame); // TODO: support for RTR?
            w.set_id(tx_header.id.reg_value());
        });
        self.t1.write(|w| {
            w.set_message_marker_low(tx_header.marker.unwrap_or(0)); // TODO: make marker non-optional?
            w.set_efc(EventFIFOControl::DontStoreTxEvents); // TODO: control TX event store
            w.set_tsce(TimeStampCaptureEnable::Disabled);
            w.set_fdf(tx_header.frame_format);
            w.set_brs(tx_header.bit_rate_switching.into());
            w.set_dlc(dlc.reg_value());
            w.set_message_marker_high(0);
        });
    }
}

#[cfg(feature = "h7")]
impl<'a> MessageRam<'a> {
    pub(crate) fn tx_buffer(&self, idx: TxBufferIdx) -> Result<TxBufferElement, Error> {
        if idx.instance != self.instance {
            return Err(Error::WrongInstance);
        }
        let Some(offset) = self.layout.tx_buffer_addr(idx.idx) else {
            return Err(Error::TxBufferIndexOutOfRange);
        };
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

    /// Reads RX FIFO element `idx`: header words and up to `buf.len()` data bytes.
    /// Returns the two header words, the number of bytes copied and whether the element was too small for
    /// the frame (data truncated by the M_CAN).
    pub(crate) fn read_rx_fifo_element(
        &self,
        fifo: RxFifo,
        idx: u8,
        buf: &mut [u8],
    ) -> Result<(RxElementR0, RxElementR1, u8, bool), Error> {
        let Some((offset, data_size)) = self.layout.rx_fifo_element_addr(fifo, idx) else {
            return Err(Error::RxFifoIndexOutOfRange);
        };
        // SAFETY: the offset lies inside this instance's layout, which lies inside the message RAM.
        let element = unsafe { crate::pac::FDCAN_MSGRAM_ADDR.add(offset as usize) };
        let r0 = RxElementR0::from_bits(unsafe { element.read_volatile() });
        let r1 = RxElementR1::from_bits(unsafe { element.add(1).read_volatile() });
        let len = dlc_to_len(r1.dlc(), r1.fdf());
        // If the element is smaller than the frame, the M_CAN stores only what fits.
        let stored = len.min(data_size.max_len());
        if buf.len() < stored as usize {
            return Err(Error::BufferTooSmall);
        }
        for (i, chunk) in buf[..stored as usize].chunks_mut(4).enumerate() {
            let word = unsafe { element.add(2 + i).read_volatile() }.to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        Ok((r0, r1, stored, stored < len))
    }

    // pub(crate) tx_fifo_put()
    // pub(crate) tx_queue_put()
}

#[cfg(not(feature = "h7"))]
impl MessageRam {}

impl<M> FdCan<M> {
    #[cfg(feature = "h7")]
    pub(crate) fn message_ram(&mut self) -> MessageRam<'_> {
        MessageRam {
            layout: &self.config.layout,
            instance: self.instance,
        }
    }

    #[cfg(not(feature = "h7"))]
    pub(crate) fn message_ram(&mut self) -> MessageRam {
        MessageRam {
            instance: self.instance,
        }
    }
}

#[cfg(all(test, not(feature = "h7")))]
mod tests {
    use super::*;

    #[test]
    fn lite_regions_are_disjoint_and_fit() {
        assert_eq!(LITE_INSTANCE_WORDS, 212);
        assert_eq!(lite_region(FdCanInstance::FdCan1), 0..212);
        // stm32-data: FDCAN2 message RAM at 0x4000_B750 = 0x4000_B400 + 0x350 bytes.
        assert_eq!(lite_region(FdCanInstance::FdCan2), 0x350 / 4..2 * 212);
        assert!(lite_region(FdCanInstance::FdCan2).end <= crate::pac::FDCAN_MSGRAM_LEN_WORDS);
    }
}
