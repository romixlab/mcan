use crate::PoweredDownMode;
use crate::fdcan::{
    BusMonitoringMode, Error, ExternalLoopbackMode, NormalOperationMode, RestrictedOperationMode,
    TestMode,
};
use crate::fdcan::{ConfigMode, FdCan, InternalLoopbackMode, LoopbackMode};
#[cfg(feature = "h7")]
use crate::message_ram_layout::MessageRamLayout;
use crate::pac::fdcan::regs::Ils;

/// Why a [NominalBitTiming] or [DataBitTiming] was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BitTimingError {
    PrescalerOutOfRange,
    Seg1OutOfRange,
    Seg2OutOfRange,
    SyncJumpWidthOutOfRange,
    /// The synchronisation jump width must not exceed seg2 (ISO 11898-1: SJW <= phase segment 2).
    SyncJumpWidthLongerThanSeg2,
    /// Transceiver delay compensation needs a data prescaler of 1 or 2 (ST AN5348, Bosch M_CAN user manual,
    /// "Transmitter Delay Compensation").
    TdcPrescalerTooLarge,
    /// TDC offset or filter window above 127 minimum time quanta.
    TdcOutOfRange,
}

const fn in_range(value: u16, min: u16, max: u16) -> bool {
    value >= min && value <= max
}

/// Nominal (arbitration phase) bit timing, written to NBTP.
///
/// All values are actual lengths in time quanta (the register holds the value minus one). One bit is
/// `1 + seg1 + seg2` time quanta of `prescaler` kernel clock periods, and the sample point lies after
/// `1 + seg1` quanta. Ranges (RM0468 / RM0444 FDCAN_NBTP): prescaler 1..=512, seg1 2..=256,
/// seg2 1..=128, sync_jump_width 1..=128 and at most seg2.
///
/// <http://www.bittiming.can-wiki.info/> can compute values: enter the FDCAN kernel clock (not the CPU
/// clock) and use seg1 = Prop_Seg + Phase_Seg1, seg2 = Phase_Seg2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NominalBitTiming {
    prescaler: u16,
    seg1: u16,
    seg2: u8,
    sync_jump_width: u8,
}

impl NominalBitTiming {
    pub const fn new(
        prescaler: u16,
        seg1: u16,
        seg2: u8,
        sync_jump_width: u8,
    ) -> Result<Self, BitTimingError> {
        if !in_range(prescaler, 1, 512) {
            return Err(BitTimingError::PrescalerOutOfRange);
        }
        if !in_range(seg1, 2, 256) {
            return Err(BitTimingError::Seg1OutOfRange);
        }
        if !in_range(seg2 as u16, 1, 128) {
            return Err(BitTimingError::Seg2OutOfRange);
        }
        if !in_range(sync_jump_width as u16, 1, 128) {
            return Err(BitTimingError::SyncJumpWidthOutOfRange);
        }
        if sync_jump_width > seg2 {
            return Err(BitTimingError::SyncJumpWidthLongerThanSeg2);
        }
        Ok(Self {
            prescaler,
            seg1,
            seg2,
            sync_jump_width,
        })
    }

    pub const fn prescaler(&self) -> u16 {
        self.prescaler
    }
    pub const fn seg1(&self) -> u16 {
        self.seg1
    }
    pub const fn seg2(&self) -> u8 {
        self.seg2
    }
    pub const fn sync_jump_width(&self) -> u8 {
        self.sync_jump_width
    }

    /// Time quanta per bit.
    pub const fn quanta_per_bit(&self) -> u16 {
        1 + self.seg1 + self.seg2 as u16
    }

    /// NBTP register value. Validated in [new](Self::new), so the `- 1`s cannot underflow.
    pub(crate) fn nbtp(&self) -> crate::pac::fdcan::regs::Nbtp {
        let mut r = crate::pac::fdcan::regs::Nbtp(0);
        r.set_nbrp(self.prescaler - 1);
        r.set_ntseg1((self.seg1 - 1) as u8);
        r.set_ntseg2(self.seg2 - 1);
        r.set_nsjw(self.sync_jump_width - 1);
        r
    }
}

impl Default for NominalBitTiming {
    /// 500 kbit/s at an 8 MHz kernel clock (16 quanta, sample point 75 %). NBTP = 0x0600_0A03.
    #[inline]
    fn default() -> Self {
        Self {
            prescaler: 1,
            seg1: 11,
            seg2: 4,
            sync_jump_width: 4,
        }
    }
}

/// Transceiver delay compensation for the data phase (TDCR, DBTP.TDC).
///
/// At high data bit rates the transceiver loop delay exceeds the sample point, so the transmitter
/// samples its own bits at a secondary sample point: measured delay + `offset`. Both values are in
/// minimum time quanta (kernel clock periods), 0..=127. `filter_window` (TDCF) ignores dominant edges
/// shorter than it when measuring the delay, 0 disables it. See the Bosch M_CAN user manual,
/// "Transmitter Delay Compensation", and ST AN5348.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TransceiverDelayCompensation {
    offset: u8,
    filter_window: u8,
}

