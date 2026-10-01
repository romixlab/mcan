#!/usr/bin/env python3
"""Inspect KiCad S-expression netlists (.net, "export" version E) of HIL test boards.

Usage:
  tools/netlist.py FILE info                  # title block, sheets
  tools/netlist.py FILE parts [REGEX]         # components (ref, value, part, sheet), filtered by regex on any of them
  tools/netlist.py FILE pins REF [REGEX]      # pin -> pin function -> net for one component (e.g. the MCU)
  tools/netlist.py FILE net REGEX             # all nodes of nets whose name matches REGEX
  tools/netlist.py FILE trace REF PIN [DEPTH] # follow a pin's net through 2-pin passives (R/L/FB/jumpers), DEPTH hops

Only the Python standard library is used.
"""
import re
import sys


def tokenize(text):
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in "()":
            yield c
            i += 1
        elif c.isspace():
            i += 1
        elif c == '"':
            j = i + 1
            buf = []
            while text[j] != '"':
                if text[j] == "\\":
                    j += 1
                buf.append(text[j])
                j += 1
            yield ("s", "".join(buf))
            i = j + 1
        else:
            j = i
            while j < n and not text[j].isspace() and text[j] not in '()"':
                j += 1
            yield ("s", text[i:j])
            i = j


def parse(text):
    stack = [[]]
    for t in tokenize(text):
        if t == "(":
            stack.append([])
        elif t == ")":
            node = stack.pop()
            stack[-1].append(node)
        else:
            stack[-1].append(t[1])
    return stack[0][0]


def children(node, name):
    return [c for c in node[1:] if isinstance(c, list) and c and c[0] == name]


def child(node, name):
    c = children(node, name)
    return c[0] if c else None


def val(node, name, default=""):
    c = child(node, name)
    if c is None or len(c) < 2:
        return default
    return c[1] if isinstance(c[1], str) else default


class Netlist:
    def __init__(self, path):
        self.root = parse(open(path, encoding="utf-8").read())
        self.design = child(self.root, "design")
        self.comps = {}
        for comp in children(child(self.root, "components"), "comp"):
            ref = val(comp, "ref")
            props = {val(p, "name"): val(p, "value") for p in children(comp, "property")}
            lib = child(comp, "libsource")
            self.comps[ref] = {
                "ref": ref,
                "value": val(comp, "value"),
                "footprint": val(comp, "footprint"),
                "part": val(lib, "part") if lib else "",
                "sheet": props.get("Sheetname", ""),
                "props": props,
            }
        self.nets = {}  # name -> [(ref, pin, pinfunction, pintype)]
        self.pin_net = {}  # (ref, pin) -> net name
        for net in children(child(self.root, "nets"), "net"):
            name = val(net, "name")
            nodes = []
            for nd in children(net, "node"):
                ref, pin = val(nd, "ref"), val(nd, "pin")
                nodes.append((ref, pin, val(nd, "pinfunction"), val(nd, "pintype")))
                self.pin_net[(ref, pin)] = name
            self.nets[name] = nodes

    def pins_of(self, ref):
        rows = []
        for name, nodes in self.nets.items():
            for r, pin, func, _ in nodes:
                if r == ref:
                    rows.append((pin, func, name))
        return sorted(rows, key=lambda r: (len(r[0]), r[0]))


BIG_NET = 12  # nets with more nodes (power, GND) are not expanded by `trace`


def is_passthrough(comp, nodes_count):
    # 2-pin series parts worth following: resistors, ferrites, inductors, jumpers, net ties
    return comp and comp["ref"][:1] in ("R", "L", "F", "J", "N") and nodes_count == 2 and not comp["ref"].startswith("J")


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(1)
    nl = Netlist(sys.argv[1])
    cmd, args = sys.argv[2], sys.argv[3:]
    if cmd == "info":
        print("source:", val(nl.design, "source"), "\ndate:", val(nl.design, "date"))
        for tv in children(nl.design, "textvar"):
            print("textvar:", val(tv, "name"), "=", tv[2] if len(tv) > 2 else "")
        for sh in children(nl.design, "sheet"):
            tb = child(sh, "title_block")
            print("sheet:", val(sh, "name"), "|", val(tb, "title"), "rev", val(tb, "rev"), val(tb, "date"))
        print(len(nl.comps), "components,", len(nl.nets), "nets")
    elif cmd == "parts":
        rx = re.compile(args[0], re.I) if args else None
        for c in sorted(nl.comps.values(), key=lambda c: (c["ref"].rstrip("0123456789"), int(re.sub(r"\D", "", c["ref"]) or 0))):
            line = f'{c["ref"]:8} {c["value"]:32} {c["part"]:28} {c["sheet"]}'
            if rx is None or rx.search(line):
                print(line)
    elif cmd == "pins":
        rx = re.compile(args[1], re.I) if len(args) > 1 else None
        for pin, func, net in nl.pins_of(args[0]):
            line = f"{pin:6} {func:28} {net}"
            if rx is None or rx.search(line):
                print(line)
    elif cmd == "net":
        rx = re.compile(args[0], re.I)
        for name, nodes in sorted(nl.nets.items()):
            if rx.search(name):
                print(name)
                for ref, pin, func, _ in nodes:
                    c = nl.comps.get(ref, {})
                    print(f'    {ref:8} pin {pin:5} {func:24} {c.get("value", "")}')
    elif cmd == "trace":
        ref, pin = args[0], args[1]
        depth = int(args[2]) if len(args) > 2 else 3
        seen = set()
        frontier = [(ref, pin, 0)]
        while frontier:
            r, p, d = frontier.pop(0)
            net = nl.pin_net.get((r, p))
            if net is None or net in seen:
                continue
            seen.add(net)
            print("  " * d + f"{r}.{p} -> net {net}")
            if len(nl.nets[net]) > BIG_NET:
                print("  " * d + f"    ({len(nl.nets[net])} nodes, not expanded; use `net`)")
                continue
            for r2, p2, func, _ in nl.nets[net]:
                if (r2, p2) == (r, p):
                    continue
                c = nl.comps.get(r2)
                print("  " * d + f"    {r2:8} pin {p2:5} {func:24} {c['value'] if c else ''}")
                pins = [x for x in nl.pin_net if x[0] == r2]
                if d < depth and is_passthrough(c, len(pins)):
                    other = [x for x in pins if x[1] != p2][0]
                    frontier.append((r2, other[1], d + 1))
    else:
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
