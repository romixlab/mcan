#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

use core::num::{NonZeroU8, NonZeroU16};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{AfType, Flex, Level, Output, OutputType, Pull, Speed};
use embassy_stm32::pac::rcc::vals::{Pllm, Plln, Pllsrc};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, HseMode, Pll, PllDiv, SupplyConfig, Sysclk, VoltageScale,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, rcc};
use embassy_time::Timer;
use mcan::{DataFieldSize, Id, NominalBitTiming, StandardId, TxBufferIdx, TxFrameHeader};
use mcan::{MessageRamBuilder, MessageRamBuilderError, MessageRamLayout, RamBuilderInitialState};
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
        divq: Some(PllDiv::DIV4),
        divr: None,
    });
    config.rcc.voltage_scale = VoltageScale::Scale0; // TODO: adjust
    config.rcc.supply_config = SupplyConfig::DirectSMPS;
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV2;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
    let p = embassy_stm32::init(config);

    info!("Hello World!");

    let mut led = Output::new(p.PC7, Level::High, Speed::Low);

    let mut term1_en = Output::new(p.PC13, Level::Low, Speed::Low);
    term1_en.set_high();
    Timer::after_millis(50).await;
    term1_en.set_low();

    mcan::embassy::configure_pins!(tx: p.PB9, rx: p.PB8);

    let (mut can_instances, builder) = unwrap!(mcan::FdCanInstances::new());
    let (layout_fdcan1, _builder, tx_buffers) = unwrap!(layout_fdcan_ram(builder));
    let can = unwrap!(can_instances.take_enabled(mcan::FdCanInstance::FdCan1));

    let mut can = unwrap!(can.into_config_mode().map_err(|(e, _)| e));
    can.set_nominal_bit_timing(NominalBitTiming {
        prescaler: unwrap!(NonZeroU16::new(1)),
        seg1: unwrap!(NonZeroU8::new(55)),
        seg2: unwrap!(NonZeroU8::new(8)),
        sync_jump_width: unwrap!(NonZeroU8::new(1)),
    });
    debug!("layout: {:#?}", layout_fdcan1);
    can.set_layout(layout_fdcan1);

    let mut can = unwrap!(can.into_normal().map_err(|(e, _)| e));

    debug!("init done");

    loop {
        debug!("send");
        let r = can.write_tx_buffer_pend(
            tx_buffers.idx1,
            TxFrameHeader::fd_brs(Id::Standard(unwrap!(StandardId::new(0x123)))),
            &[0xAA, 0xBB, 0xCC],
        );
        unwrap!(r);

        led.set_high();
        Timer::after_millis(1000).await;

        led.set_low();
        Timer::after_millis(3).await;
    }
}

struct DedicatedTxBuffers {
    idx1: TxBufferIdx,
    idx2: TxBufferIdx,
    idx3: TxBufferIdx,
}

fn layout_fdcan_ram(
    builder: MessageRamBuilder<RamBuilderInitialState>,
) -> Result<
    (
        MessageRamLayout,
        MessageRamBuilder<RamBuilderInitialState>,
        DedicatedTxBuffers,
    ),
    MessageRamBuilderError,
> {
    let builder = builder
        .allocate_11bit_filters(3)?
        .allocate_29bit_filters(3)?
        .allocate_rx_fifo0_buffers(3, DataFieldSize::_64Bytes)?
        .allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes)?
        .allocate_rx_buffers(3, DataFieldSize::_64Bytes)?
        .allocate_tx_event_fifo_buffers(3)?
        .tx_buffer_element_size(DataFieldSize::_64Bytes);
    let (idx1, builder) = builder.allocate_dedicated_tx_buffer()?;
    let (idx2, builder) = builder.allocate_dedicated_tx_buffer()?;
    let (idx3, builder) = builder.allocate_dedicated_tx_buffer()?;
    let (layout, builder) = builder.allocate_fifo_or_queue(3)?.allocate_triggers(0)?;
    let tx_buffers = DedicatedTxBuffers { idx1, idx2, idx3 };
    Ok((layout, builder, tx_buffers))
}
