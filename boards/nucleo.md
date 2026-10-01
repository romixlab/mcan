# Nucleo boards: NUCLEO-G0B1RE, NUCLEO-H533RE

Bare dev boards **without CAN transceivers**. Use them for:

1. Software-only tests and internal / external loopback (board + on-board ST-LINK only).
2. **Single-wire CAN without transceivers** between instances on the same MCU and/or other boards (see below).

The CAN pins are not fixed by the board. Choose them when the HIL crate is written, avoid pins the Nucleo already
uses (ST-LINK VCP, user LED, user button, USB, SWD), and record the choice here.

| Board | MCU | Chip feature | FDCAN instances | Message RAM | HIL crate |
|---|---|---|---|---|---|
| NUCLEO-G0B1RE | STM32G0B1RET6 | `g0` | FDCAN1 @ 0x4000_6400, FDCAN2 @ 0x4000_6800 (lite) | 0x4000_B400 (FDCAN1), 0x4000_B750 (FDCAN2) | `hil/nucleo_g0b1re` |
| NUCLEO-H533RE | STM32H533RET6 | `h5` (not implemented yet) | FDCAN1 @ 0x4000_A400, FDCAN2 @ 0x4000_A800 (lite) | 0x4000_AC00 (FDCAN1), 0x4000_AF50 (FDCAN2) | `hil/nucleo_h533re` |

Addresses are from stm32-metapac 21 (stm32-data). Each lite instance has a fixed 0x350-byte (212-word) RAM block.

## FDCAN pin options (from stm32-data)

| MCU | FDCAN1 TX / RX | FDCAN2 TX / RX | AF |
|---|---|---|---|
| STM32G0B1RE | TX PA12, PB9, PC5, PD1 / RX PA11, PB8, PC4, PD0 | TX PB1, PB6, PB13, PC3 / RX PB0, PB5, PB12, PC2 | 3 |
| STM32H533RE | TX PA12, PB7 / RX PA11, PB8 | TX PA10, PB3, PB6, PB13 / RX PA0, PB5, PB12 | 9 |

### Recommended pins (proposed, nothing wired yet)

| Board | FDCAN1 TX / RX | FDCAN2 TX / RX | AF | Single-wire pull-up |
|---|---|---|---|---|
| NUCLEO-G0B1RE | **PB9 / PB8** (Arduino D14 / D15) | **PB13 / PB12** (morpho) | 3 | 1 kΩ to 3V3 |
| NUCLEO-H533RE | **PB7 / PB8** (PB8 = Arduino D15, PB7 morpho) | **PB13 / PB12** (morpho) | 9 | 1 kΩ to 3V3 |

Why these:

- They avoid the pins the Nucleo-64 boards use by default: the ST-LINK virtual COM port (PA2/PA3), the user LED
  (PA5), the user button (PC13), SWD (PA13/PA14), the LSE (PC14/PC15) and USB (PA11/PA12). The NUCLEO-H533RE has
  a USB-C user port, which is why its FDCAN1 uses PB7/PB8 rather than PA11/PA12.
- The two boards are as symmetric as the MCUs allow (same RX pin for FDCAN1, same FDCAN2 pins), so one HIL wiring
  scheme and one harness layout fit both.
- FDCAN1 RX on PB8 matches B135 / B125 / B129, which keeps the pin tables easy to remember.
- **Before wiring, check the board's user manual** for solder bridges and default functions on these pins. This
  table is based on stm32-data AF tables and the common Nucleo-64 assignments, not yet on the board manuals.

### Recommended single-wire wiring

```
3V3 ──[1 kΩ]──┬──────────┬──────────┬──────────┬───── (to other boards, plus a common GND)
              │          │          │          │
          FDCAN1 TX  FDCAN1 RX  FDCAN2 TX  FDCAN2 RX
         (open-drain)           (open-drain)
```

- **TX pins configured as open-drain AF**, no internal pull-up (the external 1 kΩ sets the edges). No diodes
  needed. If a TX pin can't be made open-drain, use a series Schottky (e.g. BAT54, cathode at TX).
- **1 kΩ** with a short wire (< 20 cm, about 40 pF including pins) gives a recessive rise time of about
  2.2·RC ≈ 90 ns. That's comfortable for 500 kbit/s and 1 Mbit/s nominal, and likely 2 Mbit/s data phase.
  Each node sinks about 3.3 mA when dominant. For faster data phases, try 470 Ω (about 7 mA). Measure, and
  record the working rates here.
- Board ↔ board: extend the same wire and connect GNDs. Use one pull-up for the whole bus, not one per board.

## Single-wire CAN without transceivers

Several FDCAN controllers can share one wire without transceivers, as a wired-AND bus (dominant = low):

- One common bus wire with a **pull-up to 3.3 V** (recessive = high). Every node's **RX** connects straight to it.
- Every node's **TX** drives the wire **only low**. Either configure TX as an **open-drain** alternate function,
  or keep push-pull and put a Schottky diode in series (cathode at TX, anode at the bus).
- All nodes share **GND**. Keep the wire short. The pull-up value and wire capacitance limit the bit rate
  (RC rise time), so expect low CAN FD data-phase rates. Measure, and record working rates here.
- Works between two instances on the same MCU (FDCAN1 ↔ FDCAN2 on one Nucleo, giving two-node tests with no extra
  hardware) and between boards (e.g. Nucleo ↔ B129 FDCAN2 header pins).
- Not a substitute for transceiver tests: no differential signalling, and no bus-off from shorted CAN_H/CAN_L.
  Bus errors can still be injected by holding the wire low or by mismatched bit rates.
- The driver's pin setup must allow open-drain TX. `mcan::embassy::configure_pins!` currently hard-codes
  push-pull (FEATURES.md I4).
