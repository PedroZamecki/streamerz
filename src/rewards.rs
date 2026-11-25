//! Reward counters and groups, kept by Streamerz as Streamer.bot keeps its own: how many times each
//! channel reward was redeemed, and by whom, and which rewards go together (turned on, off or paused
//! as one). A reward is `platform:id` (`twitch:5f1...`, `kick:01H...`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Book {
    /// Redemptions by reward.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, Counter>,
    /// Groups by name, each with its rewards. A reward is in one group at most.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, Vec<String>>,
    /// The Kick rewards Streamerz made (Kick doesn't say which an app may change).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub made: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Counter {
    pub count: u64,
    /// By the id of who redeemed it.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub users: BTreeMap<String, u64>,
}

/// A reward's key: its platform and id.
pub fn key(platform: &str, id: &str) -> String {
    format!("{platform}:{id}")
}

impl Book {
    pub fn is_empty(&self) -> bool {
        self.counters.is_empty() && self.groups.is_empty() && self.made.is_empty()
    }

    /// One redemption of `reward` by `user`: the reward's count and the user's, after it.
    pub fn redeemed(&mut self, reward: &str, user: &str) -> (u64, u64) {
        let counter = self.counters.entry(reward.to_owned()).or_default();
        counter.count += 1;
        let mine = if user.is_empty() {
            0
        } else {
            let n = counter.users.entry(user.to_owned()).or_default();
            *n += 1;
            *n
        };
        (counter.count, mine)
    }

    /// The reward's count (and the user's).
    pub fn count(&self, reward: &str, user: &str) -> (u64, u64) {
        self.counters
            .get(reward)
            .map_or((0, 0), |c| (c.count, c.users.get(user).copied().unwrap_or(0)))
    }

    /// Starts a reward's count again, or (`users`) every user's count of it.
    pub fn reset(&mut self, reward: &str, users: bool) {
        if let Some(c) = self.counters.get_mut(reward) {
            if users {
                c.users.clear();
            } else {
                c.count = 0;
            }
            if c.count == 0 && c.users.is_empty() {
                self.counters.remove(reward);
            }
        }
    }

    /// The group `reward` is in.
    pub fn group_of(&self, reward: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|(_, rewards)| rewards.iter().any(|r| r == reward))
            .map(|(name, _)| name.as_str())
    }

    /// Puts `reward` in the group `name` (by any case of an existing one), out of the one it was in;
    /// an empty name takes it out. A group left empty goes away.
    pub fn set_group(&mut self, reward: &str, name: &str) {
        for rewards in self.groups.values_mut() {
            rewards.retain(|r| r != reward);
        }
        let name = name.trim();
        if !name.is_empty() {
            let existing = self.groups.keys().find(|g| g.eq_ignore_ascii_case(name)).cloned();
            self.groups
                .entry(existing.unwrap_or_else(|| name.to_owned()))
                .or_default()
                .push(reward.to_owned());
        }
        self.groups.retain(|_, rewards| !rewards.is_empty());
    }

    /// The ids of `platform`'s rewards in the group `name` (any case).
    pub fn in_group(&self, platform: &str, name: &str) -> Vec<String> {
        let prefix = format!("{platform}:");
        self.groups
            .iter()
            .filter(|(g, _)| g.eq_ignore_ascii_case(name.trim()))
            .flat_map(|(_, rewards)| rewards.iter())
            .filter_map(|r| r.strip_prefix(&prefix).map(str::to_owned))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redemptions_count_by_reward_and_by_user_and_reset_apart() {
        let mut book = Book::default();
        assert_eq!(book.redeemed("twitch:a", "5"), (1, 1));
        assert_eq!(book.redeemed("twitch:a", "5"), (2, 2));
        assert_eq!(book.redeemed("twitch:a", "6"), (3, 1));
        assert_eq!(book.redeemed("kick:a", "5"), (1, 1));
        book.reset("twitch:a", true);
        assert_eq!(book.count("twitch:a", "5"), (3, 0));
        book.reset("twitch:a", false);
        assert_eq!(book.count("twitch:a", "5"), (0, 0));
        assert!(!book.counters.contains_key("twitch:a"), "nothing left to keep");
        assert_eq!(book.count("kick:a", "5"), (1, 1));
    }

    #[test]
    fn a_reward_is_in_one_group_and_names_match_in_any_case() {
        let mut book = Book::default();
        book.set_group("twitch:a", "Games");
        book.set_group("kick:b", "games");
        book.set_group("twitch:c", "Chat");
        assert_eq!(book.in_group("twitch", "GAMES"), vec!["a".to_string()]);
        assert_eq!(book.in_group("kick", "Games"), vec!["b".to_string()]);
        book.set_group("twitch:a", "Chat");
        assert_eq!(book.group_of("twitch:a"), Some("Chat"));
        assert!(book.in_group("twitch", "Games").is_empty());
        book.set_group("kick:b", "");
        assert!(!book.groups.contains_key("Games"), "an empty group goes away");
        assert!(book.is_empty() == book.groups.is_empty());
    }
}
