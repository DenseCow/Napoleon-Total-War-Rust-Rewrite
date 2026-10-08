# Comparing NapoleonRust with the original game

This is a short checklist for you. Our side is scripted: one command each. On the original's side you take the same
screenshot or note the same numbers by hand, then one more command compares the two. It needs about 20 minutes.

Before you start:
- Run the original in a **window at 1280 × 960** (Options → Graphics, windowed). Our harness uses that size, so the
  pictures line up pixel for pixel.
- Use the same graphics level as far as you can (the comparison is about layout and colours, not about effects we
  don't have yet).
- Put your screenshots in one folder, e.g. `C:\Users\<you>\Pictures\ntw_compare\`. Windows `Win + Shift + S` (window
  snip) or `Alt + Print Screen` then paste into Paint and save as PNG both work.

All our commands run in the NapoleonRust folder (`%USERPROFILE%\Documents\NapoleonRust`).

## 1. Main menu (picture)
1. Ours: `cargo run -p napoleon --release -- --screenshot C:\Users\<you>\Pictures\ntw_compare\menu_ours.png`
   (the window opens, waits until the menu has settled, saves the picture and closes).
2. Original: start the game, skip the intro movies, wait on the main menu without moving the mouse over a button, and
   save `menu_original.png`.
3. Compare: `cargo run -p napoleon --release --example image_diff -- menu_ours.png menu_original.png menu_diff.png`
   (with the full paths). It prints numbers and writes a heat map (black = same, red = different).

How to read the numbers:
| number | good | means |
|---|---|---|
| layout correlation | above 0.9 | the light and dark areas are in the same places |
| pixels with a brightness difference > 32 | below 10 % | few places look clearly different |
| mean abs difference | below 15 | colours are close on average |
| PSNR | above 20 dB | overall closeness (a single number to track over time) |
The background movie of the original menu moves, so the menu never matches exactly: look at the heat map for the
buttons and the logo.

## 2. A historical battle's opening view (picture)
1. Ours: `set NAPOLEON_BATTLE_SCREENSHOT=C:\Users\<you>\Pictures\ntw_compare\austerlitz_ours.png` and then
   `cargo run -p napoleon --release -- --battle-key NHB_Austerlitz`. The camera starts where the battle file puts it (the
   same place the original starts), the picture is taken after 8 s, and the window closes.
2. Original: Historical Battles → Austerlitz → start. On the deployment screen, before you touch the mouse or keys,
   save `austerlitz_original.png`.
3. Compare as in step 1.3. Here the terrain, trees, buildings and the unit blocks are what to look at.
Other battles work the same way: `NHB_Arcole`, `NHB_Borodino`, `NHB_Dresden`, `NHB_Friedland`, `NHB_Ligny`, `NHB_Lodi`,
`NHB_Pyramids`, ... (the keys of the `battles` table).

## 3. The same battle over time (numbers)
1. Ours: `set NAPOLEON_BATTLE_TRACE=C:\Users\<you>\Pictures\ntw_compare\austerlitz_trace.csv` and run
   `cargo run -p napoleon --release -- --battle-key NHB_Austerlitz`. Start the battle and let it run; every 10 s of
   battle time the file gets one line per unit: men, morale value and state, fatigue, position. Close the window when
   you have enough.
2. Original: start the same battle, press Start Battle without giving orders, and every 60 s (the battle clock in the
   corner) write down, for 3 or 4 units of each side: the men on the unit card, and the morale state from the tooltip
   (e.g. "Eager", "Wavering", "Routing").
3. Send both to the manager. The AI and the random numbers differ between the two games, so the numbers will not match
   one for one; what we look for is the same order of magnitude: how fast men fall, when units start to waver and rout,
   how long the battle lasts.

## 4. Sound (listening)
Same volume settings in both games (ours reads your own preferences file).
| what | ours | original | listen for |
|---|---|---|---|
| menu click | `cargo run -p napoleon --release`, click Single Player, then Grand Campaign | the same clicks | same sound, same loudness |
| menu music | stay on the main menu | the same | same track and loudness; at music volume 100 both are loud |
| near volley | `cargo run -p napoleon --release -- --battle --skip-deployment --ai off`, move the camera close to the firing lines | any battle, camera close to a firing line | sharp, full sound |
| far volley | the same, camera 300–500 m away | the same | duller (high frequencies cut) and quieter |
| very far volley | camera about 1 km away | the same | nothing: the original does not start shots past their maximum distance |

## 5. What the numbers cannot tell
- Effects we don't have yet (smoke, flags, weather, water, grass) make the pictures differ: the heat map shows where.
- The original's random numbers and AI differ from ours, so battle traces are compared in shape, not line by line.
- Our own runs are deterministic: the same seed and orders always give the same battle (checked with
  `cargo run -p ntw_ai --release --example determinism -- twice battle`).
