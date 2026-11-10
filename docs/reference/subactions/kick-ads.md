# Kick / Ads (sub-actions)

Sub-actions for platform commercials and ad breaks on Kick.

## Run Commercial

- Kind: sub-action
- Code: 35050
- Type: KickRunCommercial
- Category: Kick/Ads
- Needs: a Kick broadcaster login with ads:write scope

Triggers an ad break on the live Kick stream.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Duration (seconds) | `duration` | Length of the ad break in seconds (7 to 90 seconds). |

### Arguments

- `commercialSuccess`: true when Kick accepts the commercial request.
- `adBreakId`: the UUID of the triggered ad break.
- `adDuration`: the scheduled duration in seconds.

### Notes

- The channel must be live and enrolled in the Kick ad revenue program.
- Duration is clamped between 7 and 90 seconds.

## Get Ad Status

- Kind: sub-action
- Code: 35051
- Type: KickGetAdStatus
- Category: Kick/Ads
- Needs: a Kick broadcaster login with ads:read scope

Queries current ad break availability, lock status, and cooldowns.

### Fields

None.

### Arguments

- `adStatusSuccess`: true when the status was retrieved.
- `optedIn`: whether the channel is enrolled in the ads program.
- `adsBlocked`: whether ad breaks are currently blocked.
- `autoAdsActive`: whether automated scheduled ads are active.
- `remainingAdBreaks`: number of remaining ad breaks allowed in the current window.