impl TransceiverDelayCompensation {
    pub const fn new(offset: u8, filter_window: u8) -> Result<Self, BitTimingError> {
        if offset > 127 || filter_window > 127 {
            return Err(BitTimingError::TdcOutOfRange);
        }
        Ok(Self {
            offset,
            filter_window,
        })
    }

    pub const fn offset(&self) -> u8 {
        self.offset
    }
    pub const fn filter_window(&self) -> u8 {
        self.filter_window
    }

    /// TDC with the secondary sample point at the data phase sample point, the usual choice
    /// (offset = prescaler * (1 + seg1)), no filter window.
    pub const fn at_sample_point(dbtr: &DataBitTiming) -> Result<Self, BitTimingError> {
        // prescaler <= 32, seg1 <= 32, no overflow.
        let offset = dbtr.prescaler as u16 * (1 + dbtr.seg1 as u16);
        if offset > 127 {
            return Err(BitTimingError::TdcOutOfRange);
        }
        Self::new(offset as u8, 0)
    }

    pub(crate) fn tdcr(&self) -> crate::pac::fdcan::regs::Tdcr {
        let mut r = crate::pac::fdcan::regs::Tdcr(0);
        r.set_tdco(self.offset);
        r.set_tdcf(self.filter_window);
        r
    }
}

/// Data phase bit timing for CAN FD with bit rate switching, written to DBTP (and TDCR).
/// Not used unless frame_transmit allows BRS.
///
/// Same conventions as [NominalBitTiming]. Ranges (RM0468 / RM0444 FDCAN_DBTP): prescaler 1..=32,
/// seg1 1..=32, seg2 1..=16, sync_jump_width 1..=16 and at most seg2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DataBitTiming {
    prescaler: u8,
    seg1: u8,
    seg2: u8,
    sync_jump_width: u8,
    tdc: Option<TransceiverDelayCompensation>,
}

impl DataBitTiming {
    pub const fn new(
        prescaler: u8,
        seg1: u8,
        seg2: u8,
        sync_jump_width: u8,
    ) -> Result<Self, BitTimingError> {
        if !in_range(prescaler as u16, 1, 32) {
            return Err(BitTimingError::PrescalerOutOfRange);
        }
        if !in_range(seg1 as u16, 1, 32) {
            return Err(BitTimingError::Seg1OutOfRange);
        }
        if !in_range(seg2 as u16, 1, 16) {
            return Err(BitTimingError::Seg2OutOfRange);
        }
        if !in_range(sync_jump_width as u16, 1, 16) {
            return Err(BitTimingError::SyncJumpWidthOutOfRange);
        }
        if sync_jump_width > seg2 {
            return Err(BitTimingError::SyncJumpWidthLongerThanSeg2);
        }
        Ok(Self {
            prescaler,
            seg1,
            seg2,
            sync_jump_width,
            tdc: None,
        })
    }

    /// Enables transceiver delay compensation. Requires a prescaler of 1 or 2.
    pub const fn with_tdc(
        mut self,
        tdc: TransceiverDelayCompensation,
    ) -> Result<Self, BitTimingError> {
        if self.prescaler > 2 {
            return Err(BitTimingError::TdcPrescalerTooLarge);
        }
        self.tdc = Some(tdc);
        Ok(self)
    }

    pub const fn prescaler(&self) -> u8 {
        self.prescaler
    }
    pub const fn seg1(&self) -> u8 {
        self.seg1
    }
    pub const fn seg2(&self) -> u8 {
        self.seg2
    }
    pub const fn sync_jump_width(&self) -> u8 {
        self.sync_jump_width
    }
    pub const fn tdc(&self) -> Option<TransceiverDelayCompensation> {
        self.tdc
    }

    /// Time quanta per bit.
    pub const fn quanta_per_bit(&self) -> u16 {
        1 + self.seg1 as u16 + self.seg2 as u16
    }

    /// DBTP register value. Validated in [new](Self::new), so the `- 1`s cannot underflow.
    pub(crate) fn dbtp(&self) -> crate::pac::fdcan::regs::Dbtp {
        let mut r = crate::pac::fdcan::regs::Dbtp(0);
        r.set_dbrp(self.prescaler - 1);
        r.set_dtseg1(self.seg1 - 1);
        r.set_dtseg2(self.seg2 - 1);
        r.set_dsjw(self.sync_jump_width - 1);
        r.set_tdc(self.tdc.is_some());
        r
    }
}

