# Donations & Shops / Other Services (triggers)

Donation and shop events from DonorDrive, TipeeeStream, TreatStream and Pally.gg. DonorDrive is checked by Streamerz itself. The rest arrive by webhook on the HTTP server.

## Donor Drive Donation

- Kind: trigger
- Code: 3001
- Type: DonorDriveDonation
- Category: Donations & Shops/Other Services
- Needs: a DonorDrive participant ID on the Donations page, or the HTTP server for webhooks

Starts an action for each new donation to the DonorDrive participant.

### Fields

None.

### Arguments

- `donorIsAnonymous`: true when the donor gave no name.
- `donorName`: the donor's name, or `Anonymous`.
- `donorAvatarUrl`: the donor's avatar.
- `recipientName`: who the donation is for.
- `donorMessage`: the message with the donation.
- `donorAmount`: the amount donated.
- `profileName`, `profileTeamName`, `eventName`, `goal` and `raised`: the participant's profile.
- `isTeam` and `isParticipant`: whether the profile is a team or a participant.
- `donations`: the participant's recent donations.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each new donation Streamerz finds. DonorDrive is checked every 15 seconds.

### Notes

- The first check after start only records the donations that already exist, so they don't fire.
- When several donations come in together, the oldest fires first.

## Donor Drive Profile Updated

- Kind: trigger
- Code: 3002
- Type: DonorDriveProfileUpdated
- Category: Donations & Shops/Other Services
- Needs: a DonorDrive participant ID on the Donations page, or the HTTP server for webhooks

Starts an action when the DonorDrive profile changes.

### Fields

None.

### Arguments

- `name`, `teamName` and `eventName`: the profile's names.
- `goal` and `raised`: the fundraising goal and the amount raised.
- `donations`: the participant's recent donations.
- `isTeam` and `isParticipant`: whether the profile is a team or a participant.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

When the profile changes on DonorDrive. It's checked every 15 seconds.

### Notes

- The first check after start doesn't fire this trigger.

## Donor Drive Incentive

- Kind: trigger
- Code: 3003
- Type: DonorDriveIncentive
- Category: Donations & Shops/Other Services
- Needs: a DonorDrive participant ID on the Donations page, or the HTTP server for webhooks

Starts an action for each donation that claims an incentive.

### Fields

None.

### Arguments

- The same donor and profile arguments as Donor Drive Donation, except `donations`.
- `isIncentive`: true.
- `incentive.id`, `incentive.isActive`, `incentive.amount`, `incentive.description` and `incentive.image`: the incentive.
- `incentive.quantity` and `incentive.quantityClaimed`: how many the incentive has, and how many are claimed.
- `incentive.links.donate`: the link to donate to it.
- `incentive.startDate` and `incentive.endDate`: when it runs.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each new donation with an incentive, right after its Donor Drive Donation trigger.

## Tipeee Stream Donation

- Kind: trigger
- Code: 10001
- Type: TipeeeStreamDonation
- Category: Donations & Shops/Other Services
- Needs: the HTTP server for webhooks

Starts an action for each donation TipeeeStream sends.

### Fields

None.

### Arguments

- `user`: the donor's name, or `Anonymous`.
- `amount`: the amount donated.
- `currency`: the currency, `EUR` when none is sent.
- `message`: the message with the donation.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each payload posted to `/api/tipeeestream` on the HTTP server.

### Notes

- TipeeeStream sends events by webhook only. Streamerz doesn't open a connection to it.

## Treat Stream Treat

- Kind: trigger
- Code: 11001
- Type: TreatStreamTreat
- Category: Donations & Shops/Other Services
- Needs: the HTTP server for webhooks

Starts an action for each treat TreatStream sends.

### Fields

None.

### Arguments

- `treat`: the name of the treat.
- `user` and `sender`: the sender's name, or `Anonymous`.
- `message`: the message with the treat.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each payload posted to `/api/treatstream` on the HTTP server.

### Notes

- TreatStream sends events by webhook only. Streamerz doesn't open a connection to it.

## Treat Stream Authenticated

- Kind: trigger
- Code: 11002
- Type: TreatStreamAuthenticated
- Category: Donations & Shops/Other Services
- Needs: nothing

Would start an action when TreatStream signs in.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Treat Stream Connected

- Kind: trigger
- Code: 11003
- Type: TreatStreamConnected
- Category: Donations & Shops/Other Services
- Needs: nothing

Would start an action when TreatStream connects.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Treat Stream Disconnected

- Kind: trigger
- Code: 11004
- Type: TreatStreamDisconnected
- Category: Donations & Shops/Other Services
- Needs: nothing

Would start an action when TreatStream disconnects.

### Fields

None.

### Arguments

None yet.

### When

Not yet. Streamerz doesn't fire this trigger.

### Notes

- Streamerz doesn't fire this yet.

## Pallygg Connected

- Kind: trigger
- Code: 28001
- Type: PallyggConnected
- Category: Donations & Shops/Other Services
- Needs: the HTTP server for webhooks

Starts an action when Pally.gg sends a connected event.

### Fields

None.

### Arguments

- Every field of the payload, as it was sent.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each payload posted to `/api/pallygg` (or `/api/pally`) on the HTTP server with the type `connected`.

### Notes

- Pally.gg sends events by webhook only. Streamerz doesn't open a connection to it.
- The type must be exactly `connected`. Other values count as campaign tips.

## Pallygg Disconnected

- Kind: trigger
- Code: 28002
- Type: PallyggDisconnected
- Category: Donations & Shops/Other Services
- Needs: the HTTP server for webhooks

Starts an action when Pally.gg sends a disconnected event.

### Fields

None.

### Arguments

- Every field of the payload, as it was sent.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each payload posted to `/api/pallygg` (or `/api/pally`) on the HTTP server with the type `disconnected`.

### Notes

- The type must be exactly `disconnected`. Other values count as campaign tips.

## Pallygg Campaign Tip

- Kind: trigger
- Code: 28003
- Type: PallyggCampaignTip
- Category: Donations & Shops/Other Services
- Needs: the HTTP server for webhooks

Starts an action for each campaign tip from Pally.gg.

### Fields

None.

### Arguments

- Every field of the payload, as it was sent.
- Plus `triggerId`, `triggerName`, `triggerCategory` and `eventSource`.

### When

Each payload posted to `/api/pallygg` (or `/api/pally`) on the HTTP server whose type isn't `connected` or `disconnected`. A payload with no type counts as a campaign tip.
