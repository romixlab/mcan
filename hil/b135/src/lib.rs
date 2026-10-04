//! B135A/B board support for the HIL tests: clocks, FDCAN1 bring-up and blocking TX/RX helpers.
//! Pinout and hardware notes: boards/b135.md.
#![no_std]

use defmt_rtt as _;

// Cargo unifies features, so `cnt/disabled` enabled by any crate in the build compiles out the driver's counters
// too. `Counters::get` then returns 0 and "no lost frames / no errors" asserts would pass without checking
// anything (FEATURES.md Q9).
const _: () = assert!(!cnt::DISABLED, "HIL tests need cnt counters, don't enable cnt's `disabled` feature");
use embassy_stm32::pac::rcc::vals::{Pllm, Plln, Pllsrc};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, HseMode, Pll, PllDiv, SupplyConfig, Sysclk, VoltageScale,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, rcc};
use embassy_time::{Duration, Instant};
use mcan::fdcan::Receive;
use mcan::pac::message_ram::{Esi, FrameFormat};
use mcan::{
    ConfigMode, DataBitTiming, DataFieldSize, ExtendedId, FdCan, FdCanInstance, FdCanInstances, Id,
    MessageRamBuilder, MessageRamLayout, NominalBitTiming, RamBuilderInitialState, RxFifo,
    RxFrameHeader, StandardId, TxBufferIdx, TxFrameHeader,
};

/// FDCAN kernel clock: 24 MHz HSE bypass / 12 * 128 / 4 (PLL1_Q).
pub const FDCAN_KERNEL_CLOCK_HZ: u32 = 64_000_000;

/// Clocks used by every test. Also starts the embassy time driver.
pub fn init_clocks() -> embassy_stm32::Peripherals {
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
    config.rcc.voltage_scale = VoltageScale::Scale0;
    config.rcc.supply_config = SupplyConfig::DirectSMPS;
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV2;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
    embassy_stm32::init(config)
}

/// State handed to every test: clocks are up, FDCAN instances and the RAM builder are untouched.
pub struct Board {
    pub peripherals: embassy_stm32::Peripherals,
    pub instances: FdCanInstances,
    /// Taken by the first layout a test builds.
    pub builder: Option<MessageRamBuilder<RamBuilderInitialState>>,
}

impl Board {
    pub fn init() -> Self {
        let peripherals = init_clocks();
        let (instances, builder) = defmt::unwrap!(FdCanInstances::new());
        Self {
            peripherals,
            instances,
            builder: Some(builder),
        }
    }
}

impl Board {
    /// Connects FDCAN1 to its pins (TX PB9, RX PB8, boards/b135.md). Only needed when the core uses the pins,
    /// not in internal loopback. TX is recessive while the instance is in Config mode.
    pub fn connect_fdcan1_pins(&mut self) {
        mcan::embassy::configure_pins!(
            tx: self.peripherals.PB9.reborrow(),
            rx: self.peripherals.PB8.reborrow()
        );
    }
}

/// Generous upper bound for one frame at the bit rates used here.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(10);

/// Writes a dedicated TX buffer, requests transmission and waits until nothing is pending any more.
pub fn send<M: mcan::fdcan::Transmit>(
    can: &mut FdCan<M>,
    idx: TxBufferIdx,
    header: TxFrameHeader,
    data: &[u8],
) {
    defmt::unwrap!(can.write_tx_buffer_pend(idx, header, data));
    // TXBRP shows the request only a few kernel clock cycles after the TXBAR write (FEATURES.md X1).
    embassy_time::block_for(Duration::from_micros(1));
    let deadline = Instant::now() + FRAME_TIMEOUT;
    while !can.is_idle() {
        defmt::assert!(Instant::now() < deadline, "TX did not complete");
    }
}

