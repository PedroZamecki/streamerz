# Core / Commands (triggers)

Chat commands: a command being used, and a command being refused because it is on cooldown.

## Command Triggered

- Kind: trigger
- Code: 401
- Type: CommandTriggered
- Category: Core/Commands
- Needs: nothing of its own (the chat the command is typed in must be connected)

Starts its actions when someone uses a chat command.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command | `commandId` | The command that starts the actions. |

### Arguments

- `command`: the alias the user typed.
- `commandId`, `commandName`: the command.
- `commandSource`: the platform the message came from (`twitch`, `youtube`, `kick` or `tiktok`).
- `commandType`: `message`, or `whisper` for a whisper.
- `msgId`: the id of the chat message.
- `counter`, `userCounter`: how many times the command was used, in all and by this user.
- `rawInput`, `rawInputEscaped`, `rawInputUrlEncoded`: the text after the command.
- `input0`, `input1`...: each word after the command, with `inputEscaped0` and `inputUrlEncoded0` (and so on) for the escaped and URL encoded forms.
- The regex groups, for a command that uses a regular expression.
- The user's own arguments from the chat (`user`, `userName`, `userId`...).

### When

When a chat message matches an enabled command, from a platform the command listens to, and the user may use it. A message from the bot account is skipped unless the command says otherwise. A command that is on cooldown fires Command Cooldown instead.

### Notes

- Cooldowns don't apply to the broadcaster.

## Command Cooldown

- Kind: trigger
- Code: 402
- Type: CommandCooldown
- Category: Core/Commands
- Needs: nothing of its own (the chat the command is typed in must be connected)

Starts its actions when someone uses a command that is still on cooldown, so it doesn't run.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Command | `commandId` | The command whose cooldown starts the actions. |

### Arguments

- `command`, `commandId`, `commandName`: the command.
- `commandSource`, `commandType`, `msgId`: as for Command Triggered.
- `cooldownLeft`: the seconds left of the longer cooldown, rounded up.
- `globalCooldownLeft`: the seconds left of the cooldown for everyone.
- `userCooldownLeft`: the seconds left of the cooldown for this user.
- The user's own arguments from the chat (`user`, `userName`, `userId`...).

### When

When a user uses an enabled command that is still on its global or its per-user cooldown. The command doesn't run then, and Command Triggered doesn't fire for that message.

### Notes

- The broadcaster has no cooldowns, so this never fires for them.