impl Default for DataBitTiming {
    /// 500 kbit/s at an 8 MHz kernel clock (16 quanta, sample point 75 %), no TDC. DBTP = 0x0000_0A33.
    #[inline]
    fn default() -> Self {
        Self {
            prescaler: 1,
            seg1: 11,
            seg2: 4,
            sync_jump_width: 4,
            tdc: None,
        }
    }
}

/// Configures which modes to use
/// Individual headers can contain a desire to be send via FdCan
/// or use Bit rate switching. But if this general setting does not allow
/// that, only classic CAN is used instead.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FrameTransmissionConfig {
    /// Only allow Classic CAN message Frames
    ClassicCanOnly,
    /// Allow (non-brs) FdCAN Message Frames
    AllowFdCan,
    /// Allow FdCAN Message Frames and allow Bit Rate Switching
    AllowFdCanAndBRS,
}

///
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ClockDivider {
    /// Divide by 1
    _1 = 0b0000,
    /// Divide by 2
    _2 = 0b0001,
    /// Divide by 4
    _4 = 0b0010,
    /// Divide by 6
    _6 = 0b0011,
    /// Divide by 8
    _8 = 0b0100,
    /// Divide by 10
    _10 = 0b0101,
    /// Divide by 12
    _12 = 0b0110,
    /// Divide by 14
    _14 = 0b0111,
    /// Divide by 16
    _16 = 0b1000,
    /// Divide by 18
    _18 = 0b1001,
    /// Divide by 20
    _20 = 0b1010,
    /// Divide by 22
    _22 = 0b1011,
    /// Divide by 24
    _24 = 0b1100,
    /// Divide by 26
    _26 = 0b1101,
    /// Divide by 28
    _28 = 0b1110,
    /// Divide by 30
    _30 = 0b1111,
}

/// Prescaler of the Timestamp counter
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TimestampPrescaler {
    /// 1
    _1 = 1,
    /// 2
    _2 = 2,
    /// 3
    _3 = 3,
    /// 4
    _4 = 4,
    /// 5
    _5 = 5,
    /// 6
    _6 = 6,
    /// 7
    _7 = 7,
    /// 8
    _8 = 8,
    /// 9
    _9 = 9,
    /// 10
    _10 = 10,
    /// 11
    _11 = 11,
    /// 12
    _12 = 12,
    /// 13
    _13 = 13,
    /// 14
    _14 = 14,
    /// 15
    _15 = 15,
    /// 16
    _16 = 16,
}

/// Selects the source of the Timestamp counter.
/// With CAN FD an external counter is required for timestamp generation (TSS = “10”) (Bosch MCAN: page 24)
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TimestampSource {
    /// The Timestamp counter is disabled
    None,
    /// Using the FdCan input clock as the Timstamp counter's source,
    /// and using a specific prescaler
    Prescaler(TimestampPrescaler),
    /// Using TIM3 as a source
    FromTIM3,
}

/// How to handle frames in the global filter
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum NonMatchingFilter {
    /// Frames will go to Fifo0 when they do no match any specific filter
    IntoRxFifo0 = 0b00,
    /// Frames will go to Fifo1 when they do no match any specific filter
    IntoRxFifo1 = 0b01,
    /// Frames will be rejected when they do not match any specific filter
    Reject = 0b11,
}

/// How to handle frames which do not match a specific filter
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct GlobalFilter {
    /// How to handle non-matching standard frames
    pub handle_standard_frames: NonMatchingFilter,

    /// How to handle non-matching extended frames
    pub handle_extended_frames: NonMatchingFilter,

    /// How to handle remote standard frames
    pub reject_remote_standard_frames: bool,

    /// How to handle remote extended frames
    pub reject_remote_extended_frames: bool,
}
impl GlobalFilter {
    /// Reject all non-matching and remote frames
    pub const fn reject_all() -> Self {
        Self {
            handle_standard_frames: NonMatchingFilter::Reject,
            handle_extended_frames: NonMatchingFilter::Reject,
            reject_remote_standard_frames: true,
            reject_remote_extended_frames: true,
        }
    }

    /// How to handle non-matching standard frames
    pub const fn set_handle_standard_frames(mut self, filter: NonMatchingFilter) -> Self {
        self.handle_standard_frames = filter;
        self
    }
    /// How to handle non-matching exteded frames
    pub const fn set_handle_extended_frames(mut self, filter: NonMatchingFilter) -> Self {
        self.handle_extended_frames = filter;
        self
    }
    /// How to handle remote standard frames
    pub const fn set_reject_remote_standard_frames(mut self, filter: bool) -> Self {
        self.reject_remote_standard_frames = filter;
        self
    }
    /// How to handle remote extended frames
    pub const fn set_reject_remote_extended_frames(mut self, filter: bool) -> Self {
        self.reject_remote_extended_frames = filter;
        self
    }
}
impl Default for GlobalFilter {
    #[inline]
    fn default() -> Self {
        Self {
            handle_standard_frames: NonMatchingFilter::IntoRxFifo0,
            handle_extended_frames: NonMatchingFilter::IntoRxFifo0,
            reject_remote_standard_frames: false,
            reject_remote_extended_frames: false,
        }
    }
}

