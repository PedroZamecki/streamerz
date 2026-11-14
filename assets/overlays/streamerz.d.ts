// What Streamerz adds to every overlay page: `window.streamerz`. This file is for your editor (names
// and types as you write) and for whoever reads the page; the page itself does not load it.
// The guide is docs/OVERLAYS.md (Overlays, Guide).

declare namespace Streamerz {
  type Platform = "twitch" | "youtube" | "kick" | "tiktok";

  /** A badge beside a chatter's name: a picture's address, or a drawing to use in its place. */
  interface Badge {
    key: string;
    label: string;
    /** The platform's own picture, when it gives one. */
    url?: string;
    /** `streamerz.asset("badges/" + art + ".svg")` when there is no picture. */
    art?: string;
    /** A Lucide icon name when there is neither. */
    icon?: string;
  }

  /** A piece of a message: text, or an emote with its picture. */
  type Part = { t: string } | { e: string; u: string; big?: boolean };

  /** A chat message, or an event shown in the chat, as the Chat page draws it. */
  interface Line {
    platform: Platform;
    /** The time of day, as the user reads it. */
    time: string;
    user: string;
    userId: string;
    /** The colour of the name, already readable on a dark background. */
    color: string;
    badges: Badge[];
    /** The message in pieces, with the emotes of every provider found. */
    parts: Part[];
    /** The message as plain text. */
    text: string;
    msgId: string;
    /** The picture of who wrote it, when the platform sends it along (else `streamerz.avatar`). */
    avatar?: string;
    reply?: { user: string; text: string };
    broadcaster?: boolean;
    moderator?: boolean;
    vip?: boolean;
    subscriber?: boolean;
    /** A `/me` message. */
    action?: boolean;
    gif?: string;
    picture?: string;
    tag?: string;
    deleted?: boolean;
  }

  type AlertKind =
    | "follow" | "sub" | "resub" | "giftsub" | "giftbomb" | "cheer" | "raid" | "superchat"
    | "supersticker" | "member" | "gift" | "share" | "reward" | "powerup" | "streak"
    | "announcement" | "timeout" | "ban" | "other";

  /** Something that happened in a channel: a follow, a sub, a gift, a raid. */
  interface Alert extends Line {
    /** What happened, in words ("subscribed", "sent 5 Rose"). */
    event: string;
    /** A Lucide icon name: star, gift, swords, user-plus, gem. */
    icon: string;
    /** The event's colour. */
    accent: string;
    cost?: number;
    costIcon?: string;
    /** The picture is to be drawn big. */
    huge?: boolean;
    /** One word to count by, whatever the platform. Only on a live event, not on one from `history`. */
    kind?: AlertKind;
    /** The trigger's name, as in the reference ("TwitchCheer"). Live events only. */
    trigger?: string;
    /** The trigger's own arguments: bits, viewers, coins, repeatCount. Live events only. */
    args?: Record<string, string | number | boolean>;
  }

  interface Song {
    title: string;
    artist: string;
    album: string;
    /** The cover's address, empty when the player gives none. */
    cover: string;
    playing: boolean;
    durationMs: number;
    positionMs: number;
    /** When `positionMs` was true, in ms since 1970: `streamerz.position(song)` counts on from it. */
    at: number;
  }

  interface Countdown {
    running: boolean;
    remainingMs: number;
    /** When it reaches zero, in ms since 1970: count down from this, no ticks are sent. */
    deadlineMs: number;
    maxMs: number;
  }

  interface Viewers {
    twitch: number | null;
    youtube: number | null;
    kick: number | null;
    tiktok: number | null;
    total: number;
  }

  interface Removed {
    platform: Platform;
    /** The message deleted, or empty when all of a user's messages are. */
    msgId: string;
    userId: string;
  }

  interface Action {
    id: string;
    name: string;
    enabled: boolean;
  }

