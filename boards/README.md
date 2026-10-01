# HIL test boards

Pinouts and hardware notes for every board used for hardware-in-the-loop tests. The list of boards and their
use in tests lives in the *Test boards* table in [FEATURES.md](../FEATURES.md). This directory holds the
details.

Custom boards are named `BnnnR`: `nnn` is the board number, `R` the revision letter (e.g. B135A, B135B). One
file per board number covers all its revisions and lists the differences between them.

| Board | File | MCU | CAN channels |
|---|---|---|---|
| B125A, B125B | [b125.md](b125.md) | STM32H725IGKx | 2: FDCAN1 + FDCAN3, separately isolated, switchable terminators. Also Ethernet/PoE, USB HS. |
| B135A, B135B | [b135.md](b135.md) | STM32H725IGKx | 1 (isolated, switchable terminator). B135B also has FDCAN3 on header J203 (no transceiver). |
| B129A (CANnify Micro HV) | [b129.md](b129.md) | STM32G0B1CETxN | 1: FDCAN1 + TCAN1044V (non-isolated), no terminator |
| NUCLEO-G0B1RE | [nucleo.md](nucleo.md) | STM32G0B1RET6 | no transceivers: software, loopback, single-wire CAN |
| NUCLEO-H533RE | [nucleo.md](nucleo.md) | STM32H533RET6 | no transceivers: software, loopback, single-wire CAN |

## Extracting pinouts from KiCad netlists

`tools/netlist.py` parses KiCad S-expression netlists (`.net`, Eeschema export). It uses only the Python
standard library:

```sh
tools/netlist.py board.net info                 # title block, revision, sheets
tools/netlist.py board.net parts 'STM32|^U|^K'  # find the MCU, transceivers, relays, connectors
tools/netlist.py board.net pins U201            # MCU pin -> function -> net
tools/netlist.py board.net net 'FDCAN|TERM'     # who is on a net
tools/netlist.py board.net trace U201 C1 3      # follow a pin through series parts
diff <(tools/netlist.py revA.net pins U201) <(tools/netlist.py revB.net pins U201)  # revision diff
```

Netlists stay outside the repo unless the user decides otherwise. Record their path and export date in the board
file so they can be re-checked.