/// FdCan Config Struct
#[derive(Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FdCanConfig {
    /// Nominal Bit Timings
    pub nbtr: NominalBitTiming,
    /// (Variable) Data Bit Timings
    pub dbtr: DataBitTiming,
    /// Enables or disables automatic retransmission of messages
    ///
    /// If this is enabled, the CAN peripheral will automatically try to retransmit each frame
    /// util it can be sent. Otherwise, it will try only once to send each frame.
    ///
    /// Automatic retransmission is enabled by default.
    pub automatic_retransmit: bool,
    /// Enabled or disables the pausing between transmissions
    ///
    /// This feature looses up burst transmissions coming from a single node and it protects against
    /// "babbling idiot" scenarios where the application program erroneously requests too many
    /// transmissions.
    pub transmit_pause: bool,
    /// Enabled or disables the pausing between transmissions
    ///
    /// This feature looses up burst transmissions coming from a single node and it protects against
    /// "babbling idiot" scenarios where the application program erroneously requests too many
    /// transmissions.
    pub frame_transmit: FrameTransmissionConfig,
    /// Non Isoe Mode
    /// If this is set, the FDCAN uses the CAN FD frame format as specified by the Bosch CAN
    /// FD Specification V1.0.
    pub non_iso_mode: bool,
    /// Edge Filtering: Two consecutive dominant tq required to detect an edge for hard synchronization
    pub edge_filtering: bool,
    /// Enables protocol exception handling
    pub protocol_exception_handling: bool,
    /// Sets the general clock divider for this FdCAN instance
    pub clock_divider: ClockDivider,
    /// This sets the interrupts for each interrupt line of the FdCan (FDCAN_INT0/1)
    /// Each interrupt set to 0 is set to line_0, each set to 1 is set to line_1.
    /// NOTE: This does not enable or disable the interrupt, but merely configure
    /// them to which interrupt the WOULD trigger if they are enabled.
    ///
    /// This is the raw ILS register: one bit per interrupt on the full core (H7), one bit per interrupt group
    /// on the lite cores (G0, G4, L5, H5).
    pub interrupt_line_config: Ils,
    /// Sets the timestamp source
    pub timestamp_source: TimestampSource,
    /// Configures the Global Filter
    pub global_filter: GlobalFilter,
    /// Configures RAM layout
    #[cfg(feature = "h7")]
    pub layout: MessageRamLayout,

    //#[cfg(not(feature = "embassy"))]
    /// How long to wait when entering PowerDownMode or aborting before returning an error.
    /// Should be longer than the longest frame transmission time to not false trigger the timeout, assuming all transmissions are
    /// aborted before entering power down, and just one might need to be completed.
    pub timeout_iterations_long: u32,
    pub timeout_iterations_short: u32,
}

impl FdCanConfig {
    /// Configures the bit timings.
    #[inline]
    pub const fn set_nominal_bit_timing(mut self, btr: NominalBitTiming) -> Self {
        self.nbtr = btr;
        self
    }

    /// Configures the bit timings.
    #[inline]
    pub const fn set_data_bit_timing(mut self, btr: DataBitTiming) -> Self {
        self.dbtr = btr;
        self
    }

    /// Enables or disables automatic retransmission of messages
    ///
    /// If this is enabled, the CAN peripheral will automatically try to retransmit each frame
    /// util it can be sent. Otherwise, it will try only once to send each frame.
    ///
    /// Automatic retransmission is enabled by default.
    #[inline]
    pub const fn set_automatic_retransmit(mut self, enabled: bool) -> Self {
        self.automatic_retransmit = enabled;
        self
    }

    /// Enabled or disables the pausing between transmissions
    ///
    /// This feature looses up burst transmissions coming from a single node and it protects against
    /// "babbling idiot" scenarios where the application program erroneously requests too many
    /// transmissions.
    #[inline]
    pub const fn set_transmit_pause(mut self, enabled: bool) -> Self {
        self.transmit_pause = enabled;
        self
    }

    /// If this is set, the FDCAN uses the CAN FD frame format as specified by the Bosch CAN
    /// FD Specification V1.0.
    #[inline]
    pub const fn set_non_iso_mode(mut self, enabled: bool) -> Self {
        self.non_iso_mode = enabled;
        self
    }

    /// Two consecutive dominant tq required to detect an edge for hard synchronization
    #[inline]
    pub const fn set_edge_filtering(mut self, enabled: bool) -> Self {
        self.edge_filtering = enabled;
        self
    }

