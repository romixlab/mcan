use crate::Id;
use crate::fdcan::Receive;
use crate::fdcan::Transmit;
use crate::message_ram_layout::RxFifo;
use crate::message_ram_layout::TxBufferIdx;
use crate::pac::message_ram::{Esi, FrameFormat};
use crate::pac::message_ram::{RxElementR0, RxElementR1, Xtd, dlc_to_len};
use crate::util::checked_wait;
use crate::{Error, FdCan};
use crate::{ExtendedId, StandardId};

#[derive(Copy, Clone)]
#[repr(u8)]
pub enum Dlc {
    _0Bytes = 0,
    _1Bytes = 1,
    _2Bytes = 2,
    _3Bytes = 3,
    _4Bytes = 4,
    _5Bytes = 5,
    _6Bytes = 6,
    _7Bytes = 7,
    _8Bytes = 8,
    _12Bytes = 12,
    _16Bytes = 16,
    _20Bytes = 20,
    _24Bytes = 24,
    _32Bytes = 32,
    _48Bytes = 48,
    _64Bytes = 64,
}

impl Dlc {
    #[cfg(feature = "h7")]
    const fn len(&self) -> u8 {
        *self as u8
    }

    const fn from_len(len: usize) -> Option<Self> {
        match len {
            0 => Some(Self::_0Bytes),
            1 => Some(Self::_1Bytes),
            2 => Some(Self::_2Bytes),
            3 => Some(Self::_3Bytes),
            4 => Some(Self::_4Bytes),
            5 => Some(Self::_5Bytes),
            6 => Some(Self::_6Bytes),
            7 => Some(Self::_7Bytes),
            8 => Some(Self::_8Bytes),
            12 => Some(Self::_12Bytes),
            16 => Some(Self::_16Bytes),
            20 => Some(Self::_20Bytes),
            24 => Some(Self::_24Bytes),
            32 => Some(Self::_32Bytes),
            48 => Some(Self::_48Bytes),
            64 => Some(Self::_64Bytes),
            _ => None,
        }
    }

    pub(crate) fn reg_value(&self) -> u8 {
        match self {
            Dlc::_0Bytes => 0,
            Dlc::_1Bytes => 1,
            Dlc::_2Bytes => 2,
            Dlc::_3Bytes => 3,
            Dlc::_4Bytes => 4,
            Dlc::_5Bytes => 5,
            Dlc::_6Bytes => 6,
            Dlc::_7Bytes => 7,
            Dlc::_8Bytes => 8,
            Dlc::_12Bytes => 9,
            Dlc::_16Bytes => 10,
            Dlc::_20Bytes => 11,
            Dlc::_24Bytes => 12,
            Dlc::_32Bytes => 13,
            Dlc::_48Bytes => 14,
            Dlc::_64Bytes => 15,
        }
    }
}

/// Header of a transmit request
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TxFrameHeader {
    /// Type of message - Classical or FD.
    pub frame_format: FrameFormat,
    /// Id
    pub id: Id,
    /// Should bit rate switching be used
    ///
    /// Not that this is a request, and if the global frame_transmit is set to ClassicCanOnly
    /// this is ignored.
    pub bit_rate_switching: bool,
    /// Whether this node is error passive or not
    pub error_state: Esi,
    pub marker: Option<u8>,
}

impl TxFrameHeader {
    pub fn fd_brs(id: Id) -> Self {
        Self {
            frame_format: FrameFormat::FD,
            id,
            bit_rate_switching: true,
            error_state: Esi::EsiDependsOnErrorPassive,
            marker: None,
        }
    }
}

impl<M: Transmit> FdCan<M> {
    // Puts a CAN frame in a transmit mailbox for transmission on the bus.
    //
    // Frames are transmitted to the bus based on their priority (identifier). Transmit order is
    // preserved for frames with identical identifiers.
    //
    // If all transmit mailboxes are full, a higher priority frame can replace a lower-priority
    // frame, which is returned via the closure 'pending'. If 'pending' is called; it's return value
    // is returned via `Option<P>`, if it is not, None is returned.
    // If there are only higher priority frames in the queue, this returns Err::WouldBlock
    // pub fn transmit(
    //     &mut self,
    //     frame: TxFrameHeader,
    //     buffer: &[u8],
    // ) -> nb::Result<Option<()>, Infallible> {
    //     self.transmit_preserve(frame, buffer, &mut |_, _, _| ())
    // }

