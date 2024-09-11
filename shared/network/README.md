# Network

## Bandwidth test

Run `cargo run -p psyche-network --example bandwidth_test` on one PC, then copy the join ticket at the top (might need to shift-click to select it, make sure you get the whole thing even if it's multiline) and do `cargo run -p psyche-network --example bandwidth_test -- join_ticket_here` on another machine. In ~15s they should start swapping data.
