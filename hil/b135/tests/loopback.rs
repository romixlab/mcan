//! Internal loopback on FDCAN1 (FEATURES.md Q4). Needs only the board and a probe: internal loopback doesn't
//! drive the TX pin, so the bus may stay connected.
#![no_std]
#![no_main]

#[cfg(test)]
#[embedded_test::tests]
mod tests {
    use hil_b135::{Board, pattern, receive, send};
    use mcan::config::{FrameTransmissionConfig, GlobalFilter, NonMatchingFilter};
    use mcan::fdcan::{Error, InternalLoopbackMode};
    use mcan::pac::message_ram::{Esi, FrameFormat};
    use mcan::{
        DataBitTiming, DataFieldSize, ExtendedId, FdCan, FdCanInstance, Id, MessageRamLayout,
        NominalBitTiming, RxFifo, StandardId, TxBufferIdx, TxFrameHeader,
    };

    const IDS: [Id; 6] = [
        Id::Standard(StandardId::ZERO),
        Id::Standard(StandardId::new(0x125).unwrap()),
        Id::Standard(StandardId::MAX),
        Id::Extended(ExtendedId::ZERO),
        Id::Extended(ExtendedId::new(0x1234_5678).unwrap()),
        Id::Extended(ExtendedId::MAX),
    ];

    fn classic(id: Id) -> TxFrameHeader {
        TxFrameHeader {
            frame_format: FrameFormat::Classic,
            id,
            bit_rate_switching: false,
            error_state: Esi::EsiDependsOnErrorPassive,
            marker: None,
        }
    }

    /// One dedicated 64-byte TX buffer, FIFO0 / FIFO1 with the given sizes.
    fn layout(
        board: &mut Board,
        fifo0: (u8, DataFieldSize),
        fifo1: (u8, DataFieldSize),
    ) -> (MessageRamLayout, TxBufferIdx) {
        let builder = defmt::unwrap!(board.builder.take());
        let b = defmt::unwrap!(
            builder
                .allocate_11bit_filters(0)
                .and_then(|b| b.allocate_29bit_filters(0))
                .and_then(|b| b.allocate_rx_fifo0_buffers(fifo0.0, fifo0.1))
                .and_then(|b| b.allocate_rx_fifo1_buffers(fifo1.0, fifo1.1))
                .map(|b| b.skip_dedicated_buffers())
                .and_then(|b| b.allocate_tx_event_fifo_buffers(0))
                .map(|b| b.tx_buffer_element_size(DataFieldSize::_64Bytes))
                .and_then(|b| b.allocate_dedicated_tx_buffer())
                .ok()
        );
        let (idx, b) = b;
        let (layout, _) = defmt::unwrap!(
            b.allocate_fifo_or_queue(0)
                .and_then(|b| b.allocate_triggers(0))
                .ok()
        );
        (layout, idx)
    }

    /// FDCAN1 at 1 Mbit/s nominal and 4 Mbit/s data (64 MHz kernel clock), internal loopback.
    fn loopback(
        board: &mut Board,
        layout: MessageRamLayout,
        configure: impl FnOnce(&mut FdCan<mcan::ConfigMode>),
    ) -> FdCan<InternalLoopbackMode> {
        let can = defmt::unwrap!(board.instances.take_enabled(FdCanInstance::FdCan1));
        let mut can = defmt::unwrap!(can.into_config_mode());
        can.set_nominal_bit_timing(defmt::unwrap!(NominalBitTiming::new(1, 47, 16, 16)));
        can.set_data_bit_timing(defmt::unwrap!(DataBitTiming::new(1, 11, 4, 4)));
        defmt::unwrap!(can.set_layout(layout));
        configure(&mut can);
        defmt::unwrap!(can.into_internal_loopback())
    }

    #[init]
    fn init() -> Board {
        Board::init()
    }

