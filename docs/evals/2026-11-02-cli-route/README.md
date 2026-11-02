# From a public CLI name to its Rust dispatch

On replay-catalog R7 the Coder searched the source file for a sibling command's public name, then proposed changing a line in the inventory. A disposable rebuild confirmed that the extra inventory entry left a real filtered invocation unable to parse its flag. [R7 compiled control](../2026-11-02-observer-summary/README.md)

The scoped search now includes a small navigation note when the same Rust source contains the corresponding UpperCamelCase identifier. We replayed R7 action 21 with its original file bytes and arguments through a native coupled loop. Both the Coder's next tool message and the Observer's action packet retained the three original literal search rows. Both also received the two locations of `TuiReplay`. On the [pinned source](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/55351155d73675605e478e0518cc09279ae02b80/src/main.rs#L93), line 93 declares the argument-bearing variant and line 618 [dispatches it to its handler](https://github.com/RyoSpiralArchitect/Spiral-Coder/blob/55351155d73675605e478e0518cc09279ae02b80/src/main.rs#L618).

The local test used four scripted Coder requests and one scripted Observer request per executor, all on loopback. It captured the actual outgoing bodies. The extra line identifies a spelling and sample locations. A public invocation still needs its own verification. Two positive regressions failed with the old runtime. The final local suite passed 1,050 Rust unit tests with two existing ignores, four integration tests and 15 TUI replays, plus the Python, Web, npm, repo-map and server checks.

[Before](before.json) · [After](after.json) · [Reproduction script](replay_search.py) · [R8 protocol](protocol.md)
