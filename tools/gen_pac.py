#!/usr/bin/env python3
"""Generate the register access layer in `src/pac` from a pinned `stm32-metapac` release.

The register code is chiptool output that stm32-metapac generates from stm32-data. We copy it verbatim and
only rewrite the `crate::common` path. The RCC blocks are cut down to the registers and fields that the
driver actually uses (see `RCC_KEEP`). Nothing else is changed, so every generated file can be diffed
against upstream.

Usage:
    python3 tools/gen_pac.py           # (re)generate src/pac/*
    python3 tools/gen_pac.py --check   # exit 1 if src/pac is not what the generator would produce

To bump the source: change METAPAC_VERSION and METAPAC_SHA256 (the `cksum` field from
https://index.crates.io/st/m3/stm32-metapac), regenerate, review the diff and record the bump in
FEATURES.md (P7a, P11).

Tracked as FEATURES.md P7a.
"""

import argparse
import glob
import hashlib
import io
import os
import re
import sys
import tarfile
import urllib.request

METAPAC_VERSION = "21.0.0"
METAPAC_SHA256 = "e74b78632cea498cfb28386a29f8bfae7476d6570a78733eb5fecbee66c2f4ce"

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(REPO, "src", "pac")

# Registers (accessor name on the block) and fields (getter name, the setter `set_<name>` is kept as well)
# kept from the RCC blocks. Add entries here when the driver needs more, then regenerate.
RCC_KEEP = {
    "h7": {
        "cr": ["hseon"],
        "pllcfgr": ["divqen"],
        "d2ccip1r": ["fdcansel"],
        "apb1henr": ["fdcanen"],
        "apb1hrstr": ["fdcanrst"],
    },
    "g0": {
        "cr": ["hseon"],
        "pllcfgr": ["pllqen"],
        "ccipr2": ["fdcansel"],
        "apbenr1": ["fdcanen"],
        "apbrstr1": ["fdcanrst"],
    },
}

# (output file, file inside the crate, RCC trim key or None)
OUTPUTS = [
    ("common.rs", "src/common.rs", None),
    ("fdcan_h7.rs", "src/peripherals/can_fdcan_h7.rs", None),
    ("fdcan_v1.rs", "src/peripherals/can_fdcan_v1.rs", None),
    ("rcc_h7.rs", "src/peripherals/rcc_h7.rs", "h7"),
    ("rcc_g0.rs", "src/peripherals/rcc_g0x1.rs", "g0"),
]

# Chip used to cross-check the hand-written addresses in src/pac/mod.rs (`mapping` modules).
ADDRESS_CHECK_CHIPS = {"h7": "stm32h725ig", "g0": "stm32g0b1ce"}
ADDRESS_CHECK_CONSTS = {
    "RCC_REGISTER_BLOCK_ADDR": "RCC",
    "FDCAN1_REGISTER_BLOCK_ADDR": "FDCAN1",
    "FDCAN2_REGISTER_BLOCK_ADDR": "FDCAN2",
    "FDCAN3_REGISTER_BLOCK_ADDR": "FDCAN3",
}
# The message RAM base is called FDCANRAM (H7, one shared RAM) or FDCANRAM1 (lite, one block per instance).
MSGRAM_NAMES = ["FDCANRAM", "FDCANRAM1"]


# ---------------------------------------------------------------------------------------------------------
# Fetching


def fetch_crate() -> bytes:
    name = f"stm32-metapac-{METAPAC_VERSION}.crate"
    cached = glob.glob(os.path.expanduser(f"~/.cargo/registry/cache/*/{name}"))
    for path in cached:
        with open(path, "rb") as f:
            data = f.read()
        if hashlib.sha256(data).hexdigest() == METAPAC_SHA256:
            return data
    url = f"https://static.crates.io/crates/stm32-metapac/{name}"
    print(f"downloading {url}", file=sys.stderr)
    with urllib.request.urlopen(url, timeout=60) as r:
        data = r.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != METAPAC_SHA256:
        sys.exit(f"sha256 mismatch for {name}: got {digest}, expected {METAPAC_SHA256}")
    return data