    // As Transmit, but if there is a pending frame, `pending` will be called so that the frame can
    // be preserved.
    // pub fn transmit_preserve<PTX, P>(
    //     &mut self,
    //     frame: TxFrameHeader,
    //     buffer: &[u8],
    //     pending: &mut PTX,
    // ) -> nb::Result<Option<P>, Infallible>
    // where
    //     PTX: FnMut(TxBufferIdx, TxFrameHeader, &[u32]) -> P,
    // {
    //     let queue_is_full = self.tx_queue_is_full();
    //
    //     let id = frame.into();
    //
    //     // If the queue is full,
    //     // Discard the first slot with a lower priority message
    //     let (idx, pending_frame) = if queue_is_full {
    //         if self.is_available(Mailbox::_0, id) {
    //             (
    //                 Mailbox::_0,
    //                 self.abort_pending_tx_buffer(Mailbox::_0, pending),
    //             )
    //         } else if self.is_available(Mailbox::_1, id) {
    //             (
    //                 Mailbox::_1,
    //                 self.abort_pending_tx_buffer(Mailbox::_1, pending),
    //             )
    //         } else if self.is_available(Mailbox::_2, id) {
    //             (
    //                 Mailbox::_2,
    //                 self.abort_pending_tx_buffer(Mailbox::_2, pending),
    //             )
    //         } else {
    //             // For now we bail when there is no lower priority slot available
    //             // Can this lead to priority inversion?
    //             return Err(nb::Error::WouldBlock);
    //         }
    //     } else {
    //         // Read the Write Pointer
    //         let idx = can.txfqs.read().tfqpi().bits();
    //
    //         (Mailbox::new(idx), None)
    //     };
    //
    //     self.write_tx_buffer_pend(idx, frame, buffer);
    //
    //     Ok(pending_frame)
    // }

    /// Returns if the tx queue is able to accept new messages without having to cancel an existing one
    #[inline]
    pub fn tx_queue_is_full(&self) -> bool {
        self.can.txfqs().read().tfqf()
    }

    // Returns `Ok` when the mailbox is free or if it contains pending frame with a
    // lower priority (higher ID) than the identifier `id`.
    // #[inline]
    // fn is_available(&self, idx: TxBufferIdx, id: IdReg) -> bool {
    //     if self.has_pending_frame(idx) {
    //         //read back header section
    //         let header: TxFrameHeader = (&self.tx_msg_ram().tbsa[idx.idx()].header).into();
    //         let old_id: IdReg = header.into();
    //
    //         id > old_id
    //     } else {
    //         true
    //     }
    // }

    /// Write dedicated TX buffer and set the corresponding "add request" bit.
    #[cfg(feature = "h7")]
    pub fn write_tx_buffer_pend(
        &mut self,
        idx: TxBufferIdx,
        tx_header: TxFrameHeader,
        data: &[u8],
    ) -> Result<(), Error> {
        if idx.instance != self.instance {
            return Err(Error::WrongInstance);
        }
        let mut tx_buffer = self.message_ram().tx_buffer(idx)?;
        let Some(dlc) = Dlc::from_len(data.len()) else {
            return Err(Error::WrongDataSize);
        };
        if dlc.len() > self.config.layout.tx_buffers_data_size.max_len() {
            return Err(Error::WrongDataSize);
        }

        tx_buffer.fill(&tx_header, dlc);

        let mut chunks = data.chunks(4);
        for d in tx_buffer.data {
            let Some(chunk) = chunks.next() else {
                break;
            };
            // The last chunk may be shorter than 4 bytes, pad it with zeros.
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            *d = u32::from_le_bytes(word);
        }

        // Set as ready to transmit
        _ = self.tx_buffer_pend(idx);
        Ok(())
    }

    /// Mark dedicated TX buffer as ready to transmit without modifying anything
    #[cfg(feature = "h7")]
    #[inline]
    pub fn tx_buffer_pend(&mut self, idx: TxBufferIdx) -> Result<(), Error> {
        if idx.instance != self.instance {
            return Err(Error::WrongInstance);
        }
        // Set as ready to transmit
        self.can.txbar().modify(|w| w.set_ar(idx.idx(), true));
        Ok(())
    }

