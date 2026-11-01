# Kick / Channel (sub-actions)

Changes the title and the category of the Kick channel.

## Set Channel Title

- Kind: sub-action
- Code: 35030
- Type: KickSetChannelTitle
- Category: Kick/Channel
- Needs: a Kick broadcaster login

Sets the stream title on Kick. Variables are filled in first.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Title | `title` | The new title. |

### Arguments

- `titleSuccess`: true when Kick accepted the new title.

### Notes

- The change is made with the broadcaster account. The bot account is never used here.

## Set Channel Category

- Kind: sub-action
- Code: 35031
- Type: KickSetChannelCategory
- Category: Kick/Channel
- Needs: a Kick broadcaster login

Sets the category of the stream on Kick. The category is found by its ID when one is given, else by its name. For a name, the exact match wins, else the first search result is used.

### Fields

| Field | Key | What it holds |
| --- | --- | --- |
| Source | `source` | A whole number. |
| Category name | `categoryName` | The name of the category. |
| Category ID | `categoryId` | The ID of the category, a number. |

### Arguments

- `category.success`: true when the category was set.
- `category.id`, `category.name` and `category.thumbnail`: the category that was found, set when a category was found.

### Notes

- `source` is saved but not read yet.
- The change is made with the broadcaster account. The bot account is never used here.
