use crate::FdCan;

impl<M> FdCan<M> {
    /// Puts a CAN frame in a transmit mailbox for transmission on the bus.
    ///
    /// Frames are transmitted to the bus based on their priority (identifier). Transmit order is
    /// preserved for frames with identical identifiers.
    ///
    /// If all transmit mailboxes are full, a higher priority frame can replace a lower-priority
    /// frame, which is returned via the closure 'pending'. If 'pending' is called; it's return value
    /// is returned via `Option<P>`, if it is not, None is returned.
    /// If there are only higher priority frames in the queue, this returns Err::WouldBlock
    pub fn transmit(
        &mut self,
        frame: TxFrameHeader,
        buffer: &[u8],
    ) -> nb::Result<Option<()>, Infallible> {
        self.transmit_preserve(frame, buffer, &mut |_, _, _| ())
    }

    /// As Transmit, but if there is a pending frame, `pending` will be called so that the frame can
    /// be preserved.
    pub fn transmit_preserve<PTX, P>(
        &mut self,
        frame: TxFrameHeader,
        buffer: &[u8],
        pending: &mut PTX,
    ) -> nb::Result<Option<P>, Infallible>
    where
        PTX: FnMut(Mailbox, TxFrameHeader, &[u32]) -> P,
    {
        let can = self.registers();
        let queue_is_full = self.tx_queue_is_full();

        let id = frame.into();

        // If the queue is full,
        // Discard the first slot with a lower priority message
        let (idx, pending_frame) = if queue_is_full {
            if self.is_available(Mailbox::_0, id) {
                (
                    Mailbox::_0,
                    self.abort_pending_mailbox(Mailbox::_0, pending),
                )
            } else if self.is_available(Mailbox::_1, id) {
                (
                    Mailbox::_1,
                    self.abort_pending_mailbox(Mailbox::_1, pending),
                )
            } else if self.is_available(Mailbox::_2, id) {
                (
                    Mailbox::_2,
                    self.abort_pending_mailbox(Mailbox::_2, pending),
                )
            } else {
                // For now we bail when there is no lower priority slot available
                // Can this lead to priority inversion?
                return Err(nb::Error::WouldBlock);
            }
        } else {
            // Read the Write Pointer
            let idx = can.txfqs.read().tfqpi().bits();

            (Mailbox::new(idx), None)
        };

        self.write_mailbox(idx, frame, buffer);

        Ok(pending_frame)
    }

    /// Returns if the tx queue is able to accept new messages without having to cancel an existing one
    #[inline]
    pub fn tx_queue_is_full(&self) -> bool {
        self.registers().txfqs.read().tfqf().bit()
    }

    /// Returns `Ok` when the mailbox is free or if it contains pending frame with a
    /// lower priority (higher ID) than the identifier `id`.
    #[inline]
    fn is_available(&self, idx: Mailbox, id: IdReg) -> bool {
        if self.has_pending_frame(idx) {
            //read back header section
            let header: TxFrameHeader = (&self.tx_msg_ram().tbsa[idx as usize].header).into();
            let old_id: IdReg = header.into();

            id > old_id
        } else {
            true
        }
    }

    #[inline]
    fn write_mailbox(&mut self, idx: Mailbox, tx_header: TxFrameHeader, buffer: &[u8]) {
        let tx_ram = self.tx_msg_ram_mut();

        let tx_element = &mut tx_ram.tbsa[idx as usize];

        // Clear mail slot; mainly for debugging purposes.
        tx_element.reset();
        tx_element.header.merge(tx_header);

        let mut lbuffer = [0_u32; 16];
        let data = unsafe {
            slice::from_raw_parts_mut(lbuffer.as_mut_ptr() as *mut u8, tx_header.len as usize)
        };
        data[..tx_header.len as usize].copy_from_slice(&buffer[..tx_header.len as usize]);
        let data_len = ((tx_header.len as usize) + 3) / 4;
        for (register, byte) in tx_element.data.iter_mut().zip(lbuffer[..data_len].iter()) {
            unsafe { register.write(*byte) };
        }

        // Set <idx as Mailbox> as ready to transmit
        self.registers()
            .txbar
            .modify(|r, w| unsafe { w.ar().bits(r.ar().bits() | 1 << (idx as u32)) });
    }

    #[inline]
    fn abort_pending_mailbox<PTX, R>(&mut self, idx: Mailbox, pending: PTX) -> Option<R>
    where
        PTX: FnOnce(Mailbox, TxFrameHeader, &[u32]) -> R,
    {
        if self.abort(idx) {
            let tx_ram = self.tx_msg_ram();

            //read back header section
            let header = (&tx_ram.tbsa[idx as usize].header).into();
            let mut data = [0u32; 16];
            for (byte, register) in data.iter_mut().zip(tx_ram.tbsa[idx as usize].data.iter()) {
                *byte = register.read();
            }
            Some(pending(idx, header, &data))
        } else {
            // Abort request failed because the frame was already sent (or being sent) on
            // the bus. All mailboxes are now free. This can happen for small prescaler
            // values (e.g. 1MBit/s bit timing with a source clock of 8MHz) or when an ISR
            // has preempted the execution.
            None
        }
    }

