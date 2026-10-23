# GravaStar Mercury HE keyboard protocol

Used by the magnetic (Hall effect) Mercury keyboards that the official web driver serves through
its "v2" app (V60 Pro, V75 Pro, V75, V75 Lite, …). Implementation:
[`src/protocol/keyboard.rs`](../src/protocol/keyboard.rs).

Items marked ✔ were confirmed on a Mercury V60 Pro ISO (`1CA6:2C0E`, firmware 0.4.1.0); ✎ marks
writes that were performed and read back successfully.

## Transport

| | |
| --- | --- |
| USB ids | VID `0x1CA6` or `0x31E3`; PIDs listed in `HE_KEYBOARD_PIDS` |
| HID collection | vendor usage page `0xFFB0`, usage `0x01` ✔ |
| Requests | 64-byte output report, report id 0: `[category, command, args…]` ✔ |
| Replies | 64-byte input report echoing category and command ✔ |

Multi-byte values are little endian; distances are micrometres (mm × 1000). One request at a time.
There is **no save/commit command** — the firmware persists every write on its own ✎.

Categories: 1 device, 2 global, 3 layout & keymap, 4 performance, 5 lighting, 6 advanced keys,
7 macros, 8 firmware update (never used by Orbit), 10 custom, 13 tri-mode (wireless boards).

## Device (category 1)

| Request | Reply |
| --- | --- |
| `01 01` protocol | bytes 2–5: main, sub, hardware, software version ✔ |
| `01 02` info | byte 2 type, 3 sub type, 4–7 board id (BE), 8–11 app version, 12–15 PCB, 29.. build time (ASCII) ✔ |
| `01 03` features | byte 2 switch type bits (2 = magnetic), 3 connection bits (1 USB, 2 2.4 GHz, 4 BT), 4 bit0 RGB ✔ |

## Global (category 2)

`ReadList = 0`, `Read = 1`, `Write = 2` unless noted.

| Request | Meaning |
| --- | --- |
| `02 03 00` / `01` / `02 n` | profile count / active profile / switch profile ✔ ✎ |
| `02 03 03 n` / `02 03 04 n text` | profile name read / write (UTF-8, ≤ 32 bytes) |
| `02 04 01` / `02 04 02 v` | OS mode: 0 Windows, 1 macOS ✔ |
| `02 05 00` / `01` / `02 v` | polling rates: 0 8K, 1 4K, 2 2K, 3 1K, 4 500, 5 250, 6 125 ✔ — **writing restarts the keyboard** |
| `02 06 00` / `02 06 01` | start / finish travel calibration |
| `02 08 00` | lighting zones: count, then `[index, effects, rows, cols]` per zone ✔ |
| `02 0C 00` | rapid-trigger precision in µm (byte 3) ✔ |
| `02 0D 01` / `02 0D 00 lo hi` | lighting sleep in minutes, 0 = never ✔ |
| `02 01 00` | factory reset of everything; reconnect after ~3 s |

## Layout & keymap (category 3)

`03 05 row` returns the **physical geometry** of a matrix row ✔. Every column is a u16 at byte
`3 + 2·col`: bits 15–11 = y × 4, bits 10–4 = x × 4 (key units), bits 3–0 = size code
(1 = 1u, 2 = 1.25u, 3 = 1.5u, 4 = 1.75u, 5 = 2u, 6 = 2.25u, 7 = 2.5u, 8 = 2.75u, 9 = 6.25u,
10 = 1×2u, 11 = ISO enter, 12 = knob). A reply with byte 2 = `0xFF` ends the table.

`03 01 layer row` returns 21 keycodes (u16 from byte 4) ✔. Codes `0x00xx` are HID usages,
`0x0001` is transparent, `0xF101` Fn, `0xF2xx`/`0xF3xx` control and lighting functions,
`0xF500 + n` macro n. Never put `0xF200` (long press = factory reset) or `0xF203`
(long press = calibration) on a key.

On the V60 Pro the keys sit in matrix rows 1–5; row 0 is empty ✔.

## Performance (category 4)

`04 01 row col` reads and `04 02 row col …` writes one key ✔ ✎:

| Byte | Field |
| --- | --- |
| 4 | mode: 0 fixed actuation, 1 rapid trigger |
| 5–6 | actuation point (fixed mode) |
| 7–8 | release point — the official driver always writes 0 |
| 9–10 | first actuation (rapid-trigger mode) |
| 11–12 | rapid-trigger press sensitivity |
| 13–14 | rapid-trigger release sensitivity |
| 15–16 / 17–18 | top / bottom deadzone (0–1 mm) |
| 19 | switch index (echo) |
| 20 | calibration flag — the official driver always writes 0 |
| 21–22 | switch id (echo); 13232 = GravaStar UFO switch, max travel 3.5 mm, otherwise 3.4 mm |
| 23–24 / 25–26 | switch range / coefficient (echo) |

There is no "all keys" command; the driver writes every selected key individually.

## Lighting (category 5)

| Request | Meaning |
| --- | --- |
| `05 01 zone 00` / `05 02 zone 00 on mode bright speed dir color` | effect settings; brightness and speed 0–100, direction 0 forward / 1 backward, colour = palette index ✔ ✎ |
| `05 01 zone 01` / `05 02 zone 01 + 8 × (B, G, R, 0)` | 8-colour palette ✔ |

Zones on the V60 Pro: 0 keys, 1 side light bar, 2 indicator ✔.

* Keys: 0 Drift, 1 Snow, 2 Blossom, 3 Sine, 4 Clouds, 5 Shadow, 6 Shine, 7 Pane, 8 Ripple,
  9 Flowers, 10 Rain, 11 per-key custom, 12 static, 13 Rainbow, 14 Laser, 15 Waves.
  Modes 0, 1, 2, 3, 10, 12, 14, 15 use the palette colour; palette index 0 means multicolour.
* Side bar: 0 static, 1 breathing, 2 marquee, 3 rainbow, 4 colour cycle, 5 laser,
  6 reacts to keys, 7 follows the key lighting.
* Indicator: 0 static, 1 follows the key lighting.

## Never sent by Orbit

`08 xx` (bootloader / firmware flash), `02 02` (unused save command), `02 0F 01` (restart),
`02 09` (default switch), `0A 01 01 FF` (wipe settings at power-off), `10 02 …` (gamepad mode,
re-enumerates the device), display/voice/haptic categories `0C`, `0E`, `0F`.
