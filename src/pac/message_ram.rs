use bitfield_struct::bitfield;

macro_rules! enum_bit {
    ($name:ident, $zero_name:ident, $one_name:ident) => {
        #[cfg_attr(feature = "defmt", derive(defmt::Format))]
        pub enum $name {
            /// 0 =
            $zero_name,
            /// 1 =
            $one_name,
        }

        impl $name {
            const fn into_bits(self) -> u8 {
                match self {
                    $name::$zero_name => 0,
                    $name::$one_name => 1,
                }
            }

            const fn from_bits(value: u8) -> Self {
                match value {
                    0 => $name::$zero_name,
                    1 => $name::$one_name,
                    _ => unreachable!(),
                }
            }
        }
    };
}

/// Up to 128 filter elements can be configured for 11-bit IDs. When accessing a Standard Message ID
/// Filter element, its address is the Filter List Standard Start Address SIDFC.FLSSA plus the index of
/// the filter element (0…127).
#[bitfield(u32, order = Msb, default = false, debug = false, defmt = cfg(feature = "defmt"))]
pub struct StandardFilterElement {
    /// Standard Filter Type
    #[bits(2)]
    pub sft: StandardFilterType,

    /// Standard Filter Element Configuration
    ///
    /// All enabled filter elements are used for acceptance filtering of 11-bit ID frames. Acceptance filtering
    /// stops at the first matching enabled filter element or when the end of the filter list is reached. If SFEC
    /// = “100”, “101”, or “110” a match sets interrupt flag IR.HPM and, if enabled, an interrupt is generated.
    /// In this case register HPMS is updated with the status of the priority match.
    #[bits(3)]
    pub sfec: StandardFilterConfiguration,

    /// Standard Filter ID 1
    ///
    /// First ID of standard ID filter element. When filtering for Rx Buffers, Sync messages, or for debug
    /// messages this field defines the ID of the message to be stored. The received identifiers must match
    /// exactly, no masking mechanism is used
    #[bits(11)]
    pub sfid1: u16,

    /// Standard Sync Message
    ///
    /// Only evaluated when CCCR.UTSU = ‘1’. When this bit is set and a matching message is received,
    /// a pulse with the duration of one m_can_hclk period is generated at output m_can_tsrx to signal
    /// the reception of a Sync message to the Timestamping Unit (TSU) connected to the M_CAN.
    #[bits(1)]
    pub ssync: bool,

    #[bits(4)]
    _reserved: u8,

    /// Standard Filter ID 2
    ///
    /// This bit field has a different meaning depending on the configuration of SFEC:
    /// 1) SFEC = “001”...”110” Second ID of standard ID filter element
    /// 2) SFEC = “111” Filter for Rx Buffers or for debug messages
    /// SFID2 10:9  decides whether the received message is stored into an Rx Buffer or treated as
    /// message A, B, or C of the debug message sequence.
    ///
    /// 00= Store message into an Rx Buffer
    ///
    /// 01= Debug Message A
    ///
    /// 10= Debug Message B
    ///
    /// 11= Debug Message C
    ///
    /// SFID2 8:6 is used to control the filter event pins m_can_fe 2:0 at the Extension Interface. A one
    /// at the respective bit position enables generation of a pulse at the related filter event pin with the
    /// duration of one m_can_hclk period in case the filter matches.
    ///
    /// SFID2 5:0 defines the offset to the Rx Buffer Start Address RXBC.RBSA for storage of a matching
    /// message.
    #[bits(11)]
    pub sfid2: u16,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum StandardFilterType {
    /// Range filter from SFID1 to SFID2 (SFID2 ≥ SFID1)
    Range = 0b00,
    /// Dual ID filter for SFID1 or SFID2
    DualID = 0b01,
    /// Classic filter: SFID1 = filter, SFID2 = mask
    Classic = 0b10,
    /// Filter element disabled
    Disabled = 0b11,
}

impl StandardFilterType {
    const fn into_bits(self) -> u8 {
        self as u8
    }

