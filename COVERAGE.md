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

- [ ] Added (engine fires it; called by the editor's groups, phase 5, and the platforms' user group sub-actions, phase 6+)
- [x] Cleared
- [ ] Removed (engine fires it; called by the editor's groups, phase 5, and the platforms' user group sub-actions, phase 6+)
- [ ] Users Added (engine fires it; called by the editor's groups, phase 5, and the platforms' user group sub-actions, phase 6+)
- [ ] Users Removed (engine fires it; called by the editor's groups, phase 5, and the platforms' user group sub-actions, phase 6+)

**Triggers: schedule** (2)

- [ ] Cron (new in Streamer.bot 1.1.0; the installed 1.0.7 has no type code or fields for it: when Streamer.bot is updated)
- [x] Timed Actions

**Triggers: streamerbot-started** (1)

- [x] Streamer.bot Started

**Triggers: test** (1)

- [x] Test

**Triggers** (2)

- [ ] Custom Code Event (fired by the WebSocket's ExecuteCodeTrigger: phase 4)
- [x] Custom


## Phase 4: servers

**WebSocket client/server triggers** (6)

- [ ] client: Websocket Client Closed
- [ ] client: Websocket Client Message
- [ ] client: Websocket Client Open
- [ ] custom-server: WebSocket Custom Server Connection Closed
- [ ] custom-server: WebSocket Custom Server Connection Opened
- [ ] custom-server: WebSocket Custom Server Message

**HTTP requests** (6)

- [ ] ClearCredits
- [ ] ClearFirstWordsCache
- [ ] DoAction
- [ ] GetActions
- [ ] GetCredits
- [ ] TestCredits

**UDP requests** (1)

- [ ] DoAction (UDP)

**WebSocket requests** (22)

- [ ] Subscribe
- [ ] UnSubscribe
- [ ] GetEvents
- [ ] GetActions
- [ ] DoAction
- [ ] GetBroadcaster
- [ ] GetCredits
- [ ] TestCredits
- [ ] ClearCredits
- [ ] GetInfo
- [ ] GetActiveViewers
- [ ] GetCodeTriggers
- [ ] ExecuteCodeTrigger
- [ ] GetCommands
- [ ] TwitchGetEmotes
- [ ] YouTubeGetEmotes
- [ ] GetGlobals
- [ ] GetGlobal
- [ ] TwitchGetUserGlobals
- [ ] TwitchGetUserGlobal
- [ ] SendMessage
- [ ] GetUserPronouns
- [ ] Events: every category/event the real Streamer.bot lists in `GetEvents`, emitted as its trigger fires


## Phase 6-8: Twitch

**Triggers** (131)

- [ ] ads: Ad Mid-Roll
- [ ] ads: Ad Run
- [ ] ads: Upcoming Ad
- [ ] channel-goal: Goal Begin
- [ ] channel-goal: Goal End
- [ ] channel-goal: Goal Progress
- [ ] channel-power-ups: Custom Power Up Redemption
- [ ] channel-reward: Automatic Reward Redemption
- [ ] channel-reward: Reward Redemption Updated
- [ ] channel-reward: Reward Redemption
- [ ] channel: Follow
- [ ] channel: Stream Offline
- [ ] channel: Stream Online
- [ ] channel: Viewer Count Update
- [ ] charity: Charity Completed
- [ ] charity: Charity Donation
- [ ] charity: Charity Progress
- [ ] charity: Charity Started
- [ ] chat: Bits Badge Tier
- [ ] chat: Bot Whispers
- [ ] chat: Cheer
- [ ] chat: First Words
- [ ] chat: Chat Message
- [ ] chat/mode: Emote Mode Off
- [ ] chat/mode: Emote Mode On
- [ ] chat/mode: Follower Mode Changed
- [ ] chat/mode: Follower Mode Off
- [ ] chat/mode: Follower Mode On
- [ ] chat/mode: Slow Mode Changed
- [ ] chat/mode: Slow Mode Off
- [ ] chat/mode: Slow Mode On
- [ ] chat/mode: Subscriber Mode Off
- [ ] chat/mode: Subscriber Mode On
- [ ] chat/mode: Unique Mode Off
- [ ] chat/mode: Unique Mode On
- [ ] chat: Modiversary
- [ ] chat: Watch Streak
- [ ] chat: Whispers
- [ ] community-goal: Community Goal Contribution
- [ ] community-goal: Community Goal Ended
- [ ] connections: Bot Chat Connected
- [ ] connections: Bot Chat Disconnected
- [ ] connections: Broadcaster Authenticated
- [ ] connections: Broadcaster Chat Connected
- [ ] connections: Broadcaster Chat Disconnected
- [ ] connections: Broadcaster EventSub Connected
- [ ] connections: Broadcaster EventSub Disconnected
- [ ] connections: Broadcaster PubSub Connected
- [ ] connections: Broadcaster PubSub Disconnected
- [ ] emotes: BetterTTV Emote Added
- [ ] emotes: BetterTTV Emote Removed
- [ ] emotes: SevenTV Emote Added
- [ ] emotes: SevenTV Emote Removed
- [ ] general: Announcement
- [ ] general: Present Viewers
- [ ] general: Shoutout Received
- [ ] general: Stream Update
- [ ] guest-star: Guest Star Guest Update
- [ ] guest-star: Guest Star Session Begin
- [ ] guest-star: Guest Star Session End
- [ ] guest-star: Guest Star Settings Update
- [ ] hype-train: Hype Train End
- [ ] hype-train: Hype Train Level Up
- [ ] hype-train: Hype Train Starts
- [ ] hype-train: Hype Train Update
- [ ] moderation: AutoMod Message Held
- [ ] moderation: AutoMod Message Updated
- [ ] moderation: Blocked Terms Added
- [ ] moderation: Blocked Terms Deleted
- [ ] moderation: Chat Cleared
- [ ] moderation: Chat Message Deleted
- [ ] moderation: Moderator Added
- [ ] moderation: Moderator Removed
- [ ] moderation: Permitted Terms Added
- [ ] moderation: Permitted Terms Deleted
- [ ] moderation: Shield Mode Begin
- [ ] moderation: Shield Mode End
- [ ] moderation: Shoutout Created
- [ ] moderation: Suspicious User Update
- [ ] moderation: Unban Request Approved
- [ ] moderation: Unban Request Created
- [ ] moderation: Unban Request Denied
- [ ] moderation: User Banned
- [ ] moderation: User Timed Out
- [ ] moderation: User Unbanned
- [ ] moderation: User Untimed Out
- [ ] moderation: VIP Added
- [ ] moderation: VIP Removed
- [ ] moderation: Warned User
- [ ] moderation: Warning Acknowledged
- [ ] polls: Poll Archived
- [ ] polls: Poll Completed
- [ ] polls: Poll Created
- [ ] polls: Poll Terminated
- [ ] polls: Poll Updated
- [ ] predictions: Prediction Canceled
- [ ] predictions: Prediction Completed
- [ ] predictions: Prediction Created
- [ ] predictions: Prediction Locked
- [ ] predictions: Prediction Updated
- [ ] pyramid: Pyramid Broken
- [ ] pyramid: Pyramid Success
- [ ] raid: Raid Cancelled
- [ ] raid: Raid
- [ ] raid: Raid Send
- [ ] raid: Raid Start
- [ ] shared-chat: Shared Announcement
- [ ] shared-chat: Shared Community Sub Gift
- [ ] shared-chat: Shared Gift Paid Upgrade
- [ ] shared-chat: Shared Chat Message Deleted
- [ ] shared-chat: Shared Modiversary
- [ ] shared-chat: Shared Pay It Forward
- [ ] shared-chat: Shared Prime Paid Upgrade
- [ ] shared-chat: Shared Resub
- [ ] shared-chat: Twitch Shared Chat Session Begin
- [ ] shared-chat: Twitch Shared Chat Session End
- [ ] shared-chat: Twitch Shared Chat Session Update
- [ ] shared-chat: Shared Sub Gift
- [ ] shared-chat: Shared Sub
- [ ] shared-chat: Shared Chat User Banned
- [ ] shared-chat: Shared Chat User Timed Out
- [ ] shared-chat: Shared Chat User Unbanned
- [ ] shared-chat: Shared Chat User Untimed Out
- [ ] subscriptions: Gift Bomb
- [ ] subscriptions: Gift Paid Upgrade
- [ ] subscriptions: Gift Subscription
- [ ] subscriptions: Pay It Forward
- [ ] subscriptions: Prime Paid Upgrade
- [ ] subscriptions: Resubscription
- [ ] subscriptions: Sub Counter Rollover
- [ ] subscriptions: Subscription

**Sub-actions** (95)

- [ ] ads: Get Ad Schedule
- [ ] ads: Get Pre-roll Free Time
- [ ] ads: Run Commercial
- [ ] ads: Snooze Ad
- [ ] channel: Add Viewer Count
- [ ] channel: Create Clip
- [ ] channel: Create Stream Marker
- [ ] channel: Set Channel Branded Content
- [ ] channel: Set Channel Game
- [ ] channel: Set Channel Title
- [ ] charity: Get Latest Charity Campaign
- [ ] chat: Clear Chat Messages
- [ ] chat: Get Pinned Chat Message
- [ ] chat: Pin Chat Message
- [ ] chat: Reply to Message
- [ ] chat: Send Announcement to Channel
- [ ] chat: Send Message
- [ ] chat: Unpin Chat Message
- [ ] chat: Update Pinned Chat Message Duration
- [ ] followers: Get Follow Age Info for Target
- [ ] followers: Get Follower Count
- [ ] followers: Get Latest Follower
- [ ] guest-star: Assign Guest Star Slot
- [ ] guest-star: Create Guest Star Session
- [ ] guest-star: Delete Guest Star Invite
- [ ] guest-star: Delete Guest Star Slot
- [ ] guest-star: End Guest Star Session
- [ ] guest-star: Get Channel Guest Star Settings
- [ ] guest-star: Get Guest Star Invites
- [ ] guest-star: Get Guest Star Session
- [ ] guest-star: Send Guest Star Invite
- [ ] guest-star: Update Channel Guest Star Settings
- [ ] guest-star: Update Guest Star Slot Settings
- [ ] guest-star: Update Guest Star Slot
- [ ] moderation: Add Moderator
- [ ] moderation: Add Vip
- [ ] moderation: Ban User
- [ ] moderation/chat-modes: Emote Only
- [ ] moderation/chat-modes: Follow Mode
- [ ] moderation/chat-modes: Slow Mode
- [ ] moderation/chat-modes: Subscriber Only
- [ ] moderation: Remove Moderator
- [ ] moderation: Remove Vip
- [ ] moderation: Send Shoutout
- [ ] moderation/shield: Get Shield Mode Status
- [ ] moderation/shield: Update Shield Mode Status
- [ ] moderation: Timeout User
- [ ] moderation: Unban User
- [ ] moderation: UnTimeout User
- [ ] moderation: Warn User
- [ ] polls: Archive Active Poll
- [ ] polls: Create Poll
- [ ] polls: Get Active Poll
- [ ] polls: Terminate Active Poll
- [ ] predictions: Cancel Active Prediction
- [ ] predictions: Create Prediction
- [ ] predictions: Get Active Prediction
- [ ] predictions: Lock Active Prediction
- [ ] predictions: Resolve Last Prediction by Title
- [ ] predictions: Resolve Last Prediction
- [ ] raid: Cancel Raid
- [ ] raid: Start Raid
- [ ] rewards: Configure Rewards
- [ ] rewards: Get Reward Info
- [ ] rewards: Reset Reward Counter
- [ ] rewards: Reset Reward User Counters
- [ ] rewards: Set Background Color
- [ ] rewards: Set Cost
- [ ] rewards: Set Enabled State
- [ ] rewards: Set Global Cooldown
- [ ] rewards: Set Group Enabled State
- [ ] rewards: Set Group Paused State
- [ ] rewards: Set Max Per Stream
- [ ] rewards: Set Max per User Per Stream
- [ ] rewards: Set Paused State
- [ ] rewards: Set Prompt
- [ ] rewards: Set Title
- [ ] rewards: Update Redemption Status
- [ ] rewards: Update
- [ ] sub-counter: Get Twitch Sub Counter Count
- [ ] sub-counter: Set Twitch Sub Counter Count
- [ ] subscribers: Get Latest Subscriber
- [ ] subscribers: Get Subscriber Count
- [ ] tags: Add Channel Tag
- [ ] tags: Clear Channel Tags
- [ ] tags: Remove Channel Tag
- [ ] tags: Set Channel Tags
- [ ] user-groups: Add User To Group
- [ ] user-groups: Remove User From Group
- [ ] user-groups: User in Group
- [ ] user: Add Broadcaster Information
- [ ] user: Add Present User
- [ ] user: Add Random Users
- [ ] user: Get Team Info For Target
- [ ] user: Get User Info for Target


## Phase 9: OBS Studio

**Triggers** (9)

- [ ] OBS Connected
- [ ] OBS Disconnected
- [ ] OBS Event
- [ ] OBS Recording Started
- [ ] OBS Recording Stopped
- [ ] OBS Scene Changed
- [ ] OBS Streaming Started
- [ ] OBS Streaming Stopped
- [ ] Vendor Event

**Sub-actions** (29)

- [ ] Create Record Chapter
- [ ] filters: Set State of a Random Filter
- [ ] Get Current Scene
- [ ] Get Status
- [ ] groups: Set Random Group Source Visible
- [ ] OBS Raw
- [ ] Recording
- [ ] replay: Set Replay Buffer State
- [ ] scenes: Get Scene Item Properties
- [ ] scenes: Hide Scene's Sources
- [ ] scenes: Set Random Scene Source Visible
- [ ] scenes: Set Scene Filter State
- [ ] Set Active Scene
- [ ] sources: Flip Source
- [ ] sources: Hide Group's Sources
- [ ] sources: Hide Source's Filters
- [ ] sources: Rotate Source
- [ ] sources: Set Browser Source Url
- [ ] sources: Set Color Source Color
- [ ] sources: Set GDI Text
- [ ] sources: Set Image Source File
- [ ] sources: Set Media Source File
- [ ] sources: Set Media State
- [ ] sources: Set Source Audio Track State
- [ ] sources: Set Source Filter State
- [ ] sources: Set Source Mute State
- [ ] sources: Set Source Visibility State
- [ ] Streaming
- [ ] Take Screenshot


## Phase 10: Kick

**Triggers** (18)

- [ ] channel-reward: Reward Redemption
- [ ] channel: Follow
- [ ] channel: Stream Offline
- [ ] channel: Stream Online
- [ ] channel: Viewer Count Update
- [ ] chat: First Words
- [ ] chat: Chat Message
- [ ] emotes: SevenTV Emote Added
- [ ] emotes: SevenTV Emote Removed
- [ ] general: Channel Update
- [ ] general: Present Viewers
- [ ] kicks: Kicks Gifted
- [ ] moderation: User Banned
- [ ] moderation: User Timed Out
- [ ] subscriptions: Gift Subscription
- [ ] subscriptions: Mass Gift Subscription
- [ ] subscriptions: Gift Resubscription
- [ ] subscriptions: Subscription

**Sub-actions** (14)

- [ ] channel: Set Channel Category
- [ ] channel: Set Channel Title
- [ ] chat: Reply to Message
- [ ] chat: Send Message
- [ ] moderation: Ban User
- [ ] moderation: Timeout User
- [ ] moderation: Unban User
- [ ] moderation: UnTimeout User
- [ ] user-groups: Add User To Group
- [ ] user-groups: Remove User From Group
- [ ] user-groups: User in Group
- [ ] user: Add Broadcaster Information
- [ ] user: Add Random Users
- [ ] user: Get User Info for Target Info


## Phase 11: YouTube

**Triggers** (24)

- [ ] broadcast: Broadcast Added
- [ ] broadcast: Broadcast Ended
- [ ] broadcast: Broadcast Monitoring Ended
- [ ] broadcast: Broadcast Monitoring Started
- [ ] broadcast: Broadcast Removed
- [ ] broadcast: Broadcast Started
- [ ] broadcast: Statistics Updated
- [ ] broadcast: Broadcast Updated
- [ ] chat: First Words
- [ ] chat: Jewels Gifted
- [ ] chat: Message
- [ ] chat: Super Chat
- [ ] chat: Super Sticker
- [ ] general: New Sponsor
- [ ] general: New Subscriber
- [ ] general: Present Viewers
- [ ] general: Sponsor Only Ended
- [ ] general: Sponsor Only Started
- [ ] general: User Banned
- [ ] general: User Timed Out
- [ ] membership: Gift Membership Received
- [ ] membership: Membership Gift
- [ ] membership: Member Milestone
- [ ] polls: Poll Closed

**Sub-actions** (9)

- [ ] moderation: Ban User
- [ ] moderation: Timeout User
- [ ] Send Message to Channel
- [ ] Set Description
- [ ] Set Title And Description
- [ ] Set Title
- [ ] user-groups: Add User to Group
- [ ] user-groups: Remove User from Group
- [ ] user-groups: User in Group


## Phase 12: Streamlabs Desktop

**Triggers** (7)

- [ ] Streamlabs Desktop Connected
- [ ] Streamlabs Desktop Disconnected
- [ ] Streamlabs Desktop Recording Started
- [ ] Streamlabs Desktop Recording Stopped
- [ ] Streamlabs Desktop Scene Changed
- [ ] Streamlabs Desktop Streaming Started
- [ ] Streamlabs Desktop Streaming Stopped

**Sub-actions** (16)

- [ ] filters: Set State of a Random Filter
- [ ] groups: Hide Group's Sources
- [ ] groups: Set Random Group Source Visible
- [ ] Recording
- [ ] scenes: Get Current Scene
- [ ] scenes: Set Active Scene
- [ ] scenes: Set Scene Filter State
- [ ] sources: Flip Source
- [ ] sources: Hide Source's Filters
- [ ] sources: Hide Source's Filters
- [ ] sources: Set Browser Source URL
- [ ] sources: Set GDI Text
- [ ] sources: Set Source Filter State
- [ ] sources: Set Source Mute State
- [ ] sources: Set Source Visibility State
- [ ] Streaming


## Phase 13: donations and shops

**Triggers: ko-fi** (5)

- [ ] Commission
- [ ] Donation
- [ ] Resubscription
- [ ] Shop Order
- [ ] Subscription

**Triggers: streamelements** (4)

- [ ] Connected
- [ ] Disconnected
- [ ] Merch
- [ ] Tip

**Triggers: streamlabs** (5)

- [ ] Charity Donation
- [ ] Connected
- [ ] Disconnected
- [ ] Donation
- [ ] Merchandise

**Triggers: patreon** (5)

- [ ] Follow Created
- [ ] Follow Deleted
- [ ] Pledge Created
- [ ] Pledge Deleted
- [ ] Pledge Updated

**Triggers: fourthwall** (13)

- [ ] Fourthwall Donation
- [ ] Gift Draw Ended
- [ ] Gift Draw Started
- [ ] Gift Purchase
- [ ] Newsletter Subscribed
- [ ] Order Placed
- [ ] Order Updated
- [ ] Product Created
- [ ] Product Updated
- [ ] Subscription Changed
- [ ] Subscription Expired
- [ ] Subscription Purchased
- [ ] Thank You Sent

**Triggers: throne** (3)

- [ ] Contribution Purchased
- [ ] Gift Crowdfunded
- [ ] Gift Purchased

**Triggers: tipeeestream** (1)

- [ ] Donation

**Triggers: treatstream** (1)

- [ ] Treat

**Triggers: donordrive** (3)

- [ ] Donation
- [ ] Incentive
- [ ] Profile Updated

**Triggers: pallygg** (3)

- [ ] Campaign Tip
- [ ] Connected
- [ ] Disconnected

**Triggers: shopify** (2)

- [ ] Order Created
- [ ] Order Paid

**Triggers: streamloots** (3)

- [ ] Card Redeemed
- [ ] Pack Gifted
- [ ] Pack Purchased


## Phase 14: webhooks and remote

**Sub-actions: discord** (1)

- [ ] Basic Webhook

**Sub-actions: ifttt** (2)

- [ ] IFTTT Notification (Basic)
- [ ] IFTTT Notification (Advanced)

**Sub-actions: pronouns** (1)

- [ ] Add Pronouns for User

**Sub-actions** (3)

- [ ] Send Remote Signal
- [ ] Send Remote Trigger
- [ ] Wait For Remote Signal

**Triggers** (3)

- [ ] Remote Instance Connected
- [ ] Remote Instance Disconnected
- [ ] Remote Instance Trigger

