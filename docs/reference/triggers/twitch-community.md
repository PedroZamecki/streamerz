# Twitch / Community (triggers)

Twitch's community events: raids, Hype Trains, ad breaks, charity campaigns and goals, pyramids, emote
changes, and the chat's blocked and permitted terms.

## Raid

- Kind: trigger
- Code: 107
- Type: TwitchRaid
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when another channel raids this one. The raids this channel sends are the Raid Send trigger.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the raiding channel (`userType` is `twitch`).
- `viewers`: the number of viewers the raid brings.
- `profileImageUrl`: the raiding channel's picture.

### When

When the channel's chat gets a raid notice for an incoming raid. A raid that comes in through a shared chat
fires the shared chat version of this trigger instead.

## Hype Train Start

- Kind: trigger
- Code: 108
- Type: TwitchHypeTrainStart
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a Hype Train begins.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `id`, `level`, `total`, `progress`, `goal`, `percent`, `percentDecimal`, `startedAt`: the train's values
  as Twitch sends them.
- `trainType`, `isGoldenKappaTrain`, `isTreasureTrain`: the kind of train.
- `isSharedTrain`, `isInSharedChat`: whether the train runs in a shared chat.

### When

When Twitch sends the channel's Hype Train begin event.

## Hype Train Update

- Kind: trigger
- Code: 109
- Type: TwitchHypeTrainUpdate
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires each time a Hype Train gets new contributions.

### Fields

None.

### Arguments

The same as Hype Train Start (`broadcastUser`, `broadcastUserName`, `broadcastUserId`, `id`, `level`,
`total`, `progress`, `goal`, `percent`, `percentDecimal`, `startedAt`, `trainType`, and the other train values).

### When

When Twitch sends a Hype Train progress event. It fires before Hype Train Level Up when the level goes up.

## Hype Train Level Up

- Kind: trigger
- Code: 110
- Type: TwitchHypeTrainLevelUp
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a Hype Train reaches a higher level.

### Fields

None.

### Arguments

The same as Hype Train Start, and `prevLevel`: the level the train was at before.

### When

When a Hype Train progress event shows a higher level than the one Streamerz saw last, and the train was
already running (its last level was above 0). It fires right after Hype Train Update for the same event.

## Hype Train End

- Kind: trigger
- Code: 111
- Type: TwitchHypeTrainEnd
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a Hype Train ends.

### Fields

None.

### Arguments

The same as Hype Train Start.

### When

When Twitch sends the channel's Hype Train end event.

## Ad Run

- Kind: trigger
- Code: 139
- Type: TwitchAdRun
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when an ad break starts on the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `duration`, `length`, `adLength`: the length of the break, in seconds.
- `adLengthMs`: the same length in milliseconds.
- `isAutomatic`, `adScheduled`: whether Twitch started the break on its own schedule.
- `startedAt`: when the break started.

### When

When Twitch sends the channel's ad break begin event. Upcoming Ad fires before it.

## Charity Donation

- Kind: trigger
- Code: 141
- Type: TwitchCharityDonation
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when someone donates to the channel's charity campaign.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `user`, `userName`, `userId`, `userType`: the donor.
- `amount`: the donation, as Twitch sends it (in the currency's smallest unit).
- `charity.id`, `charity.campaignId`, `charity.name`, `charity.description`, `charity.logo`,
  `charity.website`, `charity.startedAt`, `charity.stoppedAt`: the campaign.
- `charity.amount.value`, `charity.amount.valueMinor`, `charity.amount.currency`: the donation in the
  major unit (`value`), the smallest unit (`valueMinor`) and the currency.
- `charity.currentAmount.*` and `charity.targetAmount.*`: the raised and goal amounts, with the same three
  keys each.

### When

When Twitch sends the charity campaign donate event.

## Charity Started

- Kind: trigger
- Code: 146
- Type: TwitchCharityStarted
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a charity campaign starts on the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `charityName`: the charity's name.
- `charity.*`: the campaign values, as in Charity Donation.

### When

When Twitch sends the charity campaign start event.

## Charity Progress

- Kind: trigger
- Code: 147
- Type: TwitchCharityProgress
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a charity campaign's total changes.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `currentAmount`, `targetAmount`: the raised and goal amounts, as Twitch sends them (in the currency's
  smallest unit).
- `charity.*`: the campaign values, as in Charity Donation.

### When

When Twitch sends the charity campaign progress event.

## Charity Completed

- Kind: trigger
- Code: 142
- Type: TwitchCharityCompleted
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a charity campaign stops.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `charity.*`: the campaign values, as in Charity Donation.

### When

When Twitch sends the charity campaign stop event. Streamerz takes that as the campaign's end.

## Goal Begin

- Kind: trigger
- Code: 148
- Type: TwitchGoalBegin
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a goal starts on the channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `goalId`, `goalType`, `goalDescription`, `currentAmount`, `targetAmount`, `startedAt`, `endedAt`: the goal,
  as Twitch sends them.
- `goal.id`, `goal.type`, `goal.description`, `goal.currentAmount`, `goal.targetAmount`, `goal.startedAt`,
  `goal.endedAt`: the same values under the `goal.` prefix.

### When

When Twitch sends the goal begin event.

## Goal Progress

- Kind: trigger
- Code: 149
- Type: TwitchGoalProgress
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a goal's amount changes.

### Fields

None.

### Arguments

The same as Goal Begin.

### When

When Twitch sends the goal progress event.

## Goal End

- Kind: trigger
- Code: 150
- Type: TwitchGoalEnd
- Category: Twitch/Community/Charity & Goals
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a goal ends.

### Fields

None.

### Arguments

The same as Goal Begin.

### When

When Twitch sends the goal end event. Only the goal end event fires it.

## Raid Start