    /// Sets the allowed transmission types for messages.
    #[inline]
    pub const fn set_frame_transmit(mut self, fts: FrameTransmissionConfig) -> Self {
        self.frame_transmit = fts;
        self
    }

    /// Enables protocol exception handling
    #[inline]
    pub const fn set_protocol_exception_handling(mut self, peh: bool) -> Self {
        self.protocol_exception_handling = peh;
        self
    }

    /// Selects Interrupt Line 1 for the given interrupts. Interrupt Line 0 is
    /// selected for all other interrupts
    #[inline]
    pub const fn select_interrupt_line_1(mut self, l1int: Ils) -> Self {
        self.interrupt_line_config = l1int;
        self
    }

    /// Sets the general clock divider for this FdCAN instance
    #[inline]
    pub const fn set_clock_divider(mut self, div: ClockDivider) -> Self {
        self.clock_divider = div;
        self
    }

    /// Sets the timestamp source
    #[inline]
    pub const fn set_timestamp_source(mut self, tss: TimestampSource) -> Self {
        self.timestamp_source = tss;
        self
    }

    /// Sets the global filter settings
    #[inline]
    pub const fn set_global_filter(mut self, filter: GlobalFilter) -> Self {
        self.global_filter = filter;
        self
    }
}

impl Default for FdCanConfig {
    #[inline]
    fn default() -> Self {
        Self {
            nbtr: NominalBitTiming::default(),
            dbtr: DataBitTiming::default(),
            automatic_retransmit: true,
            transmit_pause: false,
            frame_transmit: FrameTransmissionConfig::ClassicCanOnly,
            non_iso_mode: false,
            edge_filtering: false,
            interrupt_line_config: Ils(0),
            protocol_exception_handling: true,
            clock_divider: ClockDivider::_1,
            timestamp_source: TimestampSource::None,
            global_filter: GlobalFilter::default(),
            #[cfg(feature = "h7")]
            layout: MessageRamLayout::default(),
            timeout_iterations_long: 10_000_000,
            timeout_iterations_short: 1_000_000,
        }
    }
}

