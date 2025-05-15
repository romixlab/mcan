#[macro_export]
macro_rules! configure_pins {
    (tx: $tx_pin:expr, rx: $rx_pin:expr) => {{
        use embassy_stm32::gpio::{AfType, Flex, Level, Output, OutputType, Pull, Speed};
        let af_tx = embassy_stm32::can::TxPin::af_num(&$tx_pin);
        let mut can_tx = Flex::new($tx_pin);
        can_tx.set_as_af_unchecked(af_tx, AfType::output(OutputType::PushPull, Speed::VeryHigh));
        core::mem::forget(can_tx);

        let af_rx = embassy_stm32::can::RxPin::af_num(&$rx_pin);
        let mut can_rx = Flex::new($rx_pin);
        can_rx.set_as_af_unchecked(af_rx, AfType::input(Pull::None));
        core::mem::forget(can_rx);
    }};
}
pub use configure_pins;