    /// Every ID class and edge value with 0..=8 data bytes, classic CAN (Y1, X1, R1, R6).
    #[test]
    fn classic_ids_and_lengths(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (4, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = loopback(&mut board, l, |_| {});
        let mut n = 0;
        for id in IDS {
            for len in 0..=8 {
                let data = pattern(n, len);
                send(&mut can, tx, classic(id), &data[..len]);
                let mut buf = [0u8; 8];
                let (h, copied) = receive(&mut can, RxFifo::Fifo0, &mut buf);
                defmt::assert_eq!(h.id, id);
                defmt::assert_eq!(h.len as usize, len);
                defmt::assert_eq!(copied, len);
                defmt::assert!(matches!(h.frame_format, FrameFormat::Classic));
                defmt::assert!(!h.rtr && !h.bit_rate_switching && !h.truncated);
                defmt::assert_eq!(h.filter_index, None); // no filters, accepted as non-matching
                defmt::assert_eq!(buf[..len], data[..len]);
                n += 1;
            }
        }
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0));
    }

    /// CAN FD frames of every FD length, with and without bit rate switching (X10, T1).
    #[test]
    fn fd_lengths_with_and_without_brs(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (4, DataFieldSize::_64Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = loopback(&mut board, l, |c| {
            c.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS)
        });
        let mut n = 0;
        for brs in [false, true] {
            for len in [0, 8, 12, 16, 20, 24, 32, 48, 64] {
                let id = IDS[n % IDS.len()];
                let mut header = TxFrameHeader::fd_brs(id);
                header.bit_rate_switching = brs;
                let data = pattern(n, len);
                send(&mut can, tx, header, &data[..len]);
                let mut buf = [0u8; 64];
                let (h, copied) = receive(&mut can, RxFifo::Fifo0, &mut buf);
                defmt::assert_eq!(h.id, id);
                defmt::assert!(matches!(h.frame_format, FrameFormat::FD));
                defmt::assert_eq!(h.bit_rate_switching, brs);
                defmt::assert_eq!((h.len as usize, copied), (len, len));
                defmt::assert_eq!(buf[..len], data[..len]);
                n += 1;
            }
        }
    }

    /// Non-matching frames routed to FIFO1 by the global filter (Y1 FIFO1 addressing, F1).
    #[test]
    fn global_filter_routes_to_fifo1(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (2, DataFieldSize::_8Bytes),
            (3, DataFieldSize::_12Bytes),
        );
        let mut can = loopback(&mut board, l, |c| {
            c.set_global_filter(GlobalFilter {
                handle_standard_frames: NonMatchingFilter::IntoRxFifo1,
                handle_extended_frames: NonMatchingFilter::IntoRxFifo1,
                reject_remote_standard_frames: false,
                reject_remote_extended_frames: false,
            })
        });
        for (n, id) in IDS.into_iter().enumerate() {
            let data = pattern(n, 8);
            send(&mut can, tx, classic(id), &data[..8]);
            let mut buf = [0u8; 8];
            let (h, copied) = receive(&mut can, RxFifo::Fifo1, &mut buf);
            defmt::assert_eq!((h.id, copied), (id, 8));
            defmt::assert_eq!(buf, data[..8]);
            defmt::assert_eq!(can.rx_fifo_fill_level(RxFifo::Fifo0), 0);
        }
    }

    /// A full FIFO (blocking mode, the reset default) drops the next frame and sets RFxL (Y6).
    #[test]
    fn full_fifo_loses_frame(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (4, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = loopback(&mut board, l, |_| {});
        for n in 0..5 {
            send(&mut can, tx, classic(IDS[n]), &pattern(n, 8)[..8]);
        }
        // Reception of the 5th frame ends shortly after TX completes.
        embassy_time::block_for(embassy_time::Duration::from_micros(200));
        defmt::assert_eq!(can.rx_fifo_fill_level(RxFifo::Fifo0), 4);
        defmt::assert!(can.take_rx_fifo_message_lost(RxFifo::Fifo0));
        defmt::assert!(!can.take_rx_fifo_message_lost(RxFifo::Fifo0)); // cleared
        // The first four frames are kept, in order.
        for (n, id) in IDS.into_iter().take(4).enumerate() {
            let mut buf = [0u8; 8];
            let (h, _) = receive(&mut can, RxFifo::Fifo0, &mut buf);
            defmt::assert_eq!(h.id, id);
            defmt::assert_eq!(buf, pattern(n, 8)[..8]);
        }
        defmt::assert!(defmt::unwrap!(can.receive_fifo(RxFifo::Fifo0, &mut [0; 8])).is_none());
    }

    /// A short buffer returns BufferTooSmall and leaves the frame in the FIFO.
    #[test]
    fn short_buffer_keeps_frame(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (2, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = loopback(&mut board, l, |_| {});
        let data = pattern(0, 8);
        send(&mut can, tx, classic(IDS[1]), &data[..8]);
        embassy_time::block_for(embassy_time::Duration::from_micros(200));
        let mut short = [0u8; 4];
        defmt::assert!(matches!(
            can.receive_fifo(RxFifo::Fifo0, &mut short),
            Err(Error::BufferTooSmall)
        ));
        defmt::assert_eq!(can.rx_fifo_fill_level(RxFifo::Fifo0), 1);
        let mut buf = [0u8; 8];
        let (h, copied) = receive(&mut can, RxFifo::Fifo0, &mut buf);
        defmt::assert_eq!((h.id, copied), (IDS[1], 8));
        defmt::assert_eq!(buf, data[..8]);
    }

    /// An RX element smaller than the frame stores only its data size; the header reports truncation.
    #[test]
    fn small_element_truncates(mut board: Board) {
        let (l, tx) = layout(
            &mut board,
            (2, DataFieldSize::_8Bytes),
            (0, DataFieldSize::_8Bytes),
        );
        let mut can = loopback(&mut board, l, |c| {
            c.set_frame_transmit(FrameTransmissionConfig::AllowFdCanAndBRS)
        });
        let data = pattern(3, 64);
        send(&mut can, tx, TxFrameHeader::fd_brs(IDS[4]), &data);
        let mut buf = [0u8; 64];
        let (h, copied) = receive(&mut can, RxFifo::Fifo0, &mut buf);
        defmt::assert_eq!((h.len, copied), (64, 8));
        defmt::assert!(h.truncated);
        defmt::assert_eq!(buf[..8], data[..8]);
    }
}