impl FdCan<ConfigMode> {
    #[inline]
    pub fn into_internal_loopback(
        mut self,
    ) -> Result<FdCan<InternalLoopbackMode>, (Error, FdCan<ConfigMode>)> {
        self.set_loopback_mode(LoopbackMode::Internal);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into ExternalLoopbackMode
    #[inline]
    pub fn into_external_loopback(
        mut self,
    ) -> Result<FdCan<ExternalLoopbackMode>, (Error, FdCan<ConfigMode>)> {
        self.set_loopback_mode(LoopbackMode::External);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into RestrictedOperationMode
    #[inline]
    pub fn into_restricted(
        mut self,
    ) -> Result<FdCan<RestrictedOperationMode>, (Error, FdCan<ConfigMode>)> {
        self.set_restricted_operations(true);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into NormalOperationMode
    #[inline]
    pub fn into_normal(mut self) -> Result<FdCan<NormalOperationMode>, (Error, FdCan<ConfigMode>)> {
        self.set_normal_operations(true);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into BusMonitoringMode
    #[inline]
    pub fn into_bus_monitoring(
        mut self,
    ) -> Result<FdCan<BusMonitoringMode>, (Error, FdCan<ConfigMode>)> {
        self.set_bus_monitoring_mode(true);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into TestMode
    #[inline]
    pub fn into_test_mode(mut self) -> Result<FdCan<TestMode>, (Error, FdCan<ConfigMode>)> {
        self.set_test_mode(true);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    /// Moves out of ConfigMode and into PoweredDownMode
    #[inline]
    pub fn into_powered_down(
        mut self,
    ) -> Result<FdCan<PoweredDownMode>, (Error, FdCan<PoweredDownMode>)> {
        // TODO: handle error better here, the only reason for it is if timeout is too short, but PoweredDownMode should be reached eventually anyway
        if let Err(e) = self.set_power_down_mode(true) {
            return Err((e, self.into_mode()));
        }
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self.into_mode()));
        }
        Ok(self.into_mode())
    }

    #[inline]
    fn leave_init_mode(&mut self) -> Result<(), Error> {
        self.apply_config(self.config)?;

        #[cfg(feature = "asynchronous")]
        self.enable_interrupts();

        self.can.cccr().modify(|w| w.set_cce(false));
        self.can.cccr().modify(|w| w.set_init(false));
        crate::util::checked_wait(
            || self.can.cccr().read().init(),
            self.config.timeout_iterations_short,
        )?;
        Ok(())
    }

    #[inline]
    #[cfg(feature = "asynchronous")]
    fn enable_interrupts(&mut self) {
        use crate::pac::fdcan::regs::{Ie, Txbcie, Txbtie};
        use crate::pac::variant::{IR_ALL, TX_BUFFERS_ALL};
        // Enable all interrupts when this crate handles them
        self.can.ie().write_value(Ie(IR_ALL));
        self.can.txbtie().write_value(Txbtie(TX_BUFFERS_ALL));
        self.can.txbcie().write_value(Txbcie(TX_BUFFERS_ALL));
        self.can.ile().modify(|w| w.set_eint0(true));
    }

    /// Applies the settings of a new FdCanConfig See [`FdCanConfig`]
    ///
    /// Fails with [Error::WrongInstance] if `config.layout` belongs to another instance.
    #[inline]
    pub fn apply_config(&mut self, config: FdCanConfig) -> Result<(), Error> {
        self.set_data_bit_timing(config.dbtr);
        self.set_nominal_bit_timing(config.nbtr);
        self.set_automatic_retransmit(config.automatic_retransmit);
        self.set_transmit_pause(config.transmit_pause);
        self.set_frame_transmit(config.frame_transmit);
        self.select_interrupt_line_1(config.interrupt_line_config);
        self.set_non_iso_mode(config.non_iso_mode);
        self.set_edge_filtering(config.edge_filtering);
        self.set_protocol_exception_handling(config.protocol_exception_handling);
        self.set_global_filter(config.global_filter);
        #[cfg(feature = "h7")]
        self.set_layout(config.layout)?;
        Ok(())
    }

    /// Configures the nominal (arbitration phase) bit timing. See [NominalBitTiming].
    #[inline]
    pub fn set_nominal_bit_timing(&mut self, btr: NominalBitTiming) {
        self.config.nbtr = btr;
        self.can.nbtp().write_value(btr.nbtp());
    }

    /// Configures the data phase bit timing and transceiver delay compensation. See [DataBitTiming].
    /// Only used when frame_transmit allows bit rate switching.
    #[inline]
    pub fn set_data_bit_timing(&mut self, btr: DataBitTiming) {
        self.config.dbtr = btr;
        self.can.dbtp().write_value(btr.dbtp());
        if let Some(tdc) = btr.tdc() {
            self.can.tdcr().write_value(tdc.tdcr());
        }
    }

    /// Enables or disables automatic retransmission of messages
    ///
    /// If this is enabled, the CAN peripheral will automatically try to retransmit each frame
    /// util it can be sent. Otherwise, it will try only once to send each frame.
    ///
    /// Automatic retransmission is enabled by default.
    #[inline]
    pub fn set_automatic_retransmit(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_dar(!enabled));
        self.config.automatic_retransmit = enabled;
    }

    /// Configures the transmit pause feature. See
    /// [`FdCanConfig::set_transmit_pause`]
    #[inline]
    pub fn set_transmit_pause(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_txp(enabled));
        self.config.transmit_pause = enabled;
    }

    /// Configures non-iso mode. See [`FdCanConfig::set_non_iso_mode`]
    #[inline]
    pub fn set_non_iso_mode(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_niso(enabled));
        self.config.non_iso_mode = enabled;
    }

