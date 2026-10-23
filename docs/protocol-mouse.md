# GravaStar mouse protocol (Compx chipset)

GravaStar mice use a Compx chipset. The same protocol is spoken by the mouse over its cable and
through the 2.4 GHz receiver. Implementation: [`src/protocol/mouse.rs`](../src/protocol/mouse.rs).

Items marked ✔ were read back from real hardware and decode as described (receiver `3554:F54B`,
mouse firmware v2.17, PAW3950). Unmarked items come from the official driver only.

## Transport

| | |
| --- | --- |
| USB ids | VID `0x3554`; receiver PIDs `F54B F575 F548 F577`, wired PIDs `F576 F549` |
| HID collection | vendor usage page `0xFF02`, usage `0x02` ✔ (the one with input + output report 8) |
| Requests | output report `0x08`, 16-byte payload ✔ |
| Replies | input report `0x08`, 16-byte payload, same header as the request ✔ |

Payload layout:

```
byte 0     command
byte 1     0 in requests; status in replies (0 ok, 1 failed/unsupported)
byte 2-3   flash address, big endian (0x07/0x08 only)
byte 4     data length (≤ 10)
byte 5-14  data
byte 15    checksum: (0x08 + sum of all 16 bytes) & 0xFF == 0x55      ✔ (replies use it too)
```

So `byte15 = (0x4D − sum(byte0..14)) & 0xFF`. Only one request may be in flight; the official
driver waits 200 ms per attempt and retries up to 5 times. The mouse may push an unsolicited
`0x0A` status report at any time (for example after the DPI button was pressed).

## Commands

| Cmd | Purpose | Reply |
| --- | --- | --- |
| `01` | identify (send 4 random bytes + 4 zero bytes, len 8) | byte 9 = customer id (18 = GravaStar), byte 10 = model id, byte 11 = link type ✔ |
| `03` | is the mouse linked/awake? | byte 5 = 1 when online ✔ |
| `04` | battery | byte 5 = %, byte 6 = charging, byte 7-8 = millivolts (BE) ✔ |
| `07` | write flash | echo of the written data |
| `08` | read flash | data at byte 5.. ✔ |
| `0E` | active onboard profile | byte 5 ✔ |
| `12` | mouse firmware | `v{byte5}.{hex(byte6)}` ✔ |
| `1D` | receiver firmware | older receivers do not answer ✔ |

Model ids (customer id 18): 1 Mercury M1 and 2 Mercury M2 → PAW3395 (max 26 000 DPI);
3 Mercury X Pro ✔, 4 Mercury X and 5 Mercury X Air → PAW3950 (max 32 000 DPI).
Link types: 0/1/4/5 = receiver with 1K/4K/2K/8K max polling, 2/3 = cable with 1K/8K.

Commands Orbit never sends: `05`/`06` (pairing), `09` (factory reset), `31` (RF channel), anything
in `B0`–`B7` (collides with bootloader opcodes) and any firmware-update traffic.

## Settings flash map

Every setting is a byte at a fixed address. Scalars are stored as a pair `[v, 0x55 − v]`; longer
records end with a check byte that makes the record sum to `0x55`. There is no separate save
command — each `07` write takes effect immediately.

| Address | Field | Encoding |
| --- | --- | --- |
| 0 | polling rate | 125→`08`, 250→`04`, 500→`02`, 1000→`01`, 2000→`10`, 4000→`20`, 8000→`40` ✔ |
| 2 | number of active DPI stages | 1..8 ✔ |
| 4 | current DPI stage | 0-based ✔ |
| 10 | lift-off distance | 3395: 1 = 1 mm, 2 = 2 mm · 3950: 3 = 0.7 mm, 1 = 1 mm, 2 = 2 mm ✔ |
| 12 + 4·i | DPI stage i | `[xLo, yLo, ext, chk]`, see below ✔ |
| 44 + 4·i | DPI stage colour | `[R, G, B, chk]` ✔ |
| 96 + 4·i | button i (0..5) | `[type, param_hi, param_lo, chk]` ✔ |
| 160 | lighting | `[mode, R, G, B, speed, brightness, chk]` ✔ |
| 167 | lighting on/off | pair ✔ |
| 169 | debounce | ms, 0..15 ✔ |
| 171 | Motion Sync | pair ✔ |
| 173 | sleep timeout | ×10 s: 1, 3, 6, 12, 18, 30, 60, 90 ✔ |
| 175 | angle snapping | pair ✔ |
| 177 | ripple control | pair ✔ |
| 179 | lights off while moving | pair ✔ |
| 185 | sensor mode | 0 = LP, 1 = HP ✔ |

### DPI encoding

`raw = dpi / 50 − 1` (10 bits; bits 8–9 go into `ext` bits 2–3 for X and 6–7 for Y). Above the
sensor's base range (26 000 on PAW3395, 30 000 on PAW3950) the step becomes 100 DPI,
`raw = dpi / 100 − 1`, and the ×2 flag is set (`ext` bit 0 for X, bit 4 for Y).
Example: 1600 DPI → `1F 1F 00 17` ✔.

### Buttons

`type` 0 = disabled, 1 = mouse button (`param` = `0x0100` left, `0x0200` right, `0x0400` middle,
`0x0800` back, `0x1000` forward), 2 = DPI cycle (`0x0100`), 5 = shortcut, 6 = macro.
Shortcut records live at `256 + 32·i`, macros at `768 + 384·i`.

### Lighting modes

0 off, 1 rainbow, 2 breathing (colour), 3 static (colour), 4 neon, 5 rainbow breathing,
6 static rainbow. Brightness and speed 1..9.

## Battery

When the voltage field is non-zero the official driver ignores byte 5 and maps millivolts to a
percentage with this table (5 % steps, 0 %…100 %):
`3050 3420 3480 3540 3600 3660 3720 3760 3800 3840 3880 3920 3940 3960 3980 4000 4020 4040 4060 4080 4110`.