  interface LineOptions {
    /** The platform's logo before the name (on unless false). */
    platform?: boolean;
    /** The badges (on unless false). */
    badges?: boolean;
    /** The time (off unless true). */
    time?: boolean;
    /** The chatter's picture (off unless true). */
    avatar?: boolean;
  }

  /** A setting's value: text, a number, true or false, a list of platforms, or the rows of a list. */
  type Setting = string | number | boolean | string[] | Record<string, string | number | boolean>[];

  interface Api {
    /** The overlay's settings, always the current ones. A `file` setting is an address to load. */
    readonly settings: Record<string, Setting>;
    /** The overlay's name. */
    readonly group: string;
    /** True while the page handles made-up data from Test: a page that keeps a count leaves it out. */
    readonly testing: boolean;
    /** The last value of each topic this page listens to. */
    readonly state: Record<string, unknown>;
    readonly platforms: Record<Platform, { name: string; color: string }>;

    /** `fn(settings)` now and whenever a setting changes in Streamerz, with no reload. */
    onSettings(fn: (settings: Record<string, Setting>) => void): Api;
    /** A topic: its current value first (`old` is true for one from before the page opened), then every change. */
    on(topic: "viewers", fn: (value: Viewers, old: boolean) => void): Api;
    on(topic: "song", fn: (value: Song, old: boolean) => void): Api;
    on(topic: string, fn: (value: any, old: boolean) => void): Api;
    /** Any Streamerz event by `Source.Type` ("Twitch.Follow"), or `Source.*` for all of one source. */
    event(name: string, fn: (data: any, name: string) => void): Api;

    /** Every chat message of every connected platform. Needs `chat.read`. */
    chat(fn: (line: Line) => void, options?: { history?: number; removed?: (removed: Removed) => void }): Api;
    /** Follows, subs, gifts, raids, cheers and rewards on every platform. Needs `alerts.read`. */
    alerts(fn: (alert: Alert) => void, options?: { history?: number }): Api;
    /** The song of the Spotify app on this computer, at every change. */
    song(fn: (song: Song) => void): Api;
    /** Where the song is now, in ms. */
    position(song: Song | null): number;
    /** A countdown, now and at every change. */
    countdown(id: string, fn: (countdown: Countdown) => void): Api;

    /** A chat line or an alert as an element, with the classes `sz-line`, `sz-name`, `sz-text`... */
    line(line: Line | Alert, options?: LineOptions): HTMLDivElement;
    /** Only the message, as nodes, with its emotes as pictures. */
    words(line: Line): DocumentFragment;
    /** The picture of who wrote a line or caused an event ("" when there is none). */
    avatar(line: Line): Promise<string>;

    /** The address of a picture Streamerz has: `brands/twitch.svg`, `icons/heart.svg`. */
    asset(path: string): string;
    /** A platform's or an app's logo. */
    logo(name: string): string;
    /** A Lucide icon, white or in `color` (`#rrggbb`). */
    icon(name: string, color?: string): string;

    /** Queue an action connected to this overlay, by name or id. Needs `actions.run`. */
    run(action: string, args?: Record<string, unknown>): Promise<unknown>;
    /** The actions connected to this overlay. Needs `actions.run`. */
    actions(): Promise<Action[]>;
    /** Send to chat, on every connected platform by default. Needs `chat.send`. */
    say(text: string, options?: { platform?: Platform | "all"; bot?: boolean }): Promise<unknown>;
    /** Keep a value of the overlay across restarts (`%group.name%` in actions). Needs `values.write`. */
    set(name: string, value: unknown): Promise<unknown>;
    get(name: string): Promise<any>;
    /** Set a topic for every page listening with `on()`. */
    publish(topic: string, value: unknown): Promise<unknown>;
    /** A line in Streamerz's Log (OBS has no console to look at). */
    log(text: string): Promise<unknown>;
  }
}

declare const streamerz: Streamerz.Api;

interface Window {
  streamerz: Streamerz.Api;
}
