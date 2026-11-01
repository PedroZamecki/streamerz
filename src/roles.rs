//! Who the logged-in account is in each chat it reads, and what that lets it do. Set when a platform
//! connects (Twitch asks; elsewhere the channel's owner is known) and updated from the badges on our own
//! messages. The chat's hover actions, the subscriptions asked for and the sub-actions that call a
//! platform all go by it, so nothing is tried that the platform would refuse.
//!
//! The rules, the same on Twitch, Kick and YouTube:
//! - Nobody can time out, ban, warn or delete the messages of the channel's owner.
//! - Nobody can time out or ban themselves.
//! - A moderator can't time out, ban or delete the messages of another moderator; only the owner can.
//! - Viewers and VIPs moderate nothing: they can only read, reply and copy.
//! - With no login the chat is read only: no reply, no moderation.

use std::sync::Mutex;

/// Our standing in a channel, lowest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum Role {
    /// Not logged in: reading only.
    #[default]
    Anonymous,
    Viewer,
    Vip,
    Moderator,
    /// Twitch's lead moderator: a moderator who also manages the channel's other moderators' tools.
    LeadModerator,
    Broadcaster,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Anonymous => "not logged in",
            Role::Viewer => "viewer",
            Role::Vip => "VIP",
            Role::Moderator => "moderator",
            Role::LeadModerator => "lead moderator",
            Role::Broadcaster => "broadcaster",
        }
    }

    pub fn moderates(self) -> bool {
        self >= Role::Moderator
    }

    /// The role a chatter's badges and flags show.
    pub fn from_flags(broadcaster: bool, lead: bool, moderator: bool, vip: bool) -> Role {
        if broadcaster {
            Role::Broadcaster
        } else if lead {
            Role::LeadModerator
        } else if moderator {
            Role::Moderator
        } else if vip {
            Role::Vip
        } else {
            Role::Viewer
        }
    }
}

/// Each platform's role and our user id there.
static ROLES: Mutex<Vec<(&'static str, Role, String)>> = Mutex::new(Vec::new());

/// The platforms roles are kept for.
fn key(platform: &str) -> Option<&'static str> {
    ["twitch", "kick", "youtube", "tiktok"]
        .into_iter()
        .find(|p| *p == platform)
}

/// Sets our role on `platform` (and our user id there). True when it changed.
pub fn set(platform: &str, role: Role, me: &str) -> bool {
    let Some(p) = key(platform) else { return false };
    let Ok(mut r) = ROLES.lock() else { return false };
    match r.iter_mut().find(|x| x.0 == p) {
        Some(x) if x.1 == role && x.2 == me => false,
        Some(x) => {
            (x.1, x.2) = (role, me.to_string());
            true
        }
        None => {
            r.push((p, role, me.to_string()));
            true
        }
    }
}

/// Forgets `platform`'s role: logged out or disconnected.
pub fn clear(platform: &str) {
    if let Ok(mut r) = ROLES.lock() {
        r.retain(|x| x.0 != platform);
    }
}

/// Our role on `platform`, and our user id there ("" when not logged in).
pub fn get(platform: &str) -> (Role, String) {
    ROLES
        .lock()
        .ok()
        .and_then(|r| r.iter().find(|x| x.0 == platform).map(|x| (x.1, x.2.clone())))
        .unwrap_or_default()
}

/// Our own message seen in chat: its badges say our role (someone modded us since we connected). Never
/// lowers the broadcaster's. True when it changed.
pub fn seen_self(platform: &str, me: &str, shown: Role) -> bool {
    let (now, id) = get(platform);
    if id != me || me.is_empty() || now == Role::Broadcaster || now == shown || now == Role::Anonymous {
        return false;
    }
    set(platform, shown, me)
}

/// The person a message action is aimed at.
#[derive(Clone, Copy)]
pub struct Target<'a> {
    pub user_id: &'a str,
    pub broadcaster: bool,
    pub moderator: bool,
}

/// What we may do to one message, given our role.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Allowed {
    pub reply: bool,
    pub delete: bool,
    pub pin: bool,
    pub timeout: bool,
    pub ban: bool,
}

