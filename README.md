# Snow

<img width="480" height="270" alt="Snow_005" src="https://github.com/user-attachments/assets/ebc66bf8-e72e-4e8d-b636-ae9a7fe5e268" />

A real-time snow demo built on the [Nightshade](https://github.com/matthewjberger/nightshade) engine.

[Play it in the browser](https://matthewberger.dev/snow/)

You walk a snowfield that remembers you. Footprints stay where you put them, the
board cuts a groove and throws a wall of snow when you carve, and five water
spells bend, erupt and freeze the ground you are standing on.

## Controls

| Input | Action |
|-------|--------|
| `WASD` | Move |
| Mouse | Look |
| `Shift` | Run |
| `Space` | Jump |
| Right mouse | Snow-surf |
| Scroll | Zoom the camera arm |
| `1` | Sweep, a crescent of slush thrown along the ground |
| `2` (hold) | Ribbon, a stream of water on the hand, released as a throw |
| `3` | Bloom, an eruption where you are looking |
| `4` | Crystallize, an ice formation planted on a spiral |
| `5` | Vortex, a column that strips the snow and gives it back |
| `F1` | Settings and frame cost |
| `Esc` | Release the cursor, then quit |

A gamepad works too. Left stick moves, right stick looks, right trigger surfs,
left stick click runs, right shoulder jumps, and the face buttons cast.

## What is in it

The terrain is a clipmap over a baked heightfield, shaded from a sky the demo
integrates itself into nine harmonic coefficients and a sun colour. Snow
deformation is a persistent GPU buffer that every contact writes into: boots, the
board, and each spell. It relaxes back over time, so a field fills in behind you
at whatever rate the slider says.

The character has no rig on disk. The skeleton is solved each frame, the garments
are simulated cloth, the fur is shell geometry, and the legs are two-bone IK onto
the snow the feet are about to land in.

Everything drawn goes through the demo's own render graph passes and its own
screen-space chain: cascaded shadows, a depth prepass, screen-space reflections,
light shafts, bloom, depth of field, a temporal resolve, tonemapping and sharpen.
The engine's equivalents are switched off, so what you see is this repository's.

Open the settings with `F1`. Every control in it does something, including the
debug views, which will show you the deformation buffer, the cascade splits, the
shadow term and the pixel footprint on their own.

## Quickstart

```bash
# native
just run

# wasm (webgpu)
just run-wasm

# steam deck
just build-steamdeck
just deploy-steamdeck        # copies binary to ~/Downloads on deck
just deploy-steamdeck-quick  # copies as 'game' for quick launching
```

> All chromium-based browsers like Brave, Vivaldi, Chrome, etc support WebGPU.
> Firefox also [supports WebGPU](https://mozillagfx.wordpress.com/2025/07/15/shipping-webgpu-on-windows-in-firefox-141/) now starting with version `141`.

## Prerequisites

* [just](https://github.com/casey/just)
* [trunk](https://trunkrs.dev/) (for web builds)
* [cross](https://github.com/cross-rs/cross) (for Steam Deck builds)
  * Requires Docker (macOS/Linux) or Docker Desktop (Windows)

> Run `just` with no arguments to list all commands

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
