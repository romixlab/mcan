use core::marker::PhantomData;
use paste::paste;
use static_cell::StaticCell;

#[derive(Default, Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct MessageRamLayout {
    pub(crate) eleven_bit_filters_addr: u16,
    pub(crate) eleven_bit_filters_len: u8,

    pub(crate) twenty_nine_bit_filters_addr: u16,
    pub(crate) twenty_nine_bit_filters_len: u8,

    pub(crate) rx_fifo0_addr: u16,
    pub(crate) rx_fifo0_len: u8,
    pub(crate) rx_fifo0_data_size: DataFieldSize,

    pub(crate) rx_fifo1_addr: u16,
    pub(crate) rx_fifo1_len: u8,
    pub(crate) rx_fifo1_data_size: DataFieldSize,

    pub(crate) rx_buffers_addr: u16,
    /// Not actually used, as 3 is implied, just to keep code clean
    pub(crate) rx_buffers_len: u8,
    pub(crate) rx_buffers_data_size: DataFieldSize,

    pub(crate) tx_event_fifo_addr: u16,
    pub(crate) tx_event_fifo_len: u8,

    pub(crate) tx_buffers_addr: u16,
    /// Number of dedicated transmit buffers
    pub(crate) tx_buffers_len: u8,
    /// Transmit FIFO/Queue size
    pub(crate) tx_fifo_or_queue_len: u8,
    pub(crate) tx_buffers_data_size: DataFieldSize,

    #[cfg(feature = "h7")]
    pub(crate) trigger_memory_addr: u16,
    #[cfg(feature = "h7")]
    pub(crate) trigger_memory_len: u8,
}

/// Data size of RX FIFO0/1, RX buffer and TX buffer element, total element size is 8 bytes longer (2 words header).
/// Should probably be all the same, and either 8 bytes or 64 bytes, unless some very specific configuration is desired.
#[derive(Default, Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DataFieldSize {
    #[default]
    _8Bytes,
    _12Bytes,
    _16Bytes,
    _20Bytes,
    _24Bytes,
    _32Bytes,
    _48Bytes,
    _64Bytes,
}

impl DataFieldSize {
    fn words(&self) -> u16 {
        match self {
            DataFieldSize::_8Bytes => 2,
            DataFieldSize::_12Bytes => 3,
            DataFieldSize::_16Bytes => 4,
            DataFieldSize::_20Bytes => 5,
            DataFieldSize::_24Bytes => 6,
            DataFieldSize::_32Bytes => 8,
            DataFieldSize::_48Bytes => 12,
            DataFieldSize::_64Bytes => 16,
        }
    }

    pub(crate) fn config_register(&self) -> u8 {
        match self {
            DataFieldSize::_8Bytes => 0b000,
            DataFieldSize::_12Bytes => 0b001,
            DataFieldSize::_16Bytes => 0b010,
            DataFieldSize::_20Bytes => 0b011,
            DataFieldSize::_24Bytes => 0b100,
            DataFieldSize::_32Bytes => 0b101,
            DataFieldSize::_48Bytes => 0b110,
            DataFieldSize::_64Bytes => 0b111,
        }
    }
}

impl MessageRamLayout {
    // Turn this layout back into builder, useful if doing re-init of just one CAN instance, without touching others.
    pub fn relayout(self) -> MessageRamBuilder<ElevenBitFilters> {
        // pos: first non zero start, end: last non zero start+size?
        todo!()
    }
}

// The builder states below. Builder will go through these states in order for consistency and
// simplicity, though MCAN itself does not impose a particular order of various blocks.
pub struct ElevenBitFilters;
pub struct TwentyNineBitFilters;
pub struct RxFifo0;
pub struct RxFifo1;
pub struct RxBuffers;
pub struct TxEventFifo;
pub struct TxBuffers;
#[cfg(feature = "h7")]
pub struct TriggerMemory;

pub struct MessageRamBuilder<S> {
    pos: u16,
    end: u16,
    layout: MessageRamLayout,
    _phantom: PhantomData<S>,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MessageRamBuilderError {
    BuilderTaken,
    TooManyElements,
    OutOfMemory,
}

static BUILDER_TAKEN: StaticCell<()> = StaticCell::new();

pub fn message_ram_builder() -> Result<MessageRamBuilder<ElevenBitFilters>, MessageRamBuilderError>
{
    if BUILDER_TAKEN.try_init(()).is_none() {
        return Err(MessageRamBuilderError::BuilderTaken);
    }
    let end = crate::pac::FDCAN_MSGRAM_LEN_WORDS as u16 - 4;
    Ok(MessageRamBuilder {
        pos: 0,
        end,
        layout: MessageRamLayout::default(),
        _phantom: Default::default(),
    })
}

impl<S> MessageRamBuilder<S> {
    fn into_state<S2>(self) -> MessageRamBuilder<S2> {
        MessageRamBuilder {
            pos: self.pos,
            end: self.end,
            layout: self.layout,
            _phantom: Default::default(),
        }
    }
}

macro_rules! check_and_advance {
    ($self:ident, $max_elements:expr, $len:expr, $element_size_words:expr, $dest:ident) => {
        if $len > $max_elements {
            return Err(MessageRamBuilderError::TooManyElements);
        }
        let new_pos = $self.pos + ($len as u16) * $element_size_words * 4;
        if new_pos > $self.end {
            return Err(MessageRamBuilderError::OutOfMemory);
        }
        paste! {
            $self.layout.[<$dest _addr>] = $self.pos;
            $self.layout.[<$dest _len>] = $len;
        }
        $self.pos = new_pos;
    };
}

impl MessageRamBuilder<ElevenBitFilters> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 28;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 128;

