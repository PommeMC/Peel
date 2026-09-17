# Reference traces

Vanilla captures of common scenarios, one file per scenario in
`<version>/<scenario>.trace`. Compare a trace of your own client, mod, proxy
or server against one:

```bash
peel diff mine.trace --against fixtures/26.3/walking.trace
peel view mine.trace --against fixtures/26.3/walking.trace
```

Record yours on the same setup and follow the same steps, or the diff reports
the differences in setup too.

| Version | Scenarios                                                   | Captured   |
| ------- | ----------------------------------------------------------- | ---------- |
| 26.3    | walking, sprint-jumping, combat, building, inventory-use    | 2026-09-17 |
| 26.2    | walking, sprint-jumping, combat, building, inventory-use    | 2026-09-17 |

## Setup

Every capture starts from the same state:

- **The world** is the one vanilla's own automated tests use
  (`GameTestServer`): flat, seed 0, no structures. `server.properties` sets only
  that, offline mode, no spawn protection and no whitelist (26.3 turns it on by
  default); everything else is vanilla's default.
- **The client** is the vanilla client from the Minecraft launcher, started as
  `Player` with `--offlineDeveloperMode`, as vanilla's own `Main` does when it
  runs without the launcher. The name keeps the offline UUID the same between
  captures and your account name out of the traces. `options.txt` only skips
  the first-run screens, so every other option is vanilla's default.

## Capturing

1. Play the version once from the Minecraft launcher, so its files are
   installed.
2. `just fixture-server <path to server.jar>` starts the server on a fresh
   world. On the first run, accept Minecraft's EULA in
   `target/fixture-server/eula.txt` and run it again.
3. In a second terminal, `just fixture-capture <scenario>` starts recording.
4. In a third, `just fixture-client` starts the client, which joins on its own.
5. As soon as the world appears, follow the scenario's steps, then press Esc
   and Disconnect. The trace is written when you disconnect.

Type setup commands into the server's terminal, never into chat, so they stay
out of the trace. Restart the server for each scenario.

Captures played by hand agree on which packets are sent and their fields, but
not on the exact tick: two walking captures started walking hundreds of ticks
apart. Read a diff against these traces for missing, extra and differing
packets rather than their tick numbers.

## Scenarios

### walking

1. Hold W for 5 seconds.
2. Turn 90° right.
3. Hold W for 5 seconds.

### sprint-jumping

1. Hold Ctrl, W and Space for about 10 jumps.

### combat

1. In the server console: `execute at Player run summon husk ^ ^ ^3`. A husk
   doesn't burn in daylight.
2. Left-click the husk until it dies.

### building

1. In the server console: `give Player dirt 16`.
2. Right-click to place 8 dirt in a line.
3. Break 4 of them.

### inventory-use

1. In the server console: `give Player dirt 16`.
2. Press E.
3. Left-click the dirt, then an empty slot, to move it.
4. Right-click the stack, then left-click another empty slot, to split it.
5. Press Q over a stack to drop one dirt.
6. Press E to close the inventory.
