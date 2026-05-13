#!/usr/bin/env python3
"""
Convert the local cheats/ directory tree into a SQLite database (cheats.db).

Directory structure expected:
  cheats/<TITLE_ID>/
      cheats/<BUILD_ID>.txt   ← cheat code content (one file per build id)
      <Game Name>.txt          ← optional credits file (any .txt not inside cheats/)

Output: cheats.db (same directory as this script)
"""

import json
import os
import sqlite3

CHEATS_DIR = os.path.join(os.path.dirname(__file__), "cheats")
DB_PATH = os.path.join(os.path.dirname(__file__), "cheats.db")


def collect_credits(title_dir: str) -> str:
    """Return the content of the first top-level .txt file (credits) in a title dir."""
    try:
        for entry in os.scandir(title_dir):
            if entry.is_file() and entry.name.lower().endswith(".txt"):
                with open(entry.path, "r", encoding="utf-8", errors="replace") as fh:
                    return fh.read().strip()
    except OSError:
        pass
    return ""


def main() -> None:
    if os.path.exists(DB_PATH):
        os.remove(DB_PATH)
        print(f"Removed existing {DB_PATH}")

    con = sqlite3.connect(DB_PATH)
    cur = con.cursor()

    cur.execute("""
        CREATE TABLE cheats (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            title_id TEXT    NOT NULL,
            build_id TEXT    NOT NULL,
            content  TEXT    NOT NULL,
            credits  TEXT    NOT NULL DEFAULT ''
        )
    """)
    cur.execute("CREATE INDEX idx_cheats_title_id ON cheats(title_id)")

    con.execute("PRAGMA journal_mode=WAL")
    con.execute("PRAGMA synchronous=OFF")

    rows_inserted = 0
    titles_processed = 0

    if not os.path.isdir(CHEATS_DIR):
        print(f"ERROR: cheats directory not found at {CHEATS_DIR}")
        return

    for title_entry in sorted(os.scandir(CHEATS_DIR), key=lambda e: e.name):
        if not title_entry.is_dir():
            continue

        title_id = title_entry.name.upper()
        title_dir = title_entry.path
        cheats_subdir = os.path.join(title_dir, "cheats")

        if not os.path.isdir(cheats_subdir):
            continue

        credits = collect_credits(title_dir)
        titles_processed += 1

        for cheat_file in sorted(os.scandir(cheats_subdir), key=lambda e: e.name):
            if not cheat_file.is_file():
                continue
            name = cheat_file.name
            if not name.lower().endswith(".txt"):
                continue

            build_id = os.path.splitext(name)[0].upper()

            with open(cheat_file.path, "r", encoding="utf-8", errors="replace") as fh:
                content = fh.read()

            cur.execute(
                "INSERT INTO cheats (title_id, build_id, content, credits) VALUES (?, ?, ?, ?)",
                (title_id, build_id, content, credits),
            )
            rows_inserted += 1

    con.commit()
    con.close()

    size_kb = os.path.getsize(DB_PATH) / 1024
    print(f"Done. {titles_processed} titles, {rows_inserted} cheat entries → {DB_PATH} ({size_kb:.1f} KB)")


if __name__ == "__main__":
    main()
