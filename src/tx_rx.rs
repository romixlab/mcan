use crate::Id;
use crate::fdcan::Transmit;
use crate::message_ram_layout::TxBufferIdx;
use crate::pac::message_ram::{Esi, FrameFormat};
use crate::util::checked_wait;
use crate::{Error, FdCan};

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
    #[inline]
    pub fn write_tx_buffer_pend(
        &mut self,
        idx: TxBufferIdx,
        tx_header: TxFrameHeader,
        data: &[u8],
    ) -> Result<(), Error> {
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
            let word = if chunk.len() == 4 {
                let word: [u8; 4] = chunk.try_into().expect("length is 4");
                u32::from_le_bytes(word)
            } else {
                let mut word = [0u8; 4];
                word[..chunk.len()].copy_from_slice(chunk);
                u32::from_le_bytes(word)
            };
            *d = word;
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
    fn abort(&mut self, idx: TxBufferIdx) -> Result<bool, Error> {
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

    /// Clears the transmission complete flag.
    #[inline]
    pub fn clear_transmission_completed_flag(&mut self) {
        self.can.ir().write(|w| w.set_tc(true));
    }

    /// Clears the transmission cancelled flag.
    #[inline]
    pub fn clear_transmission_cancelled_flag(&mut self) {
        self.can.ir().write(|w| w.set_tcf(true));
    }

    // Returns a received frame if available.
    //
    // Returns `Err` when a frame was lost due to buffer overrun.
    //
    // # Panics
    //
    // Panics if `buffer` is smaller than the header length.
    // pub fn try_receive_any(
    //     &mut self,
    //     buffer: &mut [u8],
    // ) -> nb::Result<ReceiveOverrun<RxFrameInfo>, Infallible> {
    //     if !self.rx_fifo_is_empty() {
    //         let mbox = self.get_rx_mailbox();
    //         let idx: usize = mbox.into();
    //         let mailbox: &RxFifoElement = &self.rx_msg_ram().fxsa[idx];
    //
    //         let header: RxFrameInfo = (&mailbox.header).into();
    //         for (i, register) in mailbox.data.iter().enumerate() {
    //             let register_value = register.read();
    //             let register_bytes =
    //                 unsafe { slice::from_raw_parts(&register_value as *const u32 as *const u8, 4) };
    //             let num_bytes = (header.len as usize) - i * 4;
    //             if num_bytes <= 4 {
    //                 buffer[i * 4..i * 4 + num_bytes].copy_from_slice(&register_bytes[..num_bytes]);
    //                 break;
    //             }
    //             buffer[i * 4..(i + 1) * 4].copy_from_slice(register_bytes);
    //         }
    //         self.release_mailbox(mbox);
    //
    //         if self.has_overrun() {
    //             Ok(ReceiveOverrun::<RxFrameInfo>::Overrun(header))
    //         } else {
    //             Ok(ReceiveOverrun::<RxFrameInfo>::NoOverrun(header))
    //         }
    //     } else {
    //         Err(nb::Error::WouldBlock)
    //     }
    // }
    //
    // #[inline]
    // fn has_overrun(&self, fifo_nr: FIFONr) -> bool {
    //     self.can.rxfs(fifo_nr.nr()).read().rfl()
    // }

    // Returns if the fifo contains any new messages.
    // #[inline]
    // pub fn rx_fifo_is_empty(&self) -> bool {
    //     let can = self.registers();
    //     match FIFONR::NR {
    //         0 => can.rxf0s.read().f0fl().bits() == 0,
    //         1 => can.rxf1s.read().f1fl().bits() == 0,
    //         _ => unreachable!(),
    //     }
    // }

    // #[inline]
    // fn release_mailbox(&mut self, idx: Mailbox) {
    //     unsafe {
    //         (*I::MSG_RAM).receive[FIFONR::NR].fxsa[idx as u8 as usize].reset();
    //     }
    //
    //     let can = self.registers();
    //     match FIFONR::NR {
    //         0 => can.rxf0a.write(|w| unsafe { w.f0ai().bits(idx.into()) }),
    //         1 => can.rxf1a.write(|w| unsafe { w.f1ai().bits(idx.into()) }),
    //         _ => unreachable!(),
    //     }
    // }

    // #[inline]
    // fn get_rx_mailbox(&self) -> Mailbox {
    //     let can = self.registers();
    //     let idx = match FIFONR::NR {
    //         0 => can.rxf0s.read().f0gi().bits(),
    //         1 => can.rxf1s.read().f1gi().bits(),
    //         _ => unreachable!(),
    //     };
    //     Mailbox::new(idx)
    // }
}
