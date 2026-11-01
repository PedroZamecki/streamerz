# Twitch / Charity (sub-actions)

Charity campaigns on Twitch.

## Get Latest Charity Campaign

- Kind: sub-action
- Code: 25
- Type: TwitchGetCharityCampaign
- Category: Twitch/Charity
- Needs: a Twitch login as the channel's owner (the broadcaster)

Reads the latest charity campaign of the channel and adds its info to the arguments.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Text | `text` | Kept with the sub-action. |
| Preferred account | `preferredAccount` | Kept with the sub-action. |
| Color | `color` | Kept with the sub-action. |

### Arguments

- `campaignId`: the ID of the campaign.
- `charity.name`, `charity.description`, `charity.logoUrl`, `charity.websiteUrl`: the charity's info.
- `currentAmount`, `targetAmount`: the amount raised and the goal, as plain numbers.

### Notes

- `text`, `preferredAccount` and `color` are not read yet.
