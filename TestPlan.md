# A-50 test plan

Display names on `/scoreboard objectives add` and `/team add` are text components. A quoted name stores the text inside the quotes. A JSON component stores that component. Omitting the name stores the objective or team name. The value is the same `TextComponent` the A-05 scoreboard save already writes for 26.3: plain text is a string tag, and a styled component is a compound tag.

## Automated

From the repo root, after a debug build:

```bash
cargo test -p pumpkin-core --lib command::commands::scoreboard
cargo test -p pumpkin-core --lib command::commands::team
```

`objectives_add_parses_display_name_as_a_component` and `team_add_parses_display_name_as_a_component` parse with `CommandSource::dummy`. The whole line is consumed, `errors` is empty, and the component's plain text is `Kills` or `Red`. The JSON form is not stored as the raw JSON string. A gold JSON name equals that component.

`objectives_add_without_display_name_parses_the_whole_line` and `team_add_without_display_name_parses_the_whole_line` accept the command with no display-name argument. The executors still use the objective or team name in that branch.

`parsed_display_name_uses_the_shared_scoreboard_save` and `parsed_team_display_name_uses_the_shared_scoreboard_save` write `Scoreboard::to_nbt` at `MAXIMUM_SUPPORTED_WORLD_DATA_VERSION` (5023, the 26.3 data version) and read it back with `Scoreboard::from_nbt`. That is the compound `load_scoreboard` stores in `scoreboard.dat`.

- `"Kills"` and `{"text":"Kills"}` both save `DisplayName` as the string tag `Kills`.
- `{"text":"Kills","color":"gold"}` saves a compound whose `text` is `Kills` and whose `color` is `gold`, and the loaded component matches the parsed one.
- `team add red "Red"` saves the team `DisplayName` as the string tag `Red`.

Also run:

```bash
cargo build --workspace
cargo clippy -p pumpkin-core --all-targets --all-features
cargo fmt --check
```

## In game

Follow `docs/playtest/A-50.md` with a real client. The unit tests do not draw chat or team colors.

## Out of scope

- `objectives list` and `objectives setdisplay` (A-51).
- `team join` (A-52).
- `team modify displayName`, `prefix`, and `suffix`. They stay greedy strings.
- The scoreboard NBT reader and writer. This task only feeds them a real text component.