- Kind: trigger
- Code: 158
- Type: TwitchRaidStart
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a raid that Streamerz started is accepted by Twitch.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `raidUser`, `raidUserName`, `raidUserId`: the channel being raided.
- `raidUserProfileImageURL`, `raidUserProfileImageEscaped`: that channel's picture address, plain and escaped.

### When

When the Start Raid sub-action succeeds.

## Raid Send

- Kind: trigger
- Code: 159
- Type: TwitchRaidSend
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when this channel raids another channel.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `viewers`: the number of viewers sent.
- `raidUser`, `raidUserName`, `raidUserId`, `raidUserProfileImageURL`, `raidUserProfileImageEscaped`: the
  channel that was raided.

### When

When Twitch sends a raid event that starts from this channel. Raids sent to this channel are ignored here
(see Raid).

## Raid Cancelled

- Kind: trigger
- Code: 160
- Type: TwitchRaidCancelled
- Category: Twitch/Community/Hype Train & Raids
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when a raid is cancelled through Streamerz.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `raidUser`, `raidUserName`, `raidUserId`, `raidUserProfileImageURL`, `raidUserProfileImageEscaped`: the
  raided channel, when Streamerz saw the raid start in this session.

### When

When the Cancel Raid sub-action succeeds.

## Pyramid Success

- Kind: trigger
- Code: 162
- Type: TwitchPyramidSuccess
- Category: Twitch/Community/Ads & Milestones
- Needs: a connected Twitch chat

Fires when a pyramid in the chat is completed.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the user who built the pyramid.
- `pyramidWidth`: the widest row of the pyramid.
- `pyramidEmote`: the word the pyramid is made of.
- `totalPyramids`: how many pyramids this user has completed in this session.

### When

When someone posts a single word, then two of it, then three, and so on up to the widest row, then comes back
down to one. The widest row must be at least 3. A message from anyone else in the middle breaks the pyramid
and fires Pyramid Broken instead.

## Pyramid Broken

- Kind: trigger
- Code: 163
- Type: TwitchPyramidBroken
- Category: Twitch/Community/Ads & Milestones
- Needs: a connected Twitch chat

Fires when someone breaks a pyramid that is being built in the chat.

### Fields

None.

### Arguments

- `user`, `userName`, `userId`, `userType`: the user who broke the pyramid.
- `pyramidWidth`: the widest row reached.
- `pyramidEmote`: the word the pyramid was made of.
- `pyramidOwnerId`, `pyramidOwnerUsername`, `pyramidOwnerDisplayName`: the user who was building it.
- `totalPyramidsBroken`: how many pyramids the breaker has broken in this session.

### When

When another user's message arrives while a pyramid has reached a width of at least 2.

## Seven TVEmote Added

- Kind: trigger
- Code: 180
- Type: TwitchSevenTVEmoteAdded
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it (the emote watch runs while Twitch is
  connected)

Fires when an emote is added to the channel's 7TV emote set.

### Fields

None.

### Arguments

- `emote`: the emote's name.
- `imageUrl`: the emote's image.

### When

When 7TV reports an emote added to the channel's emote set. Streamerz listens to 7TV's event socket only while
an action uses this trigger.

## Better TTVEmote Added

- Kind: trigger
- Code: 182
- Type: TwitchBetterTTVEmoteAdded
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it (the emote watch runs while Twitch is
  connected)

Fires when an emote is added to the channel's BetterTTV emotes.

### Fields

None.

### Arguments

- `emote`: the emote's name.
- `imageUrl`: the emote's image.

### When

When BetterTTV reports an emote added to the channel. Streamerz listens to BetterTTV's event socket only while
an action uses this trigger.

## Upcoming Ad

- Kind: trigger
- Code: 186
- Type: TwitchUpcomingAd
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires once when the next ad break is 5 minutes away or less.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `minutes`: the minutes left before the ad.
- `nextAdAt`: when the ad is due, as Twitch sends it.
- `snoozesLeft`: how many snoozes are left.
- `adLength`: the length of the ad, in seconds.

### When

Streamerz checks the schedule once a minute, while Twitch is connected, the stream is live and an action uses
this trigger. It fires once for each scheduled ad, when the time left is 5 minutes or less.

### Notes

- A snooze moves the ad, so it fires again for the new time when that comes within 5 minutes.

## Blocked Terms Added

- Kind: trigger
- Code: 465
- Type: TwitchBlockedTermsAdded
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when someone adds terms to the channel's blocked terms.

### Fields

None.

### Arguments

- `broadcastUser`, `broadcastUserName`, `broadcastUserId`: the channel.
- `moderatorId`: the moderator's id.
- `moderatorUsername`: the moderator's display name.
- `moderatorDisplayName`: the moderator's login.
- `termCount`: how many terms were added.
- `term.[0]`, `term.[1]`...: each term, by its position.
- `fromAutomod`: whether AutoMod added the terms.
- `createdAt`: when the change was made.

### When

When Twitch sends an automod terms update with the add blocked action.

## Permitted Terms Added

- Kind: trigger
- Code: 469
- Type: TwitchPermittedTermsAdded
- Category: Twitch/Community/Ads & Milestones
- Needs: a Twitch login that is the channel's owner or a moderator of it

Fires when someone adds terms to the channel's permitted terms.

### Fields

None.

### Arguments

The same as Blocked Terms Added.

### When

When Twitch sends an automod terms update with the add permitted action.

## Watch Streak

- Kind: trigger
- Code: 472
- Type: TwitchWatchStreak
- Category: Twitch/Community/Ads & Milestones
- Needs: nothing yet (Streamerz doesn't fire it)

Streamerz doesn't fire this yet.

### Fields

None.

### Arguments

None.

### When

Never, for now.

### Notes

- Streamerz doesn't fire this yet.
