# Translating Streamerz

You don't need to code. A language is one file, and you can do it from GitHub's website.

## Add a language

1. [Fork the repository](https://github.com/PedroZamecki/streamerz/fork). Pick your code: language and country,
   like `es_ES` (Spanish, Spain), `fr_FR`, `de_DE`, `ja_JP`.
2. Copy `assets/locales/en_US.json` to `assets/locales/<code>.json`
   (or run `python3 dev/locale.py new es_ES "Español (España)"`, which does it for you).
3. Translate the **values** (the text on the right). Never change the keys on the left.
4. Set `locale.name` to your language's own name ("Español (España)"): it is what Settings shows.
5. Add your row to the table in the [README](README.md#languages): language, code, and your name with
   your GitHub link. That is your credit.
6. Open a pull request. CI runs `python3 dev/locale.py check` and says what is missing or wrong.

Streamerz finds the file by itself when it is built: there is no code to touch.

## Fix a word

Open the file of your language, change the text, open a pull request. Add yourself to the table if you are
not there yet.

## Rules

- Keep every `{placeholder}` as it is, in any position: `"{user} joined"` may become `"{user} entrou"`.
- `&` underlines the next letter in menus: keep it where it reads well, or write `&&` for a real `&`.
- `@` is escaped as `@@` in some labels: keep what the English has.
- Keep it short, plain and natural, as a person would say it. The window is small. No em dashes, no
  decorative symbols (icons are drawn by the app).
- Names of products stay as they are (Twitch, Kick, YouTube, TikTok, OBS, Streamer.bot).
- Words that Streamer.bot users know (action, trigger, sub-action, queue) should match what its own
  translations or community use, when there is one.

## Check your work

```sh
python3 dev/locale.py check es_ES     # one language
python3 dev/locale.py check           # all of them
```

It lists keys that are missing or extra, placeholders that changed, and a language missing from the README.
To see it on screen, build Streamerz (see [CONTRIBUTING.md](CONTRIBUTING.md)) and pick your language in
Settings.

## Thank you

Everyone who translated is in the [README](README.md#languages) table, and in the release notes of the version
that brought their language.