    const fn from_bits(value: u8) -> StandardFilterType {
        match value {
            0b00 => StandardFilterType::Range,
            0b01 => StandardFilterType::DualID,
            0b10 => StandardFilterType::Classic,
            0b11 => StandardFilterType::Disabled,
            _ => unreachable!(),
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum StandardFilterConfiguration {
    /// Disable filter element
    Disable = 0b000,
    /// Store in Rx FIFO 0 if filter matches
    StoreInFIFO0 = 0b001,
    /// Store in Rx FIFO 1 if filter matches
    StoreInFIFO1 = 0b010,
    /// Reject ID if filter matches, not intended to be used with Sync messages
    Reject = 0b011,
    /// Set priority if filter matches, not intended to be used with Sync messages, no storage
    SetPriority = 0b100,
    /// Set priority and store in FIFO 0 if filter matches
    SetPriorityAndStoreInFIFO0 = 0b101,
    /// Set priority and store in FIFO 1 if filter matches
    SetPriorityAndStoreInFIFO1 = 0b110,
    /// Store into Rx Buffer or as debug message, configuration of SFT[1:0] ignored
    StoreAsDebugMessage = 0b111,
}

impl StandardFilterConfiguration {
    const fn into_bits(self) -> u8 {
        self as u8
    }

    const fn from_bits(value: u8) -> StandardFilterConfiguration {
        match value {
            0b000 => StandardFilterConfiguration::Disable,
            0b001 => StandardFilterConfiguration::StoreInFIFO0,
            0b010 => StandardFilterConfiguration::StoreInFIFO1,
            0b011 => StandardFilterConfiguration::Reject,
            0b100 => StandardFilterConfiguration::SetPriority,
            0b101 => StandardFilterConfiguration::SetPriorityAndStoreInFIFO0,
            0b110 => StandardFilterConfiguration::SetPriorityAndStoreInFIFO1,
            0b111 => StandardFilterConfiguration::StoreAsDebugMessage,
            _ => unreachable!(),
        }
    }
}

/// The Tx Buffers section can be configured to hold dedicated Tx Buffers as well as a Tx FIFO / Tx Queue.
///
/// In case that the Tx Buffers section is shared by dedicated Tx buffers and a Tx FIFO / Tx Queue:
/// the dedicated Tx Buffers start at the beginning of the Tx Buffers section followed by the
/// buffers assigned to the Tx FIFO or Tx Queue.
///
/// The Tx Handler distinguishes between dedicated Tx Buffers and Tx FIFO / Tx Queue by evaluating the
/// Tx Buffer configuration TXBC. TFQS and TXBC.NDTB. The element size can be configured for storage of
/// CAN FD messages with up to 64  bytes data field via register TXESC.
#[bitfield(u32, order = Msb, default = false, debug = false, defmt = cfg(feature = "defmt"))]
pub struct TxBufferElementT0 {
    /// Error State Indicator
    ///
    /// The ESI bit of the transmit buffer is or’ed with the error passive flag to decide the value
    /// of the ESI bit in the transmitted FD frame. As required by the CAN FD protocol specification,
    /// an error active node may optionally transmit the ESI bit recessive, but an error
    /// passive node will always transmit the ESI bit recessive
    #[bits(1)]
    pub esi: Esi,

    /// Extended Identifier
    #[bits(1)]
    pub xtd: ExtendedIdentifier,

    /// Remote Transmission Request
    ///
    /// NOTE: When RTR = 1, the M_CAN transmits a remote frame according to ISO 11898-1:2015, even
    /// if CCCR.FDOE enables the transmission in CAN FD format.
    #[bits(1)]
    pub rtr: Rtr,

    /// Standard or extended identifier depending on bit XTD. A standard identifier has to be written to ID 28:18.
    #[bits(29)]
    pub id: u32,
}

enum_bit!(Esi, EsiDependsOnErrorPassive, EsiTransmittedRecessive);
enum_bit!(ExtendedIdentifier, ElevenBits, TwentyNineBits);
enum_bit!(Rtr, TransmitDataFrame, TransmitRemoteFrame);

#[bitfield(u32, order = Msb, default = false, debug = false, defmt = cfg(feature = "defmt"))]
pub struct TxBufferElementT1 {
    /// Written by CPU during Tx Buffer configuration. Copied into Tx Event FIFO element for identification
    /// of Tx message status.
    #[bits(8)]
    pub message_marker_low: u8,

    /// Event FIFO Control
    #[bits(1)]
    pub efc: EventFIFOControl,

    #[bits(1)]
    pub tsce: TimeStampCaptureEnable,

    #[bits(1)]
    pub fdf: FDFormat,

    #[bits(1)]
    pub brs: BitRateSwitch,

    /// Data Length Code
    ///
    /// 0-8= CAN + CAN FD: transmit frame has 0-8 data bytes
    ///
    /// 9-15= CAN: transmit frame has 8 data bytes
    ///
    /// 9-15=CAN FD: transmit frame has 12/16/20/24/32/48/64 data bytes
    #[bits(4)]
    pub dlc: u8,

    /// Message Marker
    ///
    /// High byte of Wide Message Marker, written by CPU during Tx Buffer configuration. Copied into Tx
    /// Event FIFO element for identification of Tx message status. Available only when CCCR.WMM = ‘1’
    /// or when CCCR.UTSU = ‘1’
    #[bits(8)]
    pub message_marker_high: u8,

    #[bits(8)]
    _reserved: u8,
}

enum_bit!(EventFIFOControl, DontStoreTxEvents, StoreTxEvents);
enum_bit!(TimeStampCaptureEnable, Disabled, Enabled);
enum_bit!(FDFormat, Classic, FD);
enum_bit!(BitRateSwitch, Without, Switch);
