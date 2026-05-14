# Community Cheats DB — Implementation Plan

The goal is a shared, living `community_cheats.db` that grows automatically
as users fetch cheats from the Cheatslips API. Cheatslips already vets
submissions, so those cheats are trusted and pushed automatically.
User-created custom cheats stay local only.

---

## How it works end-to-end

```
User fetches from Cheatslips API
         │
         ▼
Saved to local cheats.db           (already implemented)
  • code_hash dedup prevents       (already implemented)
    storing identical codes twice
         │
         ▼ (only newly inserted rows)
App POSTs a JSON file to
eden-cheats-submissions repo
via GitHub API (bundled PAT)
         │
         ▼
GitHub Action runs on schedule
  • Reads all pending JSON files
  • Merges into community_cheats.db
    (same code_hash NOT EXISTS check)
  • Publishes new GitHub Release
         │
         ▼
Other users' apps check for
a newer release on startup
  • Download & merge into local DB
```

---

## Step 1 — One-time GitHub setup (manual)

### 1a. Create the submissions repo

Create a new public repo: `ChrisA95G/eden-cheats-submissions`

Add a `README.md` explaining it is a machine-managed submissions inbox —
human PRs are not needed.

### 1b. Create a fine-grained GitHub PAT

Go to GitHub → Settings → Developer settings → Fine-grained tokens → Generate new token.

Settings:
- **Resource owner:** ChrisA95G
- **Repository access:** Only `eden-cheats-submissions`
- **Permissions → Contents:** Read and Write

Copy the token — it gets added to the app as a Rust constant (see Step 3).

Worst-case exposure: someone decompiles the app and spams the submissions
inbox. They cannot touch any other repo. The merge Action validates before
inserting anything into the community DB.

### 1c. Create the initial community_cheats.db release

Export a copy of the current bundled `cheats.db`, rename it
`community_cheats.db`, and publish it as a GitHub Release on the main repo:

- Tag: `db-v2026-05-14` (date-based, update on each new release)
- Release title: `Community Cheats DB — 2026-05-14`
- Attach `community_cheats.db` as a release asset

### 1d. Create the merge GitHub Action

In the main repo, add `.github/workflows/merge-submissions.yml`.

The Action:
1. Triggers on a schedule (e.g. daily at 03:00 UTC) and on `workflow_dispatch`
2. Checks out both repos
3. Runs a Python script (`scripts/merge_submissions.py`) that:
   - Opens `community_cheats.db`
   - Reads every `.json` file from the submissions repo
   - Inserts new cheats using the same `code_hash` NOT EXISTS logic
   - Deletes processed submission files
4. Publishes a new release with the updated `community_cheats.db` if anything changed

The merge script schema for submission JSON files:
```json
{
  "title_id": "0100E95004038000",
  "build_id": "D007651BC7C6A51E",
  "content": "[Cheat Name]\n580F0000 ...",
  "credits": "Author",
  "code_hash": "normalized opcode fingerprint",
  "submitted_at": "2026-05-14T07:15:00Z",
  "app_version": "0.1.0"
}
```

---

## Step 2 — cheats.db schema addition

Add a `submitted_to_community` column so the app never re-submits a cheat
it already pushed, even across restarts.

Migration (add to `migrate_cheats_db` in `cheatslips.rs`):
```sql
ALTER TABLE cheats ADD COLUMN submitted_to_community INTEGER NOT NULL DEFAULT 0
```

---

## Step 3 — New Rust commands (`cheatslips.rs`)

### `check_db_update() -> Result<Option<DbRelease>, String>`

Hits the GitHub releases API (no auth needed for public repos):
```
GET https://api.github.com/repos/ChrisA95G/eden-cheats-manager/releases/latest
```

Parses the release tag (e.g. `db-v2026-05-14`) and compares it to a version
stored in the local `cheats.db` (or a small `db_version.txt` in the app data
dir). Returns `None` if already up to date, or `Some(DbRelease)` with the
download URL if a newer release exists.

```rust
pub struct DbRelease {
    pub tag: String,
    pub download_url: String,
    pub published_at: String,
}
```

### `download_db_update(download_url: String) -> Result<usize, String>`

Downloads `community_cheats.db` from the release asset URL and merges its
rows into the local `cheats.db` using `code_hash` NOT EXISTS — safe to run
repeatedly. Returns the count of newly merged rows. Saves the release tag
to mark the local DB as up to date.

### `submit_cheats_to_community(cheats: Vec<SubmissionPayload>) -> Result<(), String>`

Called automatically (fire-and-forget) after a successful Cheatslips fetch.
Only receives cheats where the local insert succeeded AND
`submitted_to_community = 0`.

For each cheat, creates a file in `ChrisA95G/eden-cheats-submissions` via
the GitHub Contents API:
```
PUT https://api.github.com/repos/ChrisA95G/eden-cheats-submissions/contents/{title_id}_{build_id}_{timestamp}.json
```
Authorization: `Bearer <BUNDLED_PAT>`

On success, sets `submitted_to_community = 1` for that row locally.
Failures are silently logged — the submission is not critical.

---

## Step 4 — Frontend changes (`CheatPanel.svelte`)

### DB update banner

On app startup (in `+page.svelte`), call `check_db_update` once silently.
If an update is available, show a dismissible banner at the top of the cheat
panel (or sidebar):

```
↓ Community DB update available (2026-05-14)   [ Update ]  [ Dismiss ]
```

Clicking Update calls `download_db_update`, shows a progress/count message,
then refreshes the cheat list for the current game.

### No UI needed for submission

`submit_cheats_to_community` fires in the background after every successful
Cheatslips fetch. No button, no spinner, no message — it just happens.

---

## Step 5 — Scripts (`scripts/`)

### `merge_submissions.py`

Used by the GitHub Action. Takes:
- `--community-db path/to/community_cheats.db`
- `--submissions-dir path/to/json/files/`

For each JSON file:
1. Compute `code_fingerprint(content)` (strip headers, normalize, lowercase)
2. Check NOT EXISTS `(title_id, build_id, code_hash)`
3. Insert if new, skip if duplicate
4. Delete the processed JSON file from the submissions dir

Outputs a count of newly inserted cheats.

---

## Deduplication summary

| Layer | Where | What it checks |
|---|---|---|
| 1 | Local DB on fetch | `(title_id, build_id, code_hash)` NOT EXISTS |
| 2 | App before submit | Only submit rows where local insert succeeded + `submitted_to_community = 0` |
| 3 | Merge Action | Same `code_hash` NOT EXISTS before inserting into community DB |

---

## Implementation order

- [ ] **Step 1a** — Create `eden-cheats-submissions` repo on GitHub
- [ ] **Step 1b** — Create fine-grained PAT, note it down
- [ ] **Step 1c** — Publish initial `community_cheats.db` release
- [ ] **Step 2** — Add `submitted_to_community` column migration
- [ ] **Step 3** — Implement `check_db_update`, `download_db_update`, `submit_cheats_to_community` in Rust
- [ ] **Step 4** — Add update banner to frontend
- [ ] **Step 1d** — Write `merge_submissions.py` and the GitHub Action workflow
- [ ] Test full round-trip: fetch → submit → merge → release → download
