
### Restoring completed edit diagnosis

A successful native read of the latest failed edit target is diagnosis, not
repair or verification. Across a role handoff, that current diagnosis may
restore Fix while the original failed check remains pending. A newer actual
failed command, edit, or read supersedes that permission; an old source read
cannot discharge it. This projection preserves failed-edit history and still
requires the original exact check after a repair. Existing restoration without
an active recovery cause retains its prior behavior.