/// Waits for a frame in `fifo` and returns its header and the number of bytes copied into `buf`.
pub fn receive<M: Receive>(
    can: &mut FdCan<M>,
    fifo: RxFifo,
    buf: &mut [u8],
) -> (RxFrameHeader, usize) {
    let deadline = Instant::now() + FRAME_TIMEOUT;
    loop {
        if let Some(r) = defmt::unwrap!(can.receive_fifo(fifo, buf)) {
            return r;
        }
        defmt::assert!(Instant::now() < deadline, "no frame received");
    }
}

/// Like [receive], with the caller's timeout (a frame from another board may take longer to arrive).
pub fn receive_before<M: Receive>(
    can: &mut FdCan<M>,
    fifo: RxFifo,
    buf: &mut [u8],
    timeout: Duration,
) -> (RxFrameHeader, usize) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(r) = defmt::unwrap!(can.receive_fifo(fifo, buf)) {
            return r;
        }
        defmt::assert!(Instant::now() < deadline, "no frame received");
    }
}

/// Test payload: distinct per frame number and byte position.
pub fn pattern(frame: usize, len: usize) -> [u8; 64] {
    core::array::from_fn(|i| {
        if i < len {
            (frame * 31 + i * 7) as u8
        } else {
            0
        }
    })
}

/// Edge values of both ID classes.
pub const IDS: [Id; 6] = [
    Id::Standard(StandardId::ZERO),
    Id::Standard(StandardId::new(0x125).unwrap()),
    Id::Standard(StandardId::MAX),
    Id::Extended(ExtendedId::ZERO),
    Id::Extended(ExtendedId::new(0x1234_5678).unwrap()),
    Id::Extended(ExtendedId::MAX),
];

/// Classic CAN data frame header.
pub fn classic(id: Id) -> TxFrameHeader {
    TxFrameHeader {
        frame_format: FrameFormat::Classic,
        id,
        bit_rate_switching: false,
        error_state: Esi::EsiDependsOnErrorPassive,
        marker: None,
    }
}

/// One dedicated 64-byte TX buffer, FIFO0 / FIFO1 with the given (length, element size).
pub fn layout(
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

/// FDCAN1 in Config mode at 1 Mbit/s nominal and 4 Mbit/s data (64 MHz kernel clock), with `layout`.
pub fn fdcan1_config(board: &mut Board, layout: MessageRamLayout) -> FdCan<ConfigMode> {
    let can = defmt::unwrap!(board.instances.take_enabled(FdCanInstance::FdCan1));
    let mut can = defmt::unwrap!(can.into_config_mode());
    can.set_nominal_bit_timing(defmt::unwrap!(NominalBitTiming::new(1, 47, 16, 16)));
    can.set_data_bit_timing(defmt::unwrap!(DataBitTiming::new(1, 11, 4, 4)));
    defmt::unwrap!(can.set_layout(layout));
    can
}

/// Runs `fut` to completion, polling it again only after it was woken, and gives up after `timeout`.
///
/// Busy-polling would hide a missing wake-up; this way a test only passes if the interrupt handler (or
/// whatever the future waits for) actually wakes it.
pub fn block_on_woken<F: core::future::Future>(timeout: Duration, fut: F) -> Option<F::Output> {
    use core::sync::atomic::{AtomicBool, Ordering};
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    static WOKEN: AtomicBool = AtomicBool::new(true);
    fn raw() -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| raw(),
        |_| WOKEN.store(true, Ordering::Release),
        |_| WOKEN.store(true, Ordering::Release),
        |_| {},
    );

    // SAFETY: the vtable functions ignore the data pointer.
    let waker = unsafe { Waker::from_raw(raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = core::pin::pin!(fut);
    let deadline = Instant::now() + timeout;
    WOKEN.store(true, Ordering::Release);
    while Instant::now() < deadline {
        if WOKEN.swap(false, Ordering::AcqRel)
            && let Poll::Ready(r) = fut.as_mut().poll(&mut cx)
        {
            return Some(r);
        }
    }
    None
}
