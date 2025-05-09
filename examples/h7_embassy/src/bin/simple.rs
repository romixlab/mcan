#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(impl_trait_in_assoc_type)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::gpio::{AfType, Flex, Level, Output, OutputType, Pull, Speed};
use embassy_stm32::pac::rcc::vals::{Pllm, Plln, Pllq, Pllr, Pllsrc};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{AHBPrescaler, APBPrescaler, HseMode, Pll, PllQDiv, PllRDiv, Sysclk};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, rcc};
use embassy_time::Timer;
use mcan::DataFieldSize;
use mcan::{ElevenBitFilters, MessageRamBuilder, MessageRamBuilderError, MessageRamLayout};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hse = Some(rcc::Hse {
        freq: Hertz::mhz(12),
        mode: HseMode::Oscillator,
    });
    config.rcc.pll = Some(Pll {
        source: Pllsrc::HSE,
        prediv: Pllm::DIV1,
        mul: Plln::MUL8,
        divp: None,
        divq: Some(Pllq::DIV2),
        divr: Some(Pllr::DIV8),
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.ahb_pre = AHBPrescaler::DIV2;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
    let p = embassy_stm32::init(config);

    info!("Hello World!");

    let mut led = Output::new(p.PB14, Level::High, Speed::Low);

    let tx = p.PD1;
    let rx = p.PB8;

    let af_tx = embassy_stm32::can::TxPin::af_num(&can_tx);
    let mut can_tx = Flex::new(can_tx);
    can_tx.set_as_af_unchecked(af_tx, AfType::output(OutputType::PushPull, Speed::VeryHigh));
    core::mem::forget(can_tx);

    let af_rx = embassy_stm32::can::RxPin::af_num(&can_rx);
    let mut can_rx = Flex::new(can_rx);
    can_rx.set_as_af_unchecked(af_rx, AfType::input(Pull::None));
    core::mem::forget(can_rx);

    let builder = unwrap!(mcan::message_ram::message_ram_builder());
    let (layout_fdcan1, _builder) = unwrap!(layout_fdcan_ram(builder));

    let can = unwrap!(mcan::new_fdcan1());
    let mut can = unwrap!(can.into_config_mode().map_err(|(e, _)| e));
    can.set_layout(layout_fdcan1);

    let can = unwrap!(can.into_internal_loopback().map_err(|(e, _)| e));

    loop {
        led.set_high();
        Timer::after_millis(3).await;

        led.set_low();
        Timer::after_millis(1000).await;
    }
}

fn layout_fdcan_ram(
    builder: MessageRamBuilder<ElevenBitFilters>,
) -> Result<(MessageRamLayout, MessageRamBuilder<ElevenBitFilters>), MessageRamBuilderError> {
    let (layout, builder) = builder
        .allocate_11bit_filters(28)?
        .allocate_29bit_filters(3)?
        .allocate_rx_fifo0_buffers(3, DataFieldSize::_64Bytes)?
        .allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes)?
        .allocate_tx_event_fifo_buffers(3)?
        .allocate_tx_buffers(1, 2, DataFieldSize::_64Bytes)?;
    Ok((layout, builder))
}
