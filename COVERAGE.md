# streamerz coverage

Every documented Streamer.bot trigger and sub-action in scope, from docs.streamer.bot (Streamerbot/docs). Each phase in `PLAN.md` ticks its own list here; nothing is dropped silently: an item that can't be done is struck through (`~~name~~`) with the reason.

## Phase 3: engine core

**Sub-actions: action-queues** (3)

- [x] Clear Action Queue
- [x] Set Action Queue Blocking State
- [x] Set Action Queue Pause State

**Sub-actions: actions** (7)

- [x] Get Action Group State
- [x] Get Action State
- [x] Run Action
- [x] Set Action Group State
- [x] Set Action State
- [x] Set Sub-Action State
- [x] Set Sub-Action Weight

**Sub-actions: arguments** (1)

- [x] Set Argument

**Sub-actions: commands** (5)

- [x] Get Command Group State
- [x] Get Command State
- [x] Get Commands
- [x] Set Command Group State
- [x] Set Command State

**Sub-actions: delay** (1)

- [x] Delay

**Sub-actions: globals** (4)

- [x] Clear Non-Persisted Globals
- [x] Clear Non-Persisted User Globals
- [x] Get Global Variable
- [x] Set Global Variable

**Sub-actions: logic** (8)

- [x] Break
- [x] Get Input
- [x] Get Random Number
- [x] If / Else
- [x] Loop Break
- [x] Loop Continue
- [x] Switch
- [x] While

**Sub-actions: misc** (2)

- [x] Comment
- [x] Log

**Sub-actions: network** (2)

- [x] Fetch URL
- [x] UDP Broadcast

**Sub-actions: signals** (2)

- [x] Send Local Signal
- [x] Wait for Local Signal

**Sub-actions: timers** (1)

- [x] Set Timer State

**Sub-actions: triggers** (2)

- [x] Trigger Custom Event
- [x] Set Trigger State

**Sub-actions: users** (1)

- [x] Clear Users From Group

**Sub-actions: csharp** (2)

- [x] ~~Execute C# Code~~ (no C#, by design: kept on import, skipped at run with a log line; Run a Program replaces it)
- [x] ~~Execute C# Method~~ (no C#, by design: kept on import, skipped at run with a log line; Run a Program replaces it)