    /// Configures edge filtering. See [`FdCanConfig::set_edge_filtering`]
    #[inline]
    pub fn set_edge_filtering(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_efbi(enabled));
        self.config.edge_filtering = enabled;
    }

    /// Configures frame transmission mode. See
    /// [`FdCanConfig::set_frame_transmit`]
    #[inline]
    pub fn set_frame_transmit(&mut self, fts: FrameTransmissionConfig) {
        let (fdoe, brse) = match fts {
            FrameTransmissionConfig::ClassicCanOnly => (false, false),
            FrameTransmissionConfig::AllowFdCan => (true, false),
            FrameTransmissionConfig::AllowFdCanAndBRS => (true, true),
        };

        self.can.cccr().modify(|w| {
            w.set_fdoe(fdoe);
            #[cfg(feature = "h7")]
            w.set_bse(brse);
            #[cfg(not(feature = "h7"))]
            w.set_brse(brse);
        });

        self.config.frame_transmit = fts;
    }

    /// Selects Interrupt Line 1 for the given interrupts. Interrupt Line 0 is
    /// selected for all other interrupts. See
    /// [`FdCanConfig::select_interrupt_line_1`]
    pub fn select_interrupt_line_1(&mut self, l1int: Ils) {
        self.can.ils().write_value(l1int);

        self.config.interrupt_line_config = l1int;
    }

    /// Sets the protocol exception handling on/off
    #[inline]
    pub fn set_protocol_exception_handling(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_pxhd(!enabled));

        self.config.protocol_exception_handling = enabled;
    }

    /// Configures and resets the timestamp counter
    #[inline]
    pub fn set_timestamp_counter_source(&mut self, select: TimestampSource) {
        let (tcp, tss) = match select {
            TimestampSource::None => (0, 0b00),
            TimestampSource::Prescaler(p) => (p as u8, 0b01),
            TimestampSource::FromTIM3 => (0, 0b10),
        };
        self.can.tscc().write(|w| {
            w.set_tcp(tcp);
            #[cfg(feature = "h7")]
            w.set_tss(tss);
            #[cfg(not(feature = "h7"))]
            w.set_tss(crate::pac::fdcan::vals::Tss::from_bits(tss));
        });

        self.config.timestamp_source = select;
    }

    /// Configures the global filter settings
    #[inline]
    pub fn set_global_filter(&mut self, filter: GlobalFilter) {
        // Stored so that `leave_init_mode` (which re-applies `self.config`) keeps it.
        self.config.global_filter = filter;
        #[cfg(feature = "h7")]
        self.can.gfc().modify(|w| {
            w.set_anfs(filter.handle_standard_frames as u8);
            w.set_anfe(filter.handle_extended_frames as u8);
            w.set_rrfs(filter.reject_remote_standard_frames);
            w.set_rrfe(filter.reject_remote_extended_frames);
        });
        // Lite cores: the global filter fields live in RXGFC (same bit positions as GFC on H7).
        #[cfg(not(feature = "h7"))]
        self.can.rxgfc().modify(|w| {
            use crate::pac::fdcan::vals::{Anfe, Anfs};
            w.set_anfs(Anfs::from_bits(filter.handle_standard_frames as u8));
            w.set_anfe(Anfe::from_bits(filter.handle_extended_frames as u8));
            w.set_rrfs(filter.reject_remote_standard_frames);
            w.set_rrfe(filter.reject_remote_extended_frames);
        });
    }

    /// Configures RAM layout for this instance. When the layout changes, the RAM region it owns is zeroed
    /// (ECC initialisation), so applying the same layout again keeps what was written in Config mode.
    ///
    /// Returns [Error::WrongInstance] if the layout was built for another instance.
    #[cfg(feature = "h7")]
    #[inline]
    pub fn set_layout(&mut self, layout: MessageRamLayout) -> Result<(), Error> {
        if layout.instance.is_some_and(|i| i != self.instance) {
            return Err(Error::WrongInstance);
        }
        if layout != self.config.layout {
            self.zero_msg_ram(layout.region());
        }
        self.config.layout = layout;
        self.can.sidfc().modify(|w| {
            w.set_flssa(layout.eleven_bit_filters_addr);
            w.set_lss(layout.eleven_bit_filters_len);
        });
        self.can.xidfc().modify(|w| {
            w.set_flesa(layout.twenty_nine_bit_filters_addr);
            w.set_lse(layout.twenty_nine_bit_filters_len);
        });
        self.can.rxfc(0).modify(|w| {
            w.set_fsa(layout.rx_fifo0_addr);
            w.set_fs(layout.rx_fifo0_len);
        });
        self.can.rxfc(1).modify(|w| {
            w.set_fsa(layout.rx_fifo1_addr);
            w.set_fs(layout.rx_fifo1_len);
        });
        self.can.rxbc().modify(|w| {
            w.set_rbsa(layout.rx_buffers_addr);
        });
        self.can.rxesc().modify(|w| {
            w.set_rbds(layout.rx_buffers_data_size.config_register());
            w.set_fds(0, layout.rx_fifo0_data_size.config_register());
            w.set_fds(1, layout.rx_fifo1_data_size.config_register());
        });
        self.can.txefc().modify(|w| {
            w.set_efsa(layout.tx_event_fifo_addr);
            w.set_efs(layout.tx_event_fifo_len);
        });
        self.can.txbc().modify(|w| {
            w.set_tbsa(layout.tx_buffers_addr);
            w.set_tfqs(layout.tx_fifo_or_queue_len);
            w.set_ndtb(layout.tx_buffers_len);
        });
        self.can
            .txesc()
            .modify(|w| w.set_tbds(layout.tx_buffers_data_size.config_register()));
        // TT registers only exist on FDCAN1 (RM0468).
        if self.instance == crate::FdCanInstance::FdCan1 {
            self.can.tttmc().modify(|w| {
                w.set_tmsa(layout.trigger_memory_addr);
                w.set_tme(layout.trigger_memory_len);
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_ranges() {
        use BitTimingError::*;
        assert!(NominalBitTiming::new(512, 256, 128, 128).is_ok());
        assert!(NominalBitTiming::new(1, 2, 1, 1).is_ok());
        assert_eq!(NominalBitTiming::new(0, 11, 4, 4), Err(PrescalerOutOfRange));
        assert_eq!(
            NominalBitTiming::new(513, 11, 4, 4),
            Err(PrescalerOutOfRange)
        );
        assert_eq!(NominalBitTiming::new(1, 1, 4, 4), Err(Seg1OutOfRange));
        assert_eq!(NominalBitTiming::new(1, 257, 4, 4), Err(Seg1OutOfRange));
        assert_eq!(NominalBitTiming::new(1, 11, 0, 1), Err(Seg2OutOfRange));
        assert_eq!(NominalBitTiming::new(1, 11, 129, 4), Err(Seg2OutOfRange));
        assert_eq!(
            NominalBitTiming::new(1, 11, 4, 0),
            Err(SyncJumpWidthOutOfRange)
        );
        assert_eq!(
            NominalBitTiming::new(1, 11, 4, 5),
            Err(SyncJumpWidthLongerThanSeg2)
        );
    }

    /// Register holds value - 1 (RM0468 FDCAN_NBTP: NSJW [31:25], NBRP [24:16], NTSEG1 [15:8], NTSEG2 [6:0]).
    #[test]
    fn nominal_register_encoding() {
        assert_eq!(NominalBitTiming::default().nbtp().0, 0x0600_0A03);
        let max = NominalBitTiming::new(512, 256, 128, 128).unwrap().nbtp();
        assert_eq!(max.0, 0xFFFF_FF7F);
        let min = NominalBitTiming::new(1, 2, 1, 1).unwrap().nbtp();
        assert_eq!(min.0, 0x0000_0100);
        assert_eq!(NominalBitTiming::default().quanta_per_bit(), 16);
    }

    #[test]
    fn data_ranges() {
        use BitTimingError::*;
        assert!(DataBitTiming::new(32, 32, 16, 16).is_ok());
        assert!(DataBitTiming::new(1, 1, 1, 1).is_ok());
        assert_eq!(DataBitTiming::new(0, 11, 4, 4), Err(PrescalerOutOfRange));
        assert_eq!(DataBitTiming::new(33, 11, 4, 4), Err(PrescalerOutOfRange));
        assert_eq!(DataBitTiming::new(1, 0, 4, 4), Err(Seg1OutOfRange));
        assert_eq!(DataBitTiming::new(1, 33, 4, 4), Err(Seg1OutOfRange));
        assert_eq!(DataBitTiming::new(1, 11, 17, 4), Err(Seg2OutOfRange));
        assert_eq!(
            DataBitTiming::new(1, 11, 4, 0),
            Err(SyncJumpWidthOutOfRange)
        );
        assert_eq!(
            DataBitTiming::new(1, 11, 4, 5),
            Err(SyncJumpWidthLongerThanSeg2)
        );
    }

    /// RM0468 FDCAN_DBTP: TDC [23], DBRP [20:16], DTSEG1 [12:8], DTSEG2 [7:4], DSJW [3:0].
    #[test]
    fn data_register_encoding() {
        assert_eq!(DataBitTiming::default().dbtp().0, 0x0000_0A33);
        let max = DataBitTiming::new(32, 32, 16, 16).unwrap().dbtp();
        assert_eq!(max.0, 0x001F_1FFF);
        let tdc = TransceiverDelayCompensation::new(5, 0).unwrap();
        let with_tdc = DataBitTiming::default().with_tdc(tdc).unwrap();
        assert_eq!(with_tdc.dbtp().0, 0x0080_0A33);
    }

    /// RM0468 FDCAN_TDCR: TDCO [14:8], TDCF [6:0].
    #[test]
    fn tdc() {
        let tdc = TransceiverDelayCompensation::new(127, 3).unwrap();
        assert_eq!(tdc.tdcr().0, 0x7F03);
        assert_eq!(
            TransceiverDelayCompensation::new(128, 0),
            Err(BitTimingError::TdcOutOfRange)
        );
        assert_eq!(
            TransceiverDelayCompensation::new(0, 128),
            Err(BitTimingError::TdcOutOfRange)
        );

        // 5 Mbit/s at 80 MHz: 16 quanta, sample point after 1 + 11 quanta.
        let dbtr = DataBitTiming::new(1, 11, 4, 4).unwrap();
        let tdc = TransceiverDelayCompensation::at_sample_point(&dbtr).unwrap();
        assert_eq!((tdc.offset(), tdc.filter_window()), (12, 0));
        let dbtr = DataBitTiming::new(2, 31, 8, 8).unwrap();
        assert_eq!(
            TransceiverDelayCompensation::at_sample_point(&dbtr)
                .unwrap()
                .offset(),
            64
        );
        let dbtr = DataBitTiming::new(4, 31, 8, 8).unwrap();
        assert_eq!(
            TransceiverDelayCompensation::at_sample_point(&dbtr),
            Err(BitTimingError::TdcOutOfRange)
        );
        assert_eq!(
            dbtr.with_tdc(TransceiverDelayCompensation::new(10, 0).unwrap()),
            Err(BitTimingError::TdcPrescalerTooLarge)
        );
    }
}
