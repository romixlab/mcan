//! Internal loopback smoke test: sends frames from a dedicated TX buffer and reads them back from RX FIFO0.
//! Needs only the board and a probe (B135A/B, B125A/B): internal loopback doesn't drive the TX pin.
#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::pac::rcc::vals::{Pllm, Plln, Pllsrc};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, HseMode, Pll, PllDiv, SupplyConfig, Sysclk, VoltageScale,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, rcc};
use embassy_time::{Duration, Instant, Timer};
use mcan::MessageRamBuilder;
use mcan::pac::message_ram::{Esi, FrameFormat};
use mcan::{
    DataFieldSize, ExtendedId, Id, MessageRamBuilderError, MessageRamLayout, NominalBitTiming,
    RamBuilderInitialState, RxFifo, StandardId, TxBufferIdx, TxFrameHeader,
};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hse = Some(rcc::Hse {
        freq: Hertz::mhz(24),
        mode: HseMode::Bypass,
    });
    config.rcc.pll1 = Some(Pll {
        source: Pllsrc::HSE,
        prediv: Pllm::DIV12,
        mul: Plln::MUL128,
        divp: Some(PllDiv::DIV2),
        divq: Some(PllDiv::DIV4), // 64 MHz FDCAN kernel clock
        divr: None,
    });
    config.rcc.voltage_scale = VoltageScale::Scale0;
    config.rcc.supply_config = SupplyConfig::DirectSMPS;
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV2;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
    let _p = embassy_stm32::init(config);

    let (mut can_instances, builder) = unwrap!(mcan::FdCanInstances::new());
    let (layout, tx) = unwrap!(layout_fdcan_ram(builder));
    let can = unwrap!(can_instances.take_enabled(mcan::FdCanInstance::FdCan1));

    let mut can = unwrap!(can.into_config_mode());
    // 64 MHz / 64 quanta = 1 Mbit/s
    can.set_nominal_bit_timing(unwrap!(NominalBitTiming::new(1, 55, 8, 1)));
    unwrap!(can.set_layout(layout));
    let mut can = unwrap!(can.into_internal_loopback());

    let ids = [
        Id::Standard(StandardId::ZERO),
        Id::Standard(unwrap!(StandardId::new(0x125))),
        Id::Standard(StandardId::MAX),
        Id::Extended(ExtendedId::ZERO),
        Id::Extended(unwrap!(ExtendedId::new(0x1234_5678))),
        Id::Extended(ExtendedId::MAX),
    ];
    let mut passed = 0;
    let mut failed = 0;
    for (n, id) in ids.iter().enumerate() {
        for len in [0usize, 1, 3, 8] {
            let data: [u8; 8] = core::array::from_fn(|i| (n * 16 + i) as u8 ^ len as u8);
            let header = TxFrameHeader {
                frame_format: FrameFormat::Classic,
                id: *id,
                bit_rate_switching: false,
                error_state: Esi::EsiDependsOnErrorPassive,
                marker: None,
            };
            unwrap!(can.write_tx_buffer_pend(tx, header, &data[..len]));

            let mut buf = [0u8; 64];
            let deadline = Instant::now() + Duration::from_millis(10);
            let received = loop {
                if let Some(r) = unwrap!(can.receive_fifo(RxFifo::Fifo0, &mut buf)) {
                    break Some(r);
                }
                if Instant::now() > deadline {
                    break None;
                }
                Timer::after_micros(50).await;
            };
            match received {
                Some((h, copied)) if h.id == *id && copied == len && buf[..len] == data[..len] => {
                    passed += 1
                }
                Some((h, copied)) => {
                    error!(
                        "mismatch: sent {} {=[u8]:x}, got {} {=[u8]:x}",
                        id,
                        &data[..len],
                        h,
                        &buf[..copied]
                    );
                    failed += 1;
                }
                None => {
                    error!("timeout: {} len {}", id, len);
                    failed += 1;
                }
            }
        }
    }
    if can.take_rx_fifo_message_lost(RxFifo::Fifo0) {
        error!("RX FIFO0 lost a frame");
        failed += 1;
    }
    if failed == 0 {
        info!("loopback PASS: {} frames", passed);
    } else {
        error!("loopback FAIL: {} passed, {} failed", passed, failed);
    }
    loop {
        Timer::after_secs(1).await;
    }
}

fn layout_fdcan_ram(
    builder: MessageRamBuilder<RamBuilderInitialState>,
) -> Result<(MessageRamLayout, TxBufferIdx), MessageRamBuilderError> {
    let builder = builder
        .allocate_11bit_filters(0)?
        .allocate_29bit_filters(0)?
        .allocate_rx_fifo0_buffers(4, DataFieldSize::_64Bytes)?
        .allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes)?
        .skip_dedicated_buffers()
        .allocate_tx_event_fifo_buffers(0)?
        .tx_buffer_element_size(DataFieldSize::_64Bytes);
    let (idx, builder) = builder.allocate_dedicated_tx_buffer()?;
    let (layout, _) = builder.allocate_fifo_or_queue(0)?.allocate_triggers(0)?;
    Ok((layout, idx))
}