    // #[inline]
    // fn abort_pending_tx_buffer<PTX, R>(
    //     &mut self,
    //     idx: TxBufferIdx,
    //     pending: PTX,
    // ) -> Result<Option<R>, Error>
    // where
    //     PTX: FnOnce(TxBufferIdx, TxFrameHeader, &[u32]) -> R,
    // {
    //     if self.abort(idx)? {
    //         // read back header section
    //         let header = (&tx_ram.tbsa[idx.idx()].header).into();
    //         let mut data = [0u32; 16];
    //         for (byte, register) in data.iter_mut().zip(tx_ram.tbsa[idx as usize].data.iter()) {
    //             *byte = register.read();
    //         }
    //         Ok(Some(pending(idx, header, &data)))
    //     } else {
    //         // Abort request failed because the frame was already sent (or being sent) on
    //         // the bus. All mailboxes are now free. This can happen for small prescaler
    //         // values (e.g. 1MBit/s bit timing with a source clock of 8MHz) or when an ISR
    //         // has preempted the execution.
    //         Ok(None)
    //     }
    // }

    // TODO: abort async
    /// Attempts to abort the sending of a frame that is pending in a mailbox.
    ///
    /// If there is no frame in the provided mailbox, or its transmission succeeds before it can be
    /// aborted, this function has no effect and returns `false`.
    ///
    /// If there is a frame in the provided mailbox, and it is canceled successfully, this function
    /// returns `true`.
    ///
    /// NOTE: Core supports multiple tx buffers abort as well.
    #[inline]
    pub fn abort_blocking(&mut self, idx: TxBufferIdx) -> Result<bool, Error> {
        if idx.instance != self.instance {
            return Err(Error::WrongInstance);
        }
        // Check if there is a request pending to abort
        if self.has_pending_frame(idx) {
            // Abort Request
            self.can.txbcr().write(|w| w.set_cr(idx.idx(), true));

            // Wait for the abort request to be finished.
            checked_wait(
                || self.can.txbcf().read().cf(idx.idx()),
                self.config.timeout_iterations_long,
            )?;
            Ok(!self.can.txbto().read().to(idx.idx()))
        } else {
            Ok(false)
        }
    }

    #[inline]
    fn has_pending_frame(&self, idx: TxBufferIdx) -> bool {
        self.can.txbrp().read().trp(idx.idx())
    }

    /// Returns `true` if no frame is pending for transmission.
    #[inline]
    pub fn is_idle(&self) -> bool {
        self.can.txbrp().read().0 == 0x0
    }
}

/// FDCAN lite cores (G0, G4, L5, H5): fixed RAM layout with a 3-element TX FIFO, no dedicated TX buffers.
#[cfg(not(feature = "h7"))]
impl<M: Transmit> FdCan<M> {
    /// Queues a frame in the TX FIFO (elements are sent in the order they were put) and requests transmission.
    ///
    /// Returns the buffer index, usable with [FdCan::abort_blocking]. [Error::TxQueueFull] if all 3 elements are
    /// pending. `data.len()` must be a valid CAN FD length (0..=8, 12, 16, 20, 24, 32, 48, 64).
    pub fn transmit(&mut self, tx_header: TxFrameHeader, data: &[u8]) -> Result<TxBufferIdx, Error> {
        let Some(dlc) = Dlc::from_len(data.len()) else {
            return Err(Error::WrongDataSize);
        };
        if self.tx_queue_is_full() {
            return Err(Error::TxQueueFull);
        }
        // TX FIFO put index: the element the core will send next after the ones already queued.
        let idx = TxBufferIdx {
            instance: self.instance,
            idx: self.can.txfqs().read().tfqpi(),
        };
        let mut tx_buffer = self.message_ram().tx_buffer(idx.idx)?;
        tx_buffer.fill(&tx_header, dlc);
        for (d, chunk) in tx_buffer.data.iter_mut().zip(data.chunks(4)) {
            // The last chunk may be shorter than 4 bytes, pad it with zeros.
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            *d = u32::from_le_bytes(word);
        }
        self.can.txbar().write(|w| w.set_ar(idx.idx(), true));
        Ok(idx)
    }
}

/// Header of a received frame.
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RxFrameHeader {
    pub id: Id,
    /// Remote frame (classic CAN only).
    pub rtr: bool,
    pub frame_format: FrameFormat,
    /// Data phase was sent with bit rate switching.
    pub bit_rate_switching: bool,
    /// The transmitter was error passive (ESI bit).
    pub error_passive: bool,
    /// Data length from the DLC, in bytes.
    pub len: u8,
    /// The RX element was smaller than `len`; only the configured data size was stored and copied.
    pub truncated: bool,
    /// Raw RX timestamp (RXTS), see FEATURES.md S1–S4.
    pub timestamp: u16,
    /// Index of the matching filter element, `None` for accepted non-matching frames.
    pub filter_index: Option<u8>,
}