    /// Attempts to abort the sending of a frame that is pending in a mailbox.
    ///
    /// If there is no frame in the provided mailbox, or its transmission succeeds before it can be
    /// aborted, this function has no effect and returns `false`.
    ///
    /// If there is a frame in the provided mailbox, and it is canceled successfully, this function
    /// returns `true`.
    #[inline]
    fn abort(&mut self, idx: Mailbox) -> bool {
        let can = self.registers();

        // Check if there is a request pending to abort
        if self.has_pending_frame(idx) {
            let idx: u8 = idx.into();
            let idx: u32 = 1u32 << (idx as u32);

            // Abort Request
            can.txbcr.write(|w| unsafe { w.cr().bits(idx) });

            // Wait for the abort request to be finished.
            loop {
                if can.txbcf.read().cf().bits() & idx != 0 {
                    // Return false when a transmission has occured
                    break can.txbto.read().to().bits() & idx == 0;
                }
            }
        } else {
            false
        }
    }

    #[inline]
    fn has_pending_frame(&self, idx: Mailbox) -> bool {
        let can = self.registers();
        let idx: u8 = idx.into();
        let idx: u32 = 1u32 << (idx as u32);

        can.txbrp.read().trp().bits() & idx != 0
    }

    /// Returns `true` if no frame is pending for transmission.
    #[inline]
    pub fn is_idle(&self) -> bool {
        let can = self.registers();
        can.txbrp.read().trp().bits() == 0x0
    }

    /// Clears the transmission complete flag.
    #[inline]
    pub fn clear_transmission_completed_flag(&mut self) {
        let can = self.registers();
        can.ir.write(|w| w.tc().set_bit());
    }

    /// Clears the transmission cancelled flag.
    #[inline]
    pub fn clear_transmission_cancelled_flag(&mut self) {
        let can = self.registers();
        can.ir.write(|w| w.tcf().set_bit());
    }

    /// Returns a received frame if available.
    ///
    /// Returns `Err` when a frame was lost due to buffer overrun.
    ///
    /// # Panics
    ///
    /// Panics if `buffer` is smaller than the header length.
    pub fn receive(
        &mut self,
        buffer: &mut [u8],
    ) -> nb::Result<ReceiveOverrun<RxFrameInfo>, Infallible> {
        if !self.rx_fifo_is_empty() {
            let mbox = self.get_rx_mailbox();
            let idx: usize = mbox.into();
            let mailbox: &RxFifoElement = &self.rx_msg_ram().fxsa[idx];

            let header: RxFrameInfo = (&mailbox.header).into();
            for (i, register) in mailbox.data.iter().enumerate() {
                let register_value = register.read();
                let register_bytes =
                    unsafe { slice::from_raw_parts(&register_value as *const u32 as *const u8, 4) };
                let num_bytes = (header.len as usize) - i * 4;
                if num_bytes <= 4 {
                    buffer[i * 4..i * 4 + num_bytes].copy_from_slice(&register_bytes[..num_bytes]);
                    break;
                }
                buffer[i * 4..(i + 1) * 4].copy_from_slice(register_bytes);
            }
            self.release_mailbox(mbox);

            if self.has_overrun() {
                Ok(ReceiveOverrun::<RxFrameInfo>::Overrun(header))
            } else {
                Ok(ReceiveOverrun::<RxFrameInfo>::NoOverrun(header))
            }
        } else {
            Err(nb::Error::WouldBlock)
        }
    }

    #[inline]
    fn registers(&self) -> &RegisterBlock {
        unsafe { &*I::REGISTERS }
    }

    #[inline]
    fn rx_msg_ram(&self) -> &message_ram::Receive {
        unsafe { &(&(*I::MSG_RAM).receive)[FIFONR::NR] }
    }

    #[inline]
    fn has_overrun(&self) -> bool {
        let can = self.registers();
        match FIFONR::NR {
            0 => can.rxf0s.read().rf0l().bit(),
            1 => can.rxf1s.read().rf1l().bit(),
            _ => unreachable!(),
        }
    }

    /// Returns if the fifo contains any new messages.
    #[inline]
    pub fn rx_fifo_is_empty(&self) -> bool {
        let can = self.registers();
        match FIFONR::NR {
            0 => can.rxf0s.read().f0fl().bits() == 0,
            1 => can.rxf1s.read().f1fl().bits() == 0,
            _ => unreachable!(),
        }
    }

    #[inline]
    fn release_mailbox(&mut self, idx: Mailbox) {
        unsafe {
            (*I::MSG_RAM).receive[FIFONR::NR].fxsa[idx as u8 as usize].reset();
        }

        let can = self.registers();
        match FIFONR::NR {
            0 => can.rxf0a.write(|w| unsafe { w.f0ai().bits(idx.into()) }),
            1 => can.rxf1a.write(|w| unsafe { w.f1ai().bits(idx.into()) }),
            _ => unreachable!(),
        }
    }

    #[inline]
    fn get_rx_mailbox(&self) -> Mailbox {
        let can = self.registers();
        let idx = match FIFONR::NR {
            0 => can.rxf0s.read().f0gi().bits(),
            1 => can.rxf1s.read().f1gi().bits(),
            _ => unreachable!(),
        };
        Mailbox::new(idx)
    }
}
