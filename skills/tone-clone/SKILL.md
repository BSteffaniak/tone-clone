---
name: tone-clone
description: Query the user's real writing samples from the tone-clone database to calibrate voice and tone before drafting GitHub comments, PR replies, or annotations.
allowed-tools: Bash(sqlite3:*), Bash(tone-clone:*)
---

# tone-clone

Query your authentic writing from a local SQLite database to calibrate AI-generated text to match your real voice.

## Database

**Path:** `~/.local/share/tone-clone/tone-clone.db`

If the database doesn't exist or is empty, run `tone-clone scrape` first to populate it.

## Schema

### `posts` table

| Column | Type | Description |
|---|---|---|
| id | INTEGER | Primary key |
| source_id | INTEGER | FK to sources |
| external_id | TEXT | Platform-specific ID (e.g., GitHub node ID) |
| post_type | TEXT | One of: `pr_comment`, `issue_comment`, `pr_body`, `issue_body`, `review_comment`, `review_body` |
| body | TEXT | The actual post content |
| url | TEXT | Link to the post |
| repo | TEXT | `owner/repo` |
| created_at | TEXT | Original post date (ISO-8601) |
| likely_ai | INTEGER | `0` = authentic, `1` = likely AI-generated |
| scraped_at | TEXT | When it was scraped |

### `posts_fts` (FTS5 virtual table)

Full-text search index over `body`, `post_type`, and `repo`.

## Generate command

The fastest way to get voice calibration context. Analyzes authentic posts and produces a voice profile (stats) plus curated examples.

### On-the-fly (stdout)

Use `--stdout` to pipe directly into your context. Best for skills that need topic-specific or type-specific calibration:

```bash
# voice profile + examples for review comments about error handling
tone-clone generate --stdout --type review_comment --topic "error handling" --limit 5

# full profile across all types
tone-clone generate --stdout

# just issue comments, 3 examples
tone-clone generate --stdout --type issue_comment --limit 3
```

### To files

Writes to `~/.local/share/tone-clone/profiles/` by default:

```bash
tone-clone generate                          # all types, 10 examples each
tone-clone generate --type pr_comment        # just PR comments
tone-clone generate --output-dir ./profiles  # custom output dir
```

Generated files:
- `voice-profile.md` -- word counts, sentence stats, style patterns (lowercase rate, contraction rate, question rate), punctuation inventory, common ngrams, per-type breakdown
- `examples-<type>.md` -- curated examples per post type, selected for variety across repos/dates/lengths

### Options

| Flag | Default | Description |
|---|---|---|
| `--stdout` | off | Print to stdout instead of writing files |
| `--type <TYPE>` | all | Filter to a specific post_type |
| `--topic <TOPIC>` | none | FTS search to focus examples on a topic |
| `--limit <N>` | 10 | Max examples per type |
| `--no-exclude-ai` | off | Include AI-flagged posts (excluded by default) |
| `--source-id <N>` | all | Filter to a specific source |
| `--output-dir <PATH>` | `~/.local/share/tone-clone/profiles/` | Output directory for file mode |

## Raw queries

For cases where you need more control than `generate` provides:

### Sample random authentic posts by type

```bash
sqlite3 ~/.local/share/tone-clone/tone-clone.db \
  "SELECT body FROM posts WHERE likely_ai = 0 AND post_type = 'pr_comment' ORDER BY RANDOM() LIMIT 5"
```

### Full-text search

```bash
sqlite3 ~/.local/share/tone-clone/tone-clone.db \
  "SELECT post_type, body FROM posts JOIN posts_fts ON posts_fts.rowid = posts.id WHERE likely_ai = 0 AND posts_fts MATCH 'error handling' LIMIT 5"
```

### Stats

```bash
sqlite3 ~/.local/share/tone-clone/tone-clone.db \
  "SELECT post_type, COUNT(*) as n FROM posts WHERE likely_ai = 0 GROUP BY post_type ORDER BY n DESC"
```

### Sample by repo

```bash
sqlite3 ~/.local/share/tone-clone/tone-clone.db \
  "SELECT body FROM posts WHERE likely_ai = 0 AND repo = 'MoosicBox/MoosicBox' ORDER BY RANDOM() LIMIT 5"
```

## Other CLI commands

```bash
tone-clone sources list          # show configured sources
tone-clone scrape                # scrape all sources
tone-clone stats                 # show post counts by type/source
tone-clone query "search terms"  # FTS search from the command line
```

## How to use samples

1. Before drafting posted text (review comments, PR replies, annotations), run `tone-clone generate --stdout` with appropriate `--type` and `--topic` flags to get voice context.
2. Study the voice profile stats and examples for: sentence length, punctuation style, capitalization, level of formality, use of contractions, how links and code are referenced.
3. Write new text that matches those patterns naturally. Don't quote or cite the samples.
4. Always use the default (exclude AI posts) to only calibrate against authentic pre-AI writing.