/// What `me` (with id `my_id`) may do to a message by `t` on `platform`.
pub fn allowed(platform: &str, me: Role, my_id: &str, t: Target<'_>) -> Allowed {
    if me == Role::Anonymous {
        return Allowed::default();
    }
    let reply = true;
    if !me.moderates() {
        return Allowed {
            reply,
            ..Allowed::default()
        };
    }
    let mine = !my_id.is_empty() && t.user_id == my_id;
    let owner = me == Role::Broadcaster;
    // Another moderator is the owner's to deal with.
    let shielded = t.broadcaster || (t.moderator && !owner && !mine);
    let sanction = !t.user_id.is_empty() && !mine && !shielded;
    let has_mod_tools = matches!(platform, "twitch" | "kick" | "youtube");
    Allowed {
        reply,
        // Our own messages we may always take back, as a moderator.
        delete: matches!(platform, "twitch" | "kick") && (mine || !shielded),
        pin: platform == "twitch",
        timeout: has_mod_tools && sanction,
        ban: has_mod_tools && sanction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(user_id: &str, broadcaster: bool, moderator: bool) -> Target<'_> {
        Target {
            user_id,
            broadcaster,
            moderator,
        }
    }

    #[test]
    fn nobody_moderates_the_broadcaster() {
        for me in [Role::Moderator, Role::LeadModerator, Role::Broadcaster] {
            for p in ["twitch", "kick", "youtube"] {
                let a = allowed(p, me, "2", t("1", true, false));
                assert!(!a.ban && !a.timeout && !a.delete, "{p} {me:?}");
            }
        }
    }

    #[test]
    fn nobody_bans_themselves() {
        let a = allowed("twitch", Role::Broadcaster, "1", t("1", true, false));
        assert!(!a.ban && !a.timeout);
        let a = allowed("kick", Role::Moderator, "5", t("5", false, true));
        assert!(
            !a.ban && !a.timeout && a.delete,
            "a moderator takes back their own message"
        );
    }

    #[test]
    fn moderators_leave_each_other_to_the_owner() {
        let a = allowed("twitch", Role::Moderator, "2", t("3", false, true));
        assert_eq!(
            a,
            Allowed {
                reply: true,
                pin: true,
                ..Allowed::default()
            }
        );
        let a = allowed("twitch", Role::Broadcaster, "1", t("3", false, true));
        assert!(a.ban && a.timeout && a.delete);
    }

    #[test]
    fn viewers_and_vips_only_reply() {
        for me in [Role::Viewer, Role::Vip] {
            let a = allowed("twitch", me, "2", t("3", false, false));
            assert_eq!(
                a,
                Allowed {
                    reply: true,
                    ..Allowed::default()
                }
            );
        }
    }

    #[test]
    fn without_a_login_the_chat_is_read_only() {
        let a = allowed("kick", Role::Anonymous, "", t("3", false, false));
        assert_eq!(a, Allowed::default());
    }

    #[test]
    fn a_moderator_handles_viewers() {
        let a = allowed("youtube", Role::Moderator, "2", t("3", false, false));
        assert!(
            a.ban && a.timeout && !a.delete && !a.pin,
            "YouTube has no delete or pin here"
        );
        let a = allowed("tiktok", Role::Broadcaster, "1", t("3", false, false));
        assert!(!a.ban && !a.timeout, "TikTok has no moderation API");
    }

    #[test]
    fn our_own_badges_update_the_role() {
        set("youtube", Role::Viewer, "me");
        assert!(seen_self("youtube", "me", Role::Moderator));
        assert_eq!(get("youtube").0, Role::Moderator);
        assert!(
            !seen_self("youtube", "someone", Role::Viewer),
            "only our own messages count"
        );
        set("youtube", Role::Broadcaster, "me");
        assert!(!seen_self("youtube", "me", Role::Viewer), "the owner stays the owner");
        clear("youtube");
        assert_eq!(get("youtube").0, Role::Anonymous);
    }
}
