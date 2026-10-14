# CLAUDE.md

Everything an agent needs is in [AGENTS.md](AGENTS.md) (rules, stack, layout,
pipeline, Windows notes, UI checks) and [TODO.md](TODO.md) (what's done and
what's left). Read both before changing anything.

Short version of the rules that matter most:

- When unsure — stop and ask the user. No older HeliLingo version is a reference.
- Design comes from the Figma file `6KVEWhFnxuyuMjFQQDYiOT`; screenshot every
  large UI change in all three themes and check it honestly.
- Never kill the user's running `helilingo.exe`, never touch their clipboard
  or send keys to their desktop while testing.
- Items marked ⏸ in TODO.md wait for the user's approval: don't build them.