    /// Allocate zero or more 11-bit filters and move to the next step.
    pub fn allocate_11bit_filters(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<TwentyNineBitFilters>, MessageRamBuilderError> {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 1, eleven_bit_filters);
        Ok(self.into_state())
    }

    /// Merge this builder with the other. Useful if doing full re-init and re-layout of multiple CAN instances.
    pub fn recombine(&mut self, other: MessageRamBuilder<ElevenBitFilters>) {
        todo!()
    }
}

impl MessageRamBuilder<TwentyNineBitFilters> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 8;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more 29-bit filters and move to the next step.
    pub fn allocate_29bit_filters(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<RxFifo0>, MessageRamBuilderError> {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, twenty_nine_bit_filters);
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxFifo0> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 3;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more RX FIFO0 elements and move to the next step.
    pub fn allocate_rx_fifo0_buffers(
        mut self,
        len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<RxFifo1>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2 + data_size.words(),
            rx_fifo0
        );
        self.layout.rx_fifo0_data_size = data_size;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxFifo1> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 3;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more RX FIFO1 elements and move to the next step.
    pub fn allocate_rx_fifo1_buffers(
        mut self,
        len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<RxBuffers>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2 + data_size.words(),
            rx_fifo1
        );
        self.layout.rx_fifo1_data_size = data_size;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxBuffers> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 3;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate space for 3 messages (debug messages A, B and C) and move to the next step.
    pub fn allocate_rx_buffers(
        mut self,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<TxEventFifo>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            3,
            2 + data_size.words(),
            rx_buffers
        );
        self.layout.rx_buffers_data_size = data_size;
        Ok(self.into_state())
    }

    /// Skip allocating and move to the next step.
    pub fn skip_debug_buffers(self) -> MessageRamBuilder<TxEventFifo> {
        self.into_state()
    }
}

impl MessageRamBuilder<TxEventFifo> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 3;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 32;

    /// Allocate zero or more TX Event FIFO elements and move to the next step.
    pub fn allocate_tx_event_fifo_buffers(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<TxBuffers>, MessageRamBuilderError> {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, tx_event_fifo);
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<TxBuffers> {
    #[cfg(feature = "g0")]
    const MAX_ELEMENTS: u8 = 3;
    #[cfg(feature = "h7")]
    const MAX_ELEMENTS: u8 = 32;

    /// Allocate zero or more dedicated TX buffer elements + FIFO/Queue of size zero or more and get a MessageRamLayout.
    /// Also get a MessageRamBuilder in initial state to build layouts for other instances, if any.
    #[cfg(feature = "g0")]
    pub fn allocate_tx_buffers(
        mut self,
        dedicated_buffers_len: u8,
        fifo_or_queue_len: u8,
        data_size: DataFieldSize,
    ) -> Result<(MessageRamLayout, MessageRamBuilder<ElevenBitFilters>), MessageRamBuilderError>
    {
        let len = fifo_or_queue_len + dedicated_buffers_len;
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2 + data_size.words(),
            tx_buffers
        );
        self.layout.tx_fifo_or_queue_len = fifo_or_queue_len;
        self.layout.tx_buffers_data_size = data_size;
        let layout = core::mem::take(&mut self.layout);
        Ok((layout, self.into_state()))
    }

    /// Allocate zero or more TX buffer elements and move to the next step.
    #[cfg(feature = "h7")]
    pub fn allocate_tx_buffers(
        mut self,
        dedicated_buffers_len: u8,
        fifo_or_queue_len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<TriggerMemory>, MessageRamBuilderError> {
        let len = fifo_or_queue_len + dedicated_buffers_len;
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2 + data_size.words(),
            tx_buffers
        );
        self.layout.tx_fifo_or_queue_len = fifo_or_queue_len;
        self.layout.tx_buffers_data_size = data_size;
        Ok(self.into_state())
    }
}

#[cfg(feature = "h7")]
impl MessageRamBuilder<TriggerMemory> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more trigger elements and get a MessageRamLayout.
    /// Also get a MessageRamBuilder in initial state to build layouts for other instances, if any.
    pub fn allocate_triggers(
        mut self,
        len: u8,
    ) -> Result<(MessageRamLayout, MessageRamBuilder<ElevenBitFilters>), MessageRamBuilderError>
    {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, trigger_memory);
        let layout = core::mem::take(&mut self.layout);
        Ok((layout, self.into_state()))
    }
}
