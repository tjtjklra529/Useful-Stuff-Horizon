# Useful Stuff — Horizon 0.0.1

An experimental native Horizon adaptation of [Useful Stuff](https://www.curseforge.com/minecraft/mc-mods/useful-stuff-tjtjklra529), by **tjtjklra529**. It targets Minecraft 26.3 through the exact Horizon SDK recorded in `horizon-mod.toml`. Java loader releases target a different game version and cannot be installed as Horizon mods.

## Included

- Obsidian hammer: directional 3×3 mining through the player's block-breaking path, including mixed materials; original item artwork and 1,250 durability.
- Slingshot: four original 16×16 charging frames, a 20-tick charge, flint consumed on full charge, custom elastic snap and “Slingshot fires” subtitle. Native first-person transforms and a two-arm third-person pose.
- Fletching table: an offset three-input flint junction and diagonal arrow assembly, with 42 modifier ingredients, different cores and shafts, colored ability lore, dyes and up to three distinct abilities. Flint and arrows retain their familiar names. Existing modified ammunition can be upgraded.
- Abilities include ricochets, fire, freezing, slowing, poison, glowing, levitation, blindness, weakness, knockback, weight and speed. Dye combinations cycle through a colored particle trail.
- Quiver: nine stacks of ammunition, an aligned selection row and automatic flint selection for the slingshot.
- Grasping I–IV on slingshots: missed flint returns after 4/3/2/1 seconds, within 16/24/32/48 blocks in the same dimension. Recall metadata travels with the saved projectile or waiting item. Hits consume ammunition; creative shots do not create recoverable flint.
- Torch pouch: four stacks of torches, double-right-click opening, delayed single-click placement and the original empty/filled 16×16 textures. Storage accepts ordinary torches only.
- Fried egg: furnace recipe, four hunger points and 2.4 saturation. Emerald apples, enchanted emerald apple buffs and rare chest loot. A held magnet attracts dropped items within three blocks.

## Differences from the Java releases

This is an initial Horizon adaptation, not a claim of complete Java feature parity.

- Menus currently use Horizon's standard container screen and native item slots. They do not reproduce the Java custom four-slot screen or its exact keyboard behavior.
- Quiver storage supports arrows, but automatic bow/crossbow feeding is not implemented in this release. Move arrows to inventory to use those weapons.
- The original levitation brewing integration and ordinary potion-to-ammunition conversion are not implemented. Explicit custom potion effects are supported; base potion-only inputs are rejected rather than producing effectless ammunition.
- Hammer extra-block outlines/cracks are not reproduced. Mining uses the player's destruction API; protection-mod compatibility still needs gameplay testing. Pouch placement checks build permission, reach, support and the world border, but is a direct world write rather than a vanilla item use.
- Cross-loader worlds and Java item data are not migrated. Use a separate Horizon world.

## Installation and validation

Install the `.hmod` through Horizon's native-mod installer. Both client and server need the matching mod and pinned Horizon build. Native mods require the launcher's native-code consent; there is no network access, file access, shell execution or unsafe code in this mod's Rust source.

The release must include its exact-SDK build and package-check evidence. Compiler checks and pure unit tests do not establish in-game visual or gameplay correctness. No Minecraft or Java process was launched on the author's laptop for this port.

The original assets and recipes were preserved. The Horizon implementation was developed with AI assistance at the author's request. See `LICENSE.txt` and `NOTICE.md` for ownership, the Horizon publication grant and third-party attribution.
