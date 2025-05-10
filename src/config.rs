use crate::PoweredDownMode;
use crate::fdcan::{
    BusMonitoringMode, Error, ExternalLoopbackMode, NormalOperationMode, RestrictedOperationMode,
    TestMode,
};
use crate::fdcan::{ConfigMode, FdCan, InternalLoopbackMode, LoopbackMode};
use crate::message_ram_layout::MessageRamLayout;
use crate::pac::registers::regs::Ir;
use core::num::{NonZeroU8, NonZeroU16};

/// Configures the bit timings.
///
/// You can use <http://www.bittiming.can-wiki.info/> to calculate the `btr` parameter. Enter
/// parameters as follows:
///
/// - *Clock Rate*: The input clock speed to the CAN peripheral (*not* the CPU clock speed).
///   This is the clock rate of the peripheral bus the CAN peripheral is attached to (eg. APB1).
/// - *Sample Point*: Should normally be left at the default value of 87.5%.
/// - *SJW*: Should normally be left at the default value of 1.
///
/// Then copy the `CAN_BUS_TIME` register value from the table and pass it as the `btr`
/// parameter to this method.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NominalBitTiming {
    /// Value by which the oscillator frequency is divided for generating the bit time quanta. The bit
    /// time is built up from a multiple of this quanta. Valid values are 1 to 512.
    pub prescaler: NonZeroU16,
    /// Valid values are 1 to 128.
    pub seg1: NonZeroU8,
    /// Valid values are 1 to 255.
    pub seg2: NonZeroU8,
    /// Valid values are 1 to 128.
    pub sync_jump_width: NonZeroU8,
}
impl NominalBitTiming {
    #[inline]
    pub(crate) fn nbrp(&self) -> u16 {
        u16::from(self.prescaler) & 0x1FF
    }
    #[inline]
    pub(crate) fn ntseg1(&self) -> u8 {
        u8::from(self.seg1)
    }
    #[inline]
    pub(crate) fn ntseg2(&self) -> u8 {
        u8::from(self.seg2) & 0x7F
    }
    #[inline]
    pub(crate) fn nsjw(&self) -> u8 {
        u8::from(self.sync_jump_width) & 0x7F
    }
}

impl Default for NominalBitTiming {
    #[inline]
    fn default() -> Self {
        // Kernel Clock 8MHz, Bit rate: 500kbit/s. Corresponds to a NBTP
        // register value of 0x0600_0A03
        Self {
            prescaler: NonZeroU16::new(1).unwrap(),
            seg1: NonZeroU8::new(11).unwrap(),
            seg2: NonZeroU8::new(4).unwrap(),
            sync_jump_width: NonZeroU8::new(4).unwrap(),
        }
    }
}

/// Configures the data bit timings for the FdCan Variable Bitrates.
/// This is not used when frame_transmit is set to anything other than AllowFdCanAndBRS.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct DataBitTiming {
    /// Tranceiver Delay Compensation
    pub transceiver_delay_compensation: bool,
    ///  The value by which the oscillator frequency is divided to generate the bit time quanta. The bit
    ///  time is built up from a multiple of this quanta. Valid values for the Baud Rate Prescaler are 1
    ///  to 31.
    pub prescaler: NonZeroU8,
    /// Valid values are 1 to 31.
    pub seg1: NonZeroU8,
    /// Valid values are 1 to 15.
    pub seg2: NonZeroU8,
    /// Must always be smaller than DTSEG2, valid values are 1 to 15.
    pub sync_jump_width: NonZeroU8,
}
impl DataBitTiming {
    // #[inline]
    // fn tdc(&self) -> u8 {
    //     let tsd = self.transceiver_delay_compensation as u8;
    //     //TODO: stm32g4 does not export the TDC field
    //     todo!()
    // }
    #[inline]
    pub(crate) fn dbrp(&self) -> u8 {
        u8::from(self.prescaler) & 0x1F
    }
    #[inline]
    pub(crate) fn dtseg1(&self) -> u8 {
        u8::from(self.seg1) & 0x1F
    }
    #[inline]
    pub(crate) fn dtseg2(&self) -> u8 {
        u8::from(self.seg2) & 0x0F
    }
    #[inline]
    pub(crate) fn dsjw(&self) -> u8 {
        u8::from(self.sync_jump_width) & 0x0F
    }
}