impl RxFrameHeader {
    pub(crate) fn from_element(r0: RxElementR0, r1: RxElementR1, truncated: bool) -> Self {
        let id = match r0.xtd() {
            // SAFETY: masked to 11 / 29 bits.
            Xtd::ElevenBits => {
                Id::Standard(unsafe { StandardId::new_unchecked(((r0.id() >> 18) & 0x7FF) as u16) })
            }
            Xtd::TwentyNineBits => {
                Id::Extended(unsafe { ExtendedId::new_unchecked(r0.id() & 0x1FFF_FFFF) })
            }
        };
        Self {
            id,
            rtr: r0.rtr(),
            frame_format: r1.fdf(),
            bit_rate_switching: r1.brs(),
            error_passive: r0.esi(),
            len: dlc_to_len(r1.dlc(), r1.fdf()),
            truncated,
            timestamp: r1.rxts(),
            filter_index: if r1.anmf() { None } else { Some(r1.fidx()) },
        }
    }
}

impl<M: Receive> FdCan<M> {
    /// Takes the oldest frame out of an RX FIFO and copies its data into `buf`.
    ///
    /// Returns `Ok(None)` if the FIFO is empty, and the header plus the number of bytes written to `buf`
    /// otherwise (`min(len, element data size)`). If `buf` is too short, returns [Error::BufferTooSmall] and
    /// leaves the frame in the FIFO.
    ///
    /// Reads the element at RXFxS.FxGI and acknowledges it via RXFxA.FxAI, which frees it and advances the
    /// get index (RM0468 FDCAN "Rx FIFOs", Bosch M_CAN user manual "Rx FIFOs").
    pub fn receive_fifo(
        &mut self,
        fifo: RxFifo,
        buf: &mut [u8],
    ) -> Result<Option<(RxFrameHeader, usize)>, Error> {
        let status = self.can.rxfs(fifo.nr()).read();
        if status.ffl() == 0 {
            return Ok(None);
        }
        let idx = status.fgi();
        let (r0, r1, copied, truncated) =
            self.message_ram().read_rx_fifo_element(fifo, idx, buf)?;
        self.can.rxfa(fifo.nr()).write(|w| w.set_fai(idx));
        Ok(Some((
            RxFrameHeader::from_element(r0, r1, truncated),
            copied as usize,
        )))
    }

    /// Number of frames waiting in an RX FIFO.
    #[inline]
    pub fn rx_fifo_fill_level(&self, fifo: RxFifo) -> u8 {
        self.can.rxfs(fifo.nr()).read().ffl()
    }

    /// Returns whether a frame was lost because the RX FIFO was full (IR.RFxL) and clears the flag. Same as
    /// [FdCan::take_interrupt_flags] with [Interrupts::rx_fifo_message_lost](crate::Interrupts::rx_fifo_message_lost).
    #[inline]
    pub fn take_rx_fifo_message_lost(&mut self, fifo: RxFifo) -> bool {
        // Also sees the flag if the interrupt handler cleared it (RFxL enabled as an interrupt).
        !self
            .take_interrupt_flags(crate::Interrupts::rx_fifo_message_lost(fifo))
            .is_empty()
    }
}

#[cfg(all(test, feature = "h7"))]
mod tests {
    use super::*;

    #[test]
    fn rx_header_standard_id() {
        let r0 = RxElementR0::from_bits(0x125 << 18);
        let r1 = RxElementR1::from_bits(3 << 24 | 8 << 16 | 0x1234);
        let h = RxFrameHeader::from_element(r0, r1, false);
        assert_eq!(h.id, Id::Standard(StandardId::new(0x125).unwrap()));
        assert!(!h.rtr && !h.error_passive && !h.bit_rate_switching && !h.truncated);
        assert!(matches!(h.frame_format, FrameFormat::Classic));
        assert_eq!(h.len, 8);
        assert_eq!(h.timestamp, 0x1234);
        assert_eq!(h.filter_index, Some(3));
    }

    #[test]
    fn rx_header_extended_fd() {
        let r0 = RxElementR0::from_bits(1 << 31 | 1 << 30 | 0x1FFF_FFFF);
        let r1 = RxElementR1::from_bits(1 << 31 | 1 << 21 | 1 << 20 | 15 << 16);
        let h = RxFrameHeader::from_element(r0, r1, true);
        assert_eq!(h.id, Id::Extended(ExtendedId::MAX));
        assert!(h.error_passive && h.bit_rate_switching && h.truncated);
        assert!(matches!(h.frame_format, FrameFormat::FD));
        assert_eq!(h.len, 64);
        assert_eq!(h.filter_index, None);
    }
}
