//! Internal loopback on FDCAN1 (FEATURES.md R5, Q4): the lite TX FIFO and RX FIFOs without a second node.
//! Internal loopback doesn't drive the TX pin, so the bus may stay connected.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use embassy_time::{Duration, block_for};
    use hil_b129::{Board, classic, pattern};
    use mcan::config::FrameTransmissionConfig;
    use mcan::pac::message_ram::FrameFormat;
    use mcan::{Error, ExtendedId, FdCan, Id, InternalLoopbackMode, RxFifo, StandardId, TxFrameHeader};

    fn ids() -> [Id; 5] {
        // SAFETY: values are in range.
        unsafe {
            [
                Id::Standard(StandardId::new_unchecked(0)),
                Id::Standard(StandardId::new_unchecked(0x123)),
                Id::Standard(StandardId::new_unchecked(0x7FF)),
                Id::Extended(ExtendedId::new_unchecked(0x1234_5678)),
                Id::Extended(ExtendedId::new_unchecked(0x1FFF_FFFF)),
            ]
        }
    }

    fn loopback(board: &mut Board, fd: bool) -> FdCan<InternalLoopbackMode> {
        let mut can = board.fdcan1_config();
        if fd {
            can.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS);
        }
        defmt::unwrap!(can.into_internal_loopback())
    }

    /// Waits until a frame arrives in FIFO0 (loopback frame takes well under a millisecond).
    fn receive(
        can: &mut FdCan<InternalLoopbackMode>,
        buf: &mut [u8],
    ) -> (mcan::RxFrameHeader, usize) {
        for _ in 0..100 {
            if let Some(r) = defmt::unwrap!(can.receive_fifo(RxFifo::Fifo0, buf)) {
                return r;
            }
            block_for(Duration::from_micros(50));
        }
        defmt::panic!("no frame received");
    }

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// Every ID class and 0..=8 bytes, classic CAN.
    #[test]
    fn classic_ids_and_lengths(mut board: Board) {
        let mut can = loopback(&mut board, false);
        let mut n = 0;
        for id in ids() {
            for len in 0..=8 {
                let data = pattern(n, len);
                defmt::unwrap!(can.transmit(classic(id), &data[..len]));
                let mut buf = [0u8; 8];
                let (h, copied) = receive(&mut can, &mut buf);
                defmt::assert_eq!(h.id, id);
                defmt::assert_eq!(h.len as usize, len);
                defmt::assert_eq!(copied, len);
                defmt::assert!(matches!(h.frame_format, FrameFormat::Classic));
                defmt::assert!(!h.rtr && !h.bit_rate_switching && !h.truncated);
                defmt::assert_eq!(buf[..len], data[..len]);
                n += 1;
            }
        }
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0));
    }

    /// FD lengths with and without BRS.
    #[test]
    fn fd_lengths_with_and_without_brs(mut board: Board) {
        let mut can = loopback(&mut board, true);
        let mut n = 0;
        for brs in [false, true] {
            for len in [0usize, 1, 7, 8, 12, 16, 20, 24, 32, 48, 64] {
                let data = pattern(n, len);
                let mut h = TxFrameHeader::fd_brs(ids()[1]);
                h.bit_rate_switching = brs;
                defmt::unwrap!(can.transmit(h, &data[..len]));
                let mut buf = [0u8; 64];
                let (rh, copied) = receive(&mut can, &mut buf);
                defmt::assert_eq!(rh.len as usize, len);
                defmt::assert_eq!(copied, len);
                defmt::assert!(matches!(rh.frame_format, FrameFormat::FD));
                defmt::assert_eq!(rh.bit_rate_switching, brs);
                defmt::assert_eq!(buf[..len], data[..len]);
                n += 1;
            }
        }
    }

    /// 3 frames queued back to back come out in order.
    #[test]
    fn tx_fifo_order(mut board: Board) {
        let mut can = loopback(&mut board, false);
        for i in 0..3u8 {
            defmt::unwrap!(can.transmit(classic(ids()[1]), &[i]));
        }
        // TxQueueFull can't be asserted here: in loopback the first frame leaves within ~100 µs. It is tested on
        // a bus without ACK, where frames stay pending (Q5a).
        for i in 0..3u8 {
            let mut buf = [0u8; 8];
            let (_, n) = receive(&mut can, &mut buf);
            defmt::assert_eq!((n, buf[0]), (1, i));
        }
        defmt::assert!(can.is_idle());
    }

    /// Wrong data length is rejected without touching the FIFO.
    #[test]
    fn wrong_length(mut board: Board) {
        let mut can = loopback(&mut board, false);
        defmt::assert!(matches!(
            can.transmit(classic(ids()[0]), &[0; 9]),
            Err(Error::WrongDataSize)
        ));
        defmt::assert!(can.is_idle());
    }
}
