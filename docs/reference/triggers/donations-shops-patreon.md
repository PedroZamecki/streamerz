# Donations & Shops / Patreon (triggers)

Patreon membership events that Patreon sends to Streamerz's HTTP server: follows, and pledges created, changed or deleted.

## Patreon Follow Created

- Kind: trigger
- Code: 8001
- Type: PatreonFollowCreated
- Category: Donations & Shops/Patreon
- Needs: the HTTP server running (off by default, port 7474), and Patreon's webhook sent to it

Fires when someone follows the Patreon page.

### Fields

None.

### Arguments

- `user`: the member's full name, or `name`, or `Anonymous` when neither is sent.
- `fullName`: the same as `user`.
- `amountCents`: the pledge amount in cents. `0` when missing.
- `tier`: the tier name. Empty when missing.

### When

When Patreon sends `members:follow:create`, or an event name that none of the other Patreon triggers match.

### Notes

- Streamerz reads the event name from an `event` key, or else from `type`. A payload with neither counts as a pledge created.
- Because any unknown event name lands here, check the event name if you rely on this trigger.
- Streamerz doesn't check a Patreon webhook secret yet. The webhook takes any POST to the route.

## Patreon Follow Deleted

- Kind: trigger
- Code: 8002
- Type: PatreonFollowDeleted
- Category: Donations & Shops/Patreon
- Needs: the HTTP server running (off by default, port 7474), and Patreon's webhook sent to it

Fires when someone stops following the Patreon page.

### Fields

None.

### Arguments

Same as Patreon Follow Created.

### When

When Patreon sends `members:follow:delete` or `FollowDeleted`.

### Notes

- Streamerz doesn't check a Patreon webhook secret yet. The webhook takes any POST to the route.

## Patreon Pledge Created

- Kind: trigger
- Code: 8003
- Type: PatreonPledgeCreated
- Category: Donations & Shops/Patreon
- Needs: the HTTP server running (off by default, port 7474), and Patreon's webhook sent to it

Fires when a new pledge is made.

### Fields

None.

### Arguments

Same as Patreon Follow Created.

### When

When Patreon sends `pledges:create`, `members:pledge:create` or `PledgeCreated`. Also when the payload has no event name at all.

### Notes

- Streamerz doesn't check a Patreon webhook secret yet. The webhook takes any POST to the route.

## Patreon Pledge Updated

- Kind: trigger
- Code: 8004
- Type: PatreonPledgeUpdated
- Category: Donations & Shops/Patreon
- Needs: the HTTP server running (off by default, port 7474), and Patreon's webhook sent to it

Fires when a pledge changes, such as a new amount or a new tier.

### Fields

None.

### Arguments

Same as Patreon Follow Created.

### When

When Patreon sends `pledges:update`, `members:pledge:update` or `PledgeUpdated`.

### Notes

- Streamerz doesn't check a Patreon webhook secret yet. The webhook takes any POST to the route.

## Patreon Pledge Deleted

- Kind: trigger
- Code: 8005
- Type: PatreonPledgeDeleted
- Category: Donations & Shops/Patreon
- Needs: the HTTP server running (off by default, port 7474), and Patreon's webhook sent to it

Fires when a pledge is cancelled.

### Fields

None.

### Arguments

Same as Patreon Follow Created.

### When

When Patreon sends `pledges:delete`, `members:pledge:delete` or `PledgeDeleted`.

### Notes

- Streamerz doesn't check a Patreon webhook secret yet. The webhook takes any POST to the route.