impl Default for DataBitTiming {
    #[inline]
    fn default() -> Self {
        // Kernel Clock 8MHz, Bit rate: 500kbit/s. Corresponds to a DBTP
        // register value of 0x0000_0A33
        Self {
            transceiver_delay_compensation: false,
            prescaler: NonZeroU8::new(1).unwrap(),
            seg1: NonZeroU8::new(11).unwrap(),
            seg2: NonZeroU8::new(4).unwrap(),
            sync_jump_width: NonZeroU8::new(4).unwrap(),
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
    pub interrupt_line_config: Ir,
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
    pub const fn select_interrupt_line_1(mut self, l1int: Ir) -> Self {
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
            interrupt_line_config: Ir(0),
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
        self.apply_config(self.config);

        self.can.cccr().modify(|w| w.set_cce(false));
        self.can.cccr().modify(|w| w.set_init(false));
        crate::util::checked_wait(
            || self.can.cccr().read().init(),
            self.config.timeout_iterations_short,
        )?;
        Ok(())
    }

    /// Applies the settings of a new FdCanConfig See [`FdCanConfig`]
    #[inline]
    pub fn apply_config(&mut self, config: FdCanConfig) {
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
        self.set_layout(config.layout);
    }

    /// Configures the bit timings.
    ///
    /// You can use <http://www.bittiming.can-wiki.info/> to calculate the `btr` parameter. Enter
    /// parameters as follows:
    ///
    /// - *Clock Rate*: The input clock speed to the CAN peripheral (*not* the CPU clock speed).
    ///   This is the clock rate of the peripheral bus the CAN peripheral is attached to (e.g., APB1).
    /// - *Sample Point*: Should normally be left at the default value of 87.5%.
    /// - *SJW*: Should normally be left at the default value of 1.
    ///
    /// Then copy the `CAN_BUS_TIME` register value from the table and pass it as the `btr`
    /// parameter to this method.
    #[inline]
    pub fn set_nominal_bit_timing(&mut self, btr: NominalBitTiming) {
        self.config.nbtr = btr;

        self.can.nbtp().write(|w| {
            w.set_nbrp(btr.nbrp() - 1);
            w.set_ntseg1(btr.ntseg1() - 1);
            w.set_ntseg2(btr.ntseg2() - 1);
            w.set_nsjw(btr.nsjw() - 1);
        });
    }

    /// Configures the data bit timings for the FdCan Variable Bitrates.
    /// This is not used when frame_transmit is set to anything other than AllowFdCanAndBRS.
    #[inline]
    pub fn set_data_bit_timing(&mut self, btr: DataBitTiming) {
        self.config.dbtr = btr;

        self.can.dbtp().write(|w| {
            w.set_dbrp(btr.dbrp() - 1);
            w.set_dtseg1(btr.dtseg1() - 1);
            w.set_dtseg2(btr.dtseg2() - 1);
            w.set_dsjw(btr.dsjw() - 1);
        });
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
            w.set_bse(brse);
        });

        self.config.frame_transmit = fts;
    }

    /// Selects Interrupt Line 1 for the given interrupts. Interrupt Line 0 is
    /// selected for all other interrupts. See
    /// [`FdCanConfig::select_interrupt_line_1`]
    pub fn select_interrupt_line_1(&mut self, l1int: Ir) {
        self.can.ils().modify(|w| w.0 = l1int.0);

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
            w.set_tss(tss);
        });

        self.config.timestamp_source = select;
    }

    /// Configures the global filter settings
    #[inline]
    pub fn set_global_filter(&mut self, filter: GlobalFilter) {
        self.can.gfc().modify(|w| {
            w.set_anfs(filter.handle_standard_frames as u8);
            w.set_anfe(filter.handle_extended_frames as u8);
            w.set_rrfs(filter.reject_remote_standard_frames);
            w.set_rrfe(filter.reject_remote_extended_frames);
        });
    }

    /// Configures RAM layout for this instance
    #[cfg(feature = "h7")]
    #[inline]
    pub fn set_layout(&mut self, layout: MessageRamLayout) {
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
        self.can.tttmc().modify(|w| {
            w.set_tmsa(layout.trigger_memory_addr);
            w.set_tme(layout.trigger_memory_len);
        });
    }
}