**Run a Program (replaces C#)** (1)

- [x] Run a Program

**Triggers: commands** (2)

- [x] Command Cooldown
- [x] Command Triggered

**Triggers: global-variables** (4)

- [x] Global Variable Created
- [x] Global Variable Deleted
- [x] Global Variable Updated
- [x] User Global Variable Updated

**Triggers: groups** (5)

- [x] Added (the editor's Groups tab; the platforms' user group sub-actions call it too, phase 6+)
- [x] Cleared
- [x] Removed (the editor's Groups tab; the platforms' user group sub-actions call it too, phase 6+)
- [ ] Users Added (engine fires it; called by the platforms' user group sub-actions, phase 6+)
- [ ] Users Removed (engine fires it; called by the platforms' user group sub-actions, phase 6+)

**Triggers: schedule** (2)

- [ ] Cron (new in Streamer.bot 1.1.0; the installed 1.0.7 has no type code or fields for it: when Streamer.bot is updated)
- [x] Timed Actions

**Triggers: streamerbot-started** (1)

- [x] Streamer.bot Started

**Triggers: test** (1)

- [x] Test

**Triggers** (2)

- [x] Custom Code Event (fired by the WebSocket's ExecuteCodeTrigger)
- [x] Custom

## Phase 4: servers

**WebSocket client/server triggers** (6)

- [x] client: Websocket Client Closed
- [x] client: Websocket Client Message
- [x] client: Websocket Client Open
- [x] custom-server: WebSocket Custom Server Connection Closed
- [x] custom-server: WebSocket Custom Server Connection Opened
- [x] custom-server: WebSocket Custom Server Message

**HTTP requests** (6)

- [x] ClearCredits
- [x] ClearFirstWordsCache
- [x] DoAction
- [x] GetActions
- [x] GetCredits (empty, like a fresh Streamer.bot: credits are a later phase)
- [x] TestCredits (empty: credits are a later phase)

**UDP requests** (1)

- [x] DoAction (UDP)

**WebSocket requests** (22)

- [x] Subscribe
- [x] UnSubscribe
- [x] GetEvents
- [x] GetActions
- [x] DoAction
- [x] GetBroadcaster (no platform yet: answers as Streamer.bot with them disconnected; filled in as each connects (phase 6+))
- [x] GetCredits (empty, like a fresh Streamer.bot: credits are a later phase)
- [x] TestCredits (empty: credits are a later phase)
- [x] ClearCredits
- [x] GetInfo
- [x] GetActiveViewers (empty until the platforms (phase 6+))
- [x] GetCodeTriggers (always empty: code triggers are registered by C#)
- [x] ExecuteCodeTrigger
- [x] GetCommands
- [x] TwitchGetEmotes ("Twitch is not connected" until phase 6)
- [x] YouTubeGetEmotes ("YouTube is not connected" until phase 11)
- [x] GetGlobals
- [x] GetGlobal
- [x] TwitchGetUserGlobals
- [x] TwitchGetUserGlobal
- [x] SendMessage (auth as Streamer.bot; "<platform> is not connected" until phase 6+)
- [x] GetUserPronouns (Streamer.bot's answer for an unknown user until the pronouns integration (phase 14))
- [ ] Events: every category/event the real Streamer.bot lists in `GetEvents`, emitted as its trigger fires (core ones done: Raw, Misc, Command, Group, WebsocketClient, WebsocketCustomServer; the platforms' own come with their phases)

## Phase 6-8: Twitch

**Triggers** (131)

- [ ] ads: Ad Mid-Roll (no type code in Streamer.bot 1.0.7, and no EventSub event for it)
- [x] ads: Ad Run
- [x] ads: Upcoming Ad
- [x] channel-goal: Goal Begin
- [x] channel-goal: Goal End
- [x] channel-goal: Goal Progress
- [ ] channel-power-ups: Custom Power Up Redemption (not in EventSub; the other power-ups come from `channel.bits.use`)
- [x] channel-reward: Automatic Reward Redemption
- [x] channel-reward: Reward Redemption Updated
- [x] channel-reward: Reward Redemption
- [x] channel: Follow
- [x] channel: Stream Offline
- [x] channel: Stream Online
- [x] channel: Viewer Count Update
- [x] charity: Charity Completed
- [x] charity: Charity Donation
- [x] charity: Charity Progress
- [x] charity: Charity Started
- [x] chat: Bits Badge Tier
- [x] chat: Bot Whispers
- [x] chat: Cheer
- [x] chat: First Words
- [x] chat: Chat Message
- [x] chat/mode: Emote Mode Off
- [x] chat/mode: Emote Mode On
- [x] chat/mode: Follower Mode Changed
- [x] chat/mode: Follower Mode Off
- [x] chat/mode: Follower Mode On
- [x] chat/mode: Slow Mode Changed
- [x] chat/mode: Slow Mode Off
- [x] chat/mode: Slow Mode On
- [x] chat/mode: Subscriber Mode Off
- [x] chat/mode: Subscriber Mode On
- [x] chat/mode: Unique Mode Off
- [x] chat/mode: Unique Mode On
- [ ] chat: Modiversary (an IRC notice only; streamerz reads chat through EventSub)
- [ ] chat: Watch Streak (an IRC notice only; streamerz reads chat through EventSub)
- [x] chat: Whispers
- [ ] community-goal: Community Goal Contribution (PubSub only, which Twitch shut down; no EventSub or Helix)
- [ ] community-goal: Community Goal Ended (PubSub only, which Twitch shut down; no EventSub or Helix)
- [x] connections: Bot Chat Connected
- [x] connections: Bot Chat Disconnected
- [x] connections: Broadcaster Authenticated
- [x] connections: Broadcaster Chat Connected
- [x] connections: Broadcaster Chat Disconnected
- [x] connections: Broadcaster EventSub Connected
- [x] connections: Broadcaster EventSub Disconnected
- [x] ~~connections: Broadcaster PubSub Connected~~ (Twitch shut down PubSub in 2024; replaced by EventSub)
- [x] ~~connections: Broadcaster PubSub Disconnected~~ (Twitch shut down PubSub in 2024; replaced by EventSub)
- [x] emotes: BetterTTV Emote Added
- [x] emotes: BetterTTV Emote Removed
- [x] emotes: SevenTV Emote Added
- [x] emotes: SevenTV Emote Removed
- [x] general: Announcement
- [x] general: Present Viewers
- [x] general: Shoutout Received
- [x] general: Stream Update
- [x] guest-star: Guest Star Guest Update
- [x] guest-star: Guest Star Session Begin
- [x] guest-star: Guest Star Session End
- [x] guest-star: Guest Star Settings Update
- [x] hype-train: Hype Train End
- [x] hype-train: Hype Train Level Up
- [x] hype-train: Hype Train Starts
- [x] hype-train: Hype Train Update
- [x] moderation: AutoMod Message Held
- [x] moderation: AutoMod Message Updated
- [x] moderation: Blocked Terms Added
- [x] moderation: Blocked Terms Deleted
- [x] moderation: Chat Cleared
- [x] moderation: Chat Message Deleted
- [x] moderation: Moderator Added
- [x] moderation: Moderator Removed
- [x] moderation: Permitted Terms Added
- [x] moderation: Permitted Terms Deleted
- [x] moderation: Shield Mode Begin
- [x] moderation: Shield Mode End
- [x] moderation: Shoutout Created
- [x] moderation: Suspicious User Update
- [x] moderation: Unban Request Approved
- [x] moderation: Unban Request Created
- [x] moderation: Unban Request Denied
- [x] moderation: User Banned
- [x] moderation: User Timed Out
- [x] moderation: User Unbanned
- [x] moderation: User Untimed Out
- [x] moderation: VIP Added
- [x] moderation: VIP Removed
- [x] moderation: Warned User
- [x] moderation: Warning Acknowledged
- [x] polls: Poll Archived
- [x] polls: Poll Completed
- [x] polls: Poll Created
- [x] polls: Poll Terminated
- [x] polls: Poll Updated
- [x] predictions: Prediction Canceled
- [x] predictions: Prediction Completed
- [x] predictions: Prediction Created
- [x] predictions: Prediction Locked
- [x] predictions: Prediction Updated
- [x] pyramid: Pyramid Broken
- [x] pyramid: Pyramid Success
- [x] raid: Raid Cancelled
- [x] raid: Raid
- [x] raid: Raid Send
- [x] raid: Raid Start
- [x] shared-chat: Shared Announcement
- [x] shared-chat: Shared Community Sub Gift
- [x] shared-chat: Shared Gift Paid Upgrade
- [x] shared-chat: Shared Chat Message Deleted
- [ ] shared-chat: Shared Modiversary (an IRC notice only)
- [x] shared-chat: Shared Pay It Forward
- [x] shared-chat: Shared Prime Paid Upgrade
- [x] shared-chat: Shared Resub
- [x] shared-chat: Twitch Shared Chat Session Begin
- [x] shared-chat: Twitch Shared Chat Session End
- [x] shared-chat: Twitch Shared Chat Session Update
- [x] shared-chat: Shared Sub Gift
- [x] shared-chat: Shared Sub
- [x] shared-chat: Shared Chat User Banned
- [x] shared-chat: Shared Chat User Timed Out
- [x] shared-chat: Shared Chat User Unbanned
- [x] shared-chat: Shared Chat User Untimed Out
- [x] subscriptions: Gift Bomb
- [x] subscriptions: Gift Paid Upgrade
- [x] subscriptions: Gift Subscription
- [x] subscriptions: Pay It Forward
- [x] subscriptions: Prime Paid Upgrade
- [x] subscriptions: Resubscription
- [x] subscriptions: Sub Counter Rollover
- [x] subscriptions: Subscription

**Sub-actions** (95)

- [x] ads: Get Ad Schedule
- [x] ads: Get Pre-roll Free Time
- [x] ads: Run Commercial
- [x] ads: Snooze Ad
- [x] channel: Add Viewer Count
- [x] channel: Create Clip
- [x] channel: Create Stream Marker
- [x] channel: Set Channel Branded Content
- [x] channel: Set Channel Game
- [x] channel: Set Channel Title
- [x] charity: Get Latest Charity Campaign
- [x] chat: Clear Chat Messages
- [x] chat: Get Pinned Chat Message
- [x] chat: Pin Chat Message
- [x] chat: Reply to Message
- [x] chat: Send Announcement to Channel
- [x] chat: Send Message
- [x] chat: Unpin Chat Message
- [x] chat: Update Pinned Chat Message Duration
- [x] followers: Get Follow Age Info for Target
- [x] followers: Get Follower Count
- [x] followers: Get Latest Follower
- [x] guest-star: Assign Guest Star Slot
- [x] guest-star: Create Guest Star Session
- [x] guest-star: Delete Guest Star Invite
- [x] guest-star: Delete Guest Star Slot
- [x] guest-star: End Guest Star Session
- [x] guest-star: Get Channel Guest Star Settings
- [x] guest-star: Get Guest Star Invites
- [x] guest-star: Get Guest Star Session
- [x] guest-star: Send Guest Star Invite
- [x] guest-star: Update Channel Guest Star Settings
- [x] guest-star: Update Guest Star Slot Settings
- [x] guest-star: Update Guest Star Slot
- [x] moderation: Add Moderator
- [x] moderation: Add Vip
- [x] moderation: Ban User
- [x] moderation/chat-modes: Emote Only
- [x] moderation/chat-modes: Follow Mode
- [x] moderation/chat-modes: Slow Mode
- [x] moderation/chat-modes: Subscriber Only
- [x] moderation: Remove Moderator
- [x] moderation: Remove Vip
- [x] moderation: Send Shoutout
- [x] moderation/shield: Get Shield Mode Status
- [x] moderation/shield: Update Shield Mode Status
- [x] moderation: Timeout User
- [x] moderation: Unban User
- [x] moderation: UnTimeout User
- [x] moderation: Warn User
- [x] polls: Archive Active Poll
- [x] polls: Create Poll
- [x] polls: Get Active Poll
- [x] polls: Terminate Active Poll
- [x] predictions: Cancel Active Prediction
- [x] predictions: Create Prediction
- [x] predictions: Get Active Prediction
- [x] predictions: Lock Active Prediction
- [x] predictions: Resolve Last Prediction by Title
- [x] predictions: Resolve Last Prediction
- [x] raid: Cancel Raid
- [x] raid: Start Raid
- [x] rewards: Configure Rewards
- [x] rewards: Get Reward Info
- [x] rewards: Reset Reward Counter
- [x] rewards: Reset Reward User Counters
- [x] rewards: Set Background Color
- [x] rewards: Set Cost
- [x] rewards: Set Enabled State
- [x] rewards: Set Global Cooldown
- [x] rewards: Set Group Enabled State
- [x] rewards: Set Group Paused State
- [x] rewards: Set Max Per Stream
- [x] rewards: Set Max per User Per Stream
- [x] rewards: Set Paused State
- [x] rewards: Set Prompt
- [x] rewards: Set Title
- [x] rewards: Update Redemption Status
- [x] rewards: Update
- [x] sub-counter: Get Twitch Sub Counter Count
- [x] sub-counter: Set Twitch Sub Counter Count
- [x] subscribers: Get Latest Subscriber
- [x] subscribers: Get Subscriber Count
- [x] tags: Add Channel Tag
- [x] tags: Clear Channel Tags
- [x] tags: Remove Channel Tag
- [x] tags: Set Channel Tags
- [x] user-groups: Add User To Group
- [x] user-groups: Remove User From Group
- [x] user-groups: User in Group
- [x] user: Add Broadcaster Information
- [x] user: Add Present User
- [x] user: Add Random Users
- [x] user: Get Team Info For Target
- [x] user: Get User Info for Target

## Phase 9: OBS Studio

**Triggers** (9)

- [x] OBS Connected
- [x] OBS Disconnected
- [x] OBS Event
- [x] OBS Recording Started
- [x] OBS Recording Stopped
- [x] OBS Scene Changed
- [x] OBS Streaming Started
- [x] OBS Streaming Stopped
- [x] Vendor Event

**Sub-actions** (29)

- [x] Create Record Chapter
- [x] filters: Set State of a Random Filter
- [x] Get Current Scene
- [x] Get Status
- [x] groups: Set Random Group Source Visible
- [x] OBS Raw
- [x] Recording
- [x] replay: Set Replay Buffer State
- [x] scenes: Get Scene Item Properties
- [x] scenes: Hide Scene's Sources
- [x] scenes: Set Random Scene Source Visible
- [x] scenes: Set Scene Filter State
- [x] Set Active Scene
- [x] sources: Flip Source
- [x] sources: Hide Group's Sources
- [x] sources: Hide Source's Filters
- [x] sources: Rotate Source
- [x] sources: Set Browser Source Url
- [x] sources: Set Color Source Color
- [x] sources: Set GDI Text
- [x] sources: Set Image Source File
- [x] sources: Set Media Source File
- [x] sources: Set Media State
- [x] sources: Set Source Audio Track State
- [x] sources: Set Source Filter State
- [x] sources: Set Source Mute State
- [x] sources: Set Source Visibility State
- [x] Streaming
- [x] Take Screenshot

## Phase 10: Kick

**Triggers** (18)

- [x] channel-reward: Reward Redemption
- [x] channel: Follow
- [x] channel: Stream Offline
- [x] channel: Stream Online
- [x] channel: Viewer Count Update
- [x] chat: First Words
- [x] chat: Chat Message
- [x] emotes: SevenTV Emote Added
- [x] emotes: SevenTV Emote Removed
- [x] general: Channel Update
- [x] general: Present Viewers
- [x] kicks: Kicks Gifted
- [x] moderation: User Banned
- [x] moderation: User Timed Out
- [x] subscriptions: Gift Subscription
- [x] subscriptions: Mass Gift Subscription
- [x] subscriptions: Gift Resubscription
- [x] subscriptions: Subscription

**Sub-actions** (14)

- [x] channel: Set Channel Category
- [x] channel: Set Channel Title
- [x] chat: Reply to Message
- [x] chat: Send Message
- [x] moderation: Ban User
- [x] moderation: Timeout User
- [x] moderation: Unban User
- [x] moderation: UnTimeout User
- [x] user-groups: Add User To Group
- [x] user-groups: Remove User From Group
- [x] user-groups: User in Group
- [x] user: Add Broadcaster Information
- [x] user: Add Random Users
- [x] user: Get User Info for Target Info

## Phase 11: YouTube

**Triggers** (24)

- [ ] broadcast: Broadcast Added (Data API scheduled broadcasts: not watched)
- [x] broadcast: Broadcast Ended
- [x] broadcast: Broadcast Monitoring Ended
- [x] broadcast: Broadcast Monitoring Started
- [ ] broadcast: Broadcast Removed (Data API scheduled broadcasts: not watched)
- [x] broadcast: Broadcast Started
- [x] broadcast: Statistics Updated
- [ ] broadcast: Broadcast Updated (Data API scheduled broadcasts: not watched)
- [x] chat: First Words
- [x] chat: Jewels Gifted
- [x] chat: Message
- [x] chat: Super Chat
- [x] chat: Super Sticker
- [x] general: New Sponsor
- [x] general: New Subscriber
- [x] general: Present Viewers
- [x] general: Sponsor Only Ended
- [x] general: Sponsor Only Started
- [x] general: User Banned
- [ ] general: User Timed Out (InnerTube shows a removal, not a ban or timeout: User Banned fires with banType unknown)
- [x] membership: Gift Membership Received
- [x] membership: Membership Gift
- [x] membership: Member Milestone
- [x] polls: Poll Closed

**Sub-actions** (9)

- [x] moderation: Ban User
- [x] moderation: Timeout User
- [x] Send Message to Channel
- [x] Set Description
- [x] Set Title And Description
- [x] Set Title
- [x] user-groups: Add User to Group
- [x] user-groups: Remove User from Group
- [x] user-groups: User in Group

## Phase 12: Streamlabs Desktop

**Triggers** (7)

- [x] Streamlabs Desktop Connected
- [x] Streamlabs Desktop Disconnected
- [x] Streamlabs Desktop Recording Started
- [x] Streamlabs Desktop Recording Stopped
- [x] Streamlabs Desktop Scene Changed
- [x] Streamlabs Desktop Streaming Started
- [x] Streamlabs Desktop Streaming Stopped

**Sub-actions** (16)

- [x] filters: Set State of a Random Filter
- [x] groups: Hide Group's Sources
- [x] groups: Set Random Group Source Visible
- [x] Recording
- [x] scenes: Get Current Scene
- [x] scenes: Set Active Scene
- [x] scenes: Set Scene Filter State
- [x] sources: Flip Source
- [x] sources: Hide Source's Filters
- [x] sources: Hide Source's Filters
- [x] sources: Set Browser Source URL
- [x] sources: Set GDI Text
- [x] sources: Set Source Filter State
- [x] sources: Set Source Mute State
- [x] sources: Set Source Visibility State
- [x] Streaming

## Phase 13: donations and shops

**Triggers: ko-fi** (5)

- [x] Commission
- [x] Donation
- [x] Resubscription
- [x] Shop Order
- [x] Subscription

**Triggers: streamelements** (4)

- [x] Connected
- [x] Disconnected
- [x] Merch
- [x] Tip

**Triggers: streamlabs** (5)

- [x] Charity Donation
- [x] Connected
- [x] Disconnected
- [x] Donation
- [x] Merchandise

**Triggers: patreon** (5)

- [x] Follow Created
- [x] Follow Deleted
- [x] Pledge Created
- [x] Pledge Deleted
- [x] Pledge Updated

**Triggers: fourthwall** (13)

- [x] Fourthwall Donation
- [x] Gift Draw Ended
- [x] Gift Draw Started
- [x] Gift Purchase
- [x] Newsletter Subscribed
- [x] Order Placed
- [x] Order Updated
- [x] Product Created
- [x] Product Updated
- [x] Subscription Changed
- [x] Subscription Expired
- [x] Subscription Purchased
- [x] Thank You Sent

**Triggers: throne** (3)

- [x] Contribution Purchased
- [x] Gift Crowdfunded
- [x] Gift Purchased

**Triggers: tipeeestream** (1)

- [x] Donation

**Triggers: treatstream** (1)

- [x] Treat

**Triggers: donordrive** (3)

- [x] Donation
- [x] Incentive
- [x] Profile Updated

**Triggers: pallygg** (3)

- [x] Campaign Tip
- [x] Connected
- [x] Disconnected

**Triggers: shopify** (2)

- [x] Order Created
- [x] Order Paid

**Triggers: streamloots** (3)

- [x] Card Redeemed
- [x] Pack Gifted
- [x] Pack Purchased

## Phase 14: webhooks and remote

**Sub-actions: discord** (1)

- [x] Basic Webhook

**Sub-actions: ifttt** (2)

- [x] IFTTT Notification (Basic)
- [x] IFTTT Notification (Advanced)

**Sub-actions: pronouns** (1)

- [x] Add Pronouns for User

**Sub-actions** (3)

- [x] Send Remote Signal
- [x] Send Remote Trigger
- [x] Wait For Remote Signal

**Triggers** (3)

- [x] Remote Instance Connected
- [x] Remote Instance Disconnected
- [x] Remote Instance Trigger

## Phase 15: hotkeys, inputs, MIDI and voice control

**Sub-actions: system / inputs** (1)

- [x] Keyboard Press

**Sub-actions: voice control** (3)

- [x] Set Voice Control Command
- [x] Set Voice Control Command State
- [x] Set Voice Control Input

**Sub-actions: MIDI** (3)

- [x] Note On
- [x] Control Change
- [x] Generic Event

**Triggers: speech to text** (2)

- [x] Speech to Text Dictation
- [x] Speech to Text Command

**Triggers: MIDI** (1)

- [x] Midi Message

**Triggers: inputs** (2)

- [x] Key Press
- [x] Mouse Click

## Phase 16: Decks, Stream Deck, HTML overlay and chat window

**Sub-actions: Stream Deck** (7)

- [x] Set Title
- [x] Set Background
- [x] Set State
- [x] Show Alert
- [x] Show Ok
- [x] Toggle State
- [x] Set Value

**Sub-actions: Streamer.bot Decks** (10)

- [x] Item Set State
- [x] Item Toggle State
- [x] Item Set Title
- [x] Item Set Background
- [x] Item Set Icon
- [x] Item Set Value
- [x] Deck Notify
- [x] Page Set
- [x] Page Next
- [x] Page Previous

**Triggers: Stream Deck** (4)

- [x] Action
- [x] Connected
- [x] Disconnected
- [x] Info

**Triggers: chat window** (1)

- [x] Chat Window Action

## Phase 17: File I/O, file tail, watcher and process triggers

**Sub-actions: File** (7)

- [x] Write to File
- [x] Read Random Line From File
- [x] Read Lines From File
- [x] Read Specific Line From File
- [x] Set File Tail State
- [x] File Exists
- [x] Folder Exists

**Triggers: file watcher** (4)

- [x] File Changed
- [x] File Created
- [x] File Deleted
- [x] File Renamed

**Triggers: file tail** (1)

- [x] File Tail Changed

**Triggers: process** (2)

- [x] Process Started
- [x] Process Stopped