def read_member(tar: tarfile.TarFile, path: str) -> str:
    f = tar.extractfile(f"stm32-metapac-{METAPAC_VERSION}/{path}")
    if f is None:
        sys.exit(f"{path} not found in stm32-metapac {METAPAC_VERSION}")
    return f.read().decode()


# ---------------------------------------------------------------------------------------------------------
# A tiny item splitter for chiptool output (enough for its regular structure, not a Rust parser).


def split_items(body: str) -> list[str]:
    """Split a module / impl body into items. Each item keeps its leading whitespace and attributes."""
    items = []
    start = 0
    depth = 0  # () [] {} combined
    i = 0
    in_str = False
    while i < len(body):
        c = body[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c in "([{":
            depth += 1
        elif c in ")]":
            depth -= 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                items.append(body[start : i + 1])
                start = i + 1
        elif c == ";" and depth == 0:
            items.append(body[start : i + 1])
            start = i + 1
        i += 1
    if body[start:].strip():
        sys.exit(f"unterminated item: {body[start:start + 200]!r}")
    items.append(body[start:])  # trailing whitespace
    return items


def strip_attrs(item: str) -> str:
    """Item text without leading whitespace and outer attributes."""
    s = item.lstrip()
    while s.startswith("#["):
        depth = 0
        in_str = False
        for j, c in enumerate(s):
            if in_str:
                if c == '"' and s[j - 1] != "\\":
                    in_str = False
            elif c == '"':
                in_str = True
            elif c == "[":
                depth += 1
            elif c == "]":
                depth -= 1
                if depth == 0:
                    s = s[j + 1 :].lstrip()
                    break
    return s


def item_name(item: str) -> str | None:
    s = strip_attrs(item)
    for pat in (
        r"^pub (?:const )?fn (\w+)",
        r"^pub (?:struct|enum) (\w+)",
        r"^impl From<(\w+)> for u8",
        r"^(?:unsafe )?impl [\w:<>]+ for (\w+)",
        r"^impl (\w+)",
        r"^pub mod (\w+)",
    ):
        m = re.match(pat, s)
        if m:
            return m.group(1)
    return None


def block_body(item: str) -> tuple[str, str, str]:
    """Split `head { body }` into (head + '{', body, '}')."""
    open_idx = strip_attrs_offset(item)
    open_idx = item.index("{", open_idx)
    return item[: open_idx + 1], item[open_idx + 1 : -1], item[-1]


def strip_attrs_offset(item: str) -> int:
    return len(item) - len(strip_attrs(item))


# ---------------------------------------------------------------------------------------------------------
# RCC trimming


def trim_rcc(src: str, keep: dict[str, list[str]]) -> str:
    items = split_items(src)
    out = []
    reg_types = {}  # register type name -> kept fields
    for item in items:
        s = strip_attrs(item)
        if re.match(r"^impl Rcc \{", s):
            head, body, tail = block_body(item)
            kept = []
            found = set()
            for fn in split_items(body):
                name = item_name(fn)
                if name in ("from_ptr", "as_ptr") or name is None:
                    kept.append(fn)
                elif name in keep:
                    found.add(name)
                    m = re.search(r"Reg<regs::(\w+),", fn)
                    if not m:
                        sys.exit(f"cannot find register type of rcc accessor {name}")
                    reg_types[m.group(1)] = keep[name]
                    kept.append(fn)
            missing = set(keep) - found
            if missing:
                sys.exit(f"rcc registers not found upstream: {sorted(missing)}")
            out.append(head + "".join(kept) + tail)
        elif re.match(r"^pub mod regs \{", s):
            head, body, tail = block_body(item)
            kept = []
            for it in split_items(body):
                name = item_name(it)
                if name is None:
                    kept.append(it)  # whitespace
                    continue
                if name not in reg_types:
                    continue
                st = strip_attrs(it)
                if st.startswith(f"impl {name} {{"):
                    fhead, fbody, ftail = block_body(it)
                    fields = reg_types[name]
                    want = set(fields) | {f"set_{f}" for f in fields}
                    fns = [f for f in split_items(fbody) if item_name(f) in want or item_name(f) is None]
                    got = {item_name(f) for f in fns} - {None}
                    if got != want:
                        sys.exit(f"rcc {name}: fields not found upstream: {sorted(want - got)}")
                    kept.append(fhead + "".join(fns) + ftail)
                elif "core::fmt::Debug for" in st or "defmt::Format for" in st:
                    continue  # would reference removed fields
                else:
                    kept.append(it)
            regs_text = "".join(kept)
            out.append(head + regs_text + tail)
        elif re.match(r"^pub mod vals \{", s):
            vals_item = item
            out.append(None)  # placeholder, needs the kept regs
        else:
            out.append(item)
    used_vals = set(re.findall(r"super::vals::(\w+)", "".join(o for o in out if o)))
    head, body, tail = block_body(vals_item)
    kept = [it for it in split_items(body) if item_name(it) is None or item_name(it) in used_vals]
    vals_text = head + "".join(kept) + tail
    return "".join(vals_text if o is None else o for o in out)


# ---------------------------------------------------------------------------------------------------------


def generate(tar: tarfile.TarFile) -> dict[str, str]:
    files = {}
    for out_name, member, rcc in OUTPUTS:
        text = read_member(tar, member)
        if rcc is not None:
            text = trim_rcc(text, RCC_KEEP[rcc])
        text = text.replace("crate::common::", "crate::pac::common::")
        header = (
            f"// Generated by tools/gen_pac.py from stm32-metapac {METAPAC_VERSION} ({member}"
            + (", trimmed" if rcc else "")
            + ").\n// Do not edit by hand: change the generator and re-run it (FEATURES.md P7a).\n\n"
        )
        files[out_name] = header + text
    return files


def check_addresses(tar: tarfile.TarFile) -> list[str]:
    """Compare the hand-written `mapping` constants in src/pac/mod.rs with stm32-metapac's chip files."""
    errors = []
    with open(os.path.join(OUT_DIR, "mod.rs")) as f:
        mod_rs = f.read()
    for feature, chip in ADDRESS_CHECK_CHIPS.items():
        m = re.search(
            r'#\[cfg\(feature = "' + feature + r'"\)\]\s*pub\(crate\) mod mapping \{(.*?)\n\}', mod_rs, re.S
        )
        if not m:
            errors.append(f"mod.rs: no mapping module for {feature}")
            continue
        ours = {k: int(v.replace("_", ""), 16) for k, v in re.findall(r"const (\w+): [^=]+= (0x[0-9a-fA-F_]+)", m.group(1))}
        pac = read_member(tar, f"src/chips/{chip}/pac.rs")
        theirs = {
            k: int(v.replace("_", ""), 16)
            for k, v in re.findall(r"pub const (\w+): [\w:]+ = unsafe \{ [\w:]+::from_ptr\((0x[0-9a-f_]+)usize", pac)
        }
        for const, periph in ADDRESS_CHECK_CONSTS.items():
            if const in ours and ours[const] != theirs.get(periph):
                errors.append(f"{feature}: {const} = {ours[const]:#x}, {chip} {periph} = {hex(theirs[periph]) if periph in theirs else None}")
            if const not in ours and periph in theirs:
                errors.append(f"{feature}: {chip} has {periph} but mod.rs has no {const}")
        ram = next((theirs[n] for n in MSGRAM_NAMES if n in theirs), None)
        if ours.get("FDCAN_MSGRAM_ADDR") != ram:
            errors.append(f"{feature}: FDCAN_MSGRAM_ADDR = {hex(ours.get('FDCAN_MSGRAM_ADDR', 0))}, {chip} = {hex(ram or 0)}")
    return errors


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true", help="only check, do not write")
    args = ap.parse_args()

    tar = tarfile.open(fileobj=io.BytesIO(fetch_crate()), mode="r:gz")
    files = generate(tar)
    errors = check_addresses(tar)

    for name, text in files.items():
        path = os.path.join(OUT_DIR, name)
        current = open(path).read() if os.path.exists(path) else None
        if current == text:
            continue
        if args.check:
            errors.append(f"src/pac/{name} is out of date, run tools/gen_pac.py")
        else:
            with open(path, "w") as f:
                f.write(text)
            print(f"wrote src/pac/{name}")

    for e in errors:
        print(f"error: {e}", file=sys.stderr)
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
