# Integrations / Pronouns (sub-actions)

Looks up the pronouns a Twitch user has set, so an action can use them in its text.

## Add Pronouns for User

- Kind: sub-action
- Code: 15001
- Type: PronounsAddForUser
- Category: Integrations/Pronouns
- Needs: nothing (it reads a public API, no login)

Looks up the pronouns of a user on the alejo.io Twitch Chat Pronouns API and adds them to the arguments as variables, such as `pronoun` and `pronounSubject`. The sub-actions after it can use them.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| User Login | `userLogin` | The Twitch login to look up. Variables are filled in first. Empty: the `userLogin`, `user` or `userName` argument of the action. |
| Prefix | `prefix` | Text put in front of every variable name, with a dot: a prefix of `target` gives `target.pronoun`. Empty: no prefix. |

### Arguments

With an empty prefix, these are added. With a prefix, each name gets the prefix and a dot in front.

- `userFound`: true when the API returned pronouns for the user.
- `pronouns`: the display form, such as `(She/Her)`.
- `pronoun`, `pronounSubject`, `pronounObject`, `pronounPossessive`, `pronounPronoun`, `pronounReflexive`:
  the pronoun forms, capitalized (for example `They`, `Them`, `Their`).
- `pronounSubjectLower`, `pronounObjectLower`, `pronounPossessiveLower`, `pronounPronounLower`,
  `pronounReflexiveLower`: the same forms in lower case.
- `pronounPastTense`, `pronounPastTenseLower`, `pronounCurrentTense`, `pronounCurrentTenseLower`: the verb
  forms that go with the pronouns (for example `Is` and `Was`).
- `pronounPluralLower`: the plural form in lower case, empty when there is none.
- `pronounLastCached`: when the values were set, as an ISO date and time.

### Notes

- The lookup is cached in memory for 15 minutes per login, so the same user is not asked again right away.
- When the user has no pronouns, the lookup fails, or the login is empty, `userFound` is false and the values
  are the They/Them ones (`pronouns` is `(They/Them)`, `pronoun` is `Theirs`).
- The login is lower-cased before the lookup.
